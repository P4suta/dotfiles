extern crate alloc;
use self::alloc::vec::Vec;

/// The `text` attribute as `git check-attr` resolves it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Text {
    Set,
    Unset,
    Auto,
    Unspecified,
}

/// The `eol` attribute as `git check-attr` resolves it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Eol {
    Lf,
    Crlf,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Attributes {
    pub text: Text,
    pub eol: Eol,
    pub binary: bool,
}

/// The statistics Git's `gather_stats` collects for `text=auto` detection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Content {
    pub crlf: bool,
    pub lone_cr: bool,
    pub nul: bool,
    pub printable: usize,
    pub nonprintable: usize,
}

/// Whether Git's index entry for the path holds text with CRLF, as `git ls-files --eol` reports `i/crlf` or `i/mixed`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Index {
    CrlfText,
    Other,
}

/// How Git stages the bytes.
/// `git add` keeps CRLF already in the index under `text=auto`, and `git add --renormalize` doesn't.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Staging {
    Add,
    Renormalize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Normalize,
    Keep,
}

/// A path whose attributes ask for CRLF, no conversion, or binary handling keeps its bytes.
fn declared_exempt(attributes: Attributes) -> bool {
    attributes.eol == Eol::Crlf || attributes.text == Text::Unset || attributes.binary
}

/// Explicit `text`, or `eol=lf` without a `text` value, makes Git convert without content detection.
fn forces_text(attributes: Attributes) -> bool {
    attributes.text == Text::Set
        || (attributes.text == Text::Unspecified && attributes.eol == Eol::Lf)
}

/// Git's `convert_is_binary` admits one nonprintable byte per this many printable ones, as a shift.
const PRINTABLE_PER_NONPRINTABLE_SHIFT: u32 = 7;

/// Control bytes that Git's `gather_stats` counts as printable.
const BACKSPACE: u8 = 0x08;
const FORM_FEED: u8 = 0x0c;
const ESCAPE: u8 = 0x1b;
const DELETE: u8 = 0x7f;
const FIRST_CONTROL: u8 = 0x01;
const LAST_CONTROL: u8 = 0x1f;
/// The `Ctrl-Z` end-of-file byte, which Git doesn't count when it ends the content.
const END_OF_FILE: u8 = 0x1a;

/// Git's `convert_is_binary`: a lone CR, a NUL, or more than one nonprintable byte per 128 printable ones.
#[must_use]
pub fn auto_binary(content: Content) -> bool {
    content.lone_cr
        || content.nul
        || (content.printable >> PRINTABLE_PER_NONPRINTABLE_SHIFT) < content.nonprintable
}

/// Git's `has_crlf_in_index` rule: content detection keeps a path whose index entry already holds CRLF text.
fn index_keeps(staging: Staging, index: Index) -> bool {
    staging == Staging::Add && index == Index::CrlfText
}

#[must_use]
pub fn action(attributes: Attributes, staging: Staging, index: Index, content: Content) -> Action {
    if declared_exempt(attributes) || !content.crlf {
        Action::Keep
    } else if forces_text(attributes) {
        Action::Normalize
    } else if auto_binary(content) || index_keeps(staging, index) {
        Action::Keep
    } else {
        Action::Normalize
    }
}

/// Classifies bytes as Git's `gather_stats` does.
/// LF and both bytes of a CRLF pair count as neither printable nor nonprintable.
#[must_use]
pub fn content(bytes: &[u8]) -> Content {
    let mut found = Content {
        crlf: false,
        lone_cr: false,
        nul: false,
        printable: 0,
        nonprintable: 0,
    };
    // The line feed of a CRLF pair counts as nothing, like every line feed, so the pair needs no skip.
    let mut rest = bytes;
    while let [byte, tail @ ..] = rest {
        let byte = *byte;
        rest = tail;
        match byte {
            b'\r' if tail.first() == Some(&b'\n') => found.crlf = true,
            b'\r' => found.lone_cr = true,
            b'\n' => {}
            BACKSPACE | b'\t' | FORM_FEED | ESCAPE => found.printable += 1,
            0 => {
                found.nul = true;
                found.nonprintable += 1;
            }
            DELETE | FIRST_CONTROL..=LAST_CONTROL => found.nonprintable += 1,
            _ => found.printable += 1,
        }
    }
    if bytes.last() == Some(&END_OF_FILE) {
        found.nonprintable -= 1;
    }
    found
}

/// Whether conversion keeps the byte at `index`: every byte except a carriage return that precedes a line feed.
#[must_use]
pub fn kept(bytes: &[u8], index: usize) -> bool {
    !matches!(bytes.get(index..), Some([b'\r', b'\n', ..]))
}

/// Removes each carriage return that precedes a line feed, as Git does when it normalizes text.
///
/// Exempt as no-decision: it keeps the bytes the proved `kept` admits, and Kani can't verify the allocation of its result.
/// Evidence: `crates/rules/src/eol_rules.rs::tests::normalizing_drops_one_carriage_return_per_pair_and_keeps_every_line_feed`.
/// Reconsider when it gains a branch.
#[must_use]
pub fn to_lf(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .enumerate()
        .filter(|&(index, _)| kept(bytes, index))
        .map(|(_, &byte)| byte)
        .collect()
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const TEXTS: [Text; 4] = [Text::Set, Text::Unset, Text::Auto, Text::Unspecified];
    pub const EOLS: [Eol; 3] = [Eol::Lf, Eol::Crlf, Eol::Unspecified];

    /// Git never rewrites declared line endings or content without CRLF, a forced text conversion always normalizes, and content detection keeps binary data and CRLF the index already holds.
    pub fn spec_action(attributes: Attributes, staging: Staging, index: Index, content: Content) {
        let decided = action(attributes, staging, index, content);
        let declared =
            attributes.eol == Eol::Crlf || attributes.text == Text::Unset || attributes.binary;
        let forced = attributes.text == Text::Set
            || (attributes.text == Text::Unspecified && attributes.eol == Eol::Lf);
        let binary =
            content.lone_cr || content.nul || content.printable / 128 < content.nonprintable;
        let indexed = staging == Staging::Add && index == Index::CrlfText;
        let normalizes = content.crlf && !declared && (forced || (!binary && !indexed));
        assert_eq!(decided == Action::Normalize, normalizes);
        assert_eq!(auto_binary(content), binary);
    }

    /// The statistics count every byte but a line feed and both bytes of a CRLF pair, and drop a final `Ctrl-Z` from the nonprintable ones.
    pub fn spec_content(bytes: &[u8]) {
        let mut expected = Content {
            crlf: false,
            lone_cr: false,
            nul: false,
            printable: 0,
            nonprintable: 0,
        };
        let mut index = 0;
        while let Some(&byte) = bytes.get(index) {
            let pair = byte == b'\r' && bytes.get(index + 1) == Some(&b'\n');
            assert_eq!(kept(bytes, index), !pair);
            if pair {
                expected.crlf = true;
                index += 2;
                continue;
            }
            match byte {
                b'\r' => expected.lone_cr = true,
                b'\n' => {}
                0 => {
                    expected.nul = true;
                    expected.nonprintable += 1;
                }
                0x08 | b'\t' | 0x0c | 0x1b => expected.printable += 1,
                _ if byte < 0x20 || byte == 0x7f => expected.nonprintable += 1,
                _ => expected.printable += 1,
            }
            index += 1;
        }
        if bytes.last() == Some(&0x1a) {
            expected.nonprintable -= 1;
        }
        assert_eq!(content(bytes), expected);
        assert!(kept(bytes, bytes.len()));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn declared_line_endings_are_never_rewritten_and_detection_keeps_binary_and_indexed_crlf() {
        let attributes = Attributes {
            text: TEXTS[usize::from(kani::any::<u8>() % 4)],
            eol: EOLS[usize::from(kani::any::<u8>() % 3)],
            binary: kani::any(),
        };
        let content = Content {
            crlf: kani::any(),
            lone_cr: kani::any(),
            nul: kani::any(),
            printable: kani::any(),
            nonprintable: kani::any(),
        };
        let staging = if kani::any() {
            Staging::Add
        } else {
            Staging::Renormalize
        };
        let index = if kani::any() {
            Index::CrlfText
        } else {
            Index::Other
        };
        spec_action(attributes, staging, index, content);
        let decided = action(attributes, staging, index, content);
        kani::cover!(decided == Action::Normalize && attributes.text == Text::Auto);
        kani::cover!(decided == Action::Normalize && content.nul);
        kani::cover!(decided == Action::Keep && content.crlf && !content.nul && !content.lone_cr);
        kani::cover!(
            decided == Action::Normalize
                && attributes.text == Text::Auto
                && index == Index::CrlfText
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(5)]
    fn detection_follows_git_byte_classes_and_conversion_removes_only_carriage_returns_before_line_feeds()
     {
        let input: [u8; 4] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= input.len());
        spec_content(&input[..length]);
        let found = content(&input[..length]);
        let binary = auto_binary(found);
        kani::cover!(binary & !found.lone_cr & !found.nul);
        kani::cover!(!binary & found.crlf & (length == 4) & (input[3] == END_OF_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every attribute and staging")]
    fn every_attribute_staging_and_boundary_content_meets_the_action_specification() {
        for text in proofs::TEXTS {
            for eol in proofs::EOLS {
                for [binary, crlf, lone_cr, nul, add, crlf_index] in crate::domain::flags() {
                    for printable in [0, 127, 128, 255, 256, usize::MAX] {
                        for nonprintable in [0, 1, 2, usize::MAX] {
                            proofs::spec_action(
                                Attributes { text, eol, binary },
                                if add {
                                    Staging::Add
                                } else {
                                    Staging::Renormalize
                                },
                                if crlf_index {
                                    Index::CrlfText
                                } else {
                                    Index::Other
                                },
                                Content {
                                    crlf,
                                    lone_cr,
                                    nul,
                                    printable,
                                    nonprintable,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every text")]
    fn every_short_text_over_each_byte_class_meets_the_content_specification() {
        for bytes in crate::domain::words(b"\0\x01\x08\t\n\x0c\r\x1a\x1b\x1f a\x7f\x80", 4) {
            proofs::spec_content(&bytes);
        }
    }

    #[test]
    fn normalizing_drops_one_carriage_return_per_pair_and_keeps_every_line_feed() {
        assert_eq!(to_lf(b"a\r\nb\r\n"), b"a\nb\n");
        assert_eq!(to_lf(b"\r\r\n"), b"\r\n");
        assert_eq!(to_lf(b"a\rb\n"), b"a\rb\n");
        assert_eq!(to_lf(b"\r"), b"\r");
        assert_eq!(to_lf(b""), b"");
    }
}
