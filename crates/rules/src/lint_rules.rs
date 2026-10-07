//! The text encoding every tracked and rendered file keeps, and the shrink-only baselines of the lint stages.

/// What keeps a text file from UTF-8 without a byte order mark that ends with a newline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fault {
    ByteOrderMark,
    NotUtf8,
    NoFinalNewline,
}

impl Fault {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::ByteOrderMark => "byte-order-mark",
            Self::NotUtf8 => "not-utf-8",
            Self::NoFinalNewline => "no-final-newline",
        }
    }
}

/// The UTF-8 encoding forms of RFC 3629: each lead byte's mask, marker, payload bits, sequence length, and least scalar value.
const ASCII_END: u8 = 0x80;
const TWO_MASK: u8 = 0xE0;
const TWO_MARKER: u8 = 0xC0;
const TWO_PAYLOAD: u8 = 0x1F;
const TWO_LENGTH: usize = 2;
const TWO_LEAST: u32 = 0x80;
const THREE_MASK: u8 = 0xF0;
const THREE_MARKER: u8 = 0xE0;
const THREE_PAYLOAD: u8 = 0x0F;
const THREE_LENGTH: usize = 3;
const THREE_LEAST: u32 = 0x800;
const FOUR_MASK: u8 = 0xF8;
const FOUR_MARKER: u8 = 0xF0;
const FOUR_PAYLOAD: u8 = 0x07;
const FOUR_LENGTH: usize = 4;
const FOUR_LEAST: u32 = 0x1_0000;
const CONTINUATION_MASK: u8 = 0xC0;
const CONTINUATION_MARKER: u8 = 0x80;
const CONTINUATION_PAYLOAD: u8 = 0x3F;
const CONTINUATION_BITS: u32 = 6;
const MAX_SCALAR: u32 = 0x10_FFFF;
const FIRST_SURROGATE: u32 = 0xD800;
const LAST_SURROGATE: u32 = 0xDFFF;

/// The UTF-8 byte order mark.
const MARK_FIRST: u8 = 0xEF;
const MARK_SECOND: u8 = 0xBB;
const MARK_THIRD: u8 = 0xBF;

/// Whether `bytes` hold well-formed UTF-8: shortest forms only, no surrogates, and nothing past U+10FFFF.
#[must_use]
pub fn is_utf8(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    while let [lead, tail @ ..] = rest {
        let lead = *lead;
        let (length, least, mut point) = if lead < ASCII_END {
            (1, 0, u32::from(lead))
        } else if lead & TWO_MASK == TWO_MARKER {
            (TWO_LENGTH, TWO_LEAST, u32::from(lead & TWO_PAYLOAD))
        } else if lead & THREE_MASK == THREE_MARKER {
            (THREE_LENGTH, THREE_LEAST, u32::from(lead & THREE_PAYLOAD))
        } else if lead & FOUR_MASK == FOUR_MARKER {
            (FOUR_LENGTH, FOUR_LEAST, u32::from(lead & FOUR_PAYLOAD))
        } else {
            return false;
        };
        let (Some(mut continuation), Some(after)) =
            (tail.get(..length - 1), tail.get(length - 1..))
        else {
            return false;
        };
        while let [next, others @ ..] = continuation {
            if next & CONTINUATION_MASK != CONTINUATION_MARKER {
                return false;
            }
            point = (point << CONTINUATION_BITS) + u32::from(next & CONTINUATION_PAYLOAD);
            continuation = others;
        }
        if point < least
            || point > MAX_SCALAR
            || (FIRST_SURROGATE..=LAST_SURROGATE).contains(&point)
        {
            return false;
        }
        rest = after;
    }
    true
}

/// The first fault of a text file's bytes, or `None` for an empty file or one that keeps the encoding.
#[must_use]
pub fn fault(bytes: &[u8]) -> Option<Fault> {
    if matches!(bytes, [MARK_FIRST, MARK_SECOND, MARK_THIRD, ..]) {
        Some(Fault::ByteOrderMark)
    } else if !is_utf8(bytes) {
        Some(Fault::NotUtf8)
    } else if bytes.last().is_some_and(|&last| last != b'\n') {
        Some(Fault::NoFinalNewline)
    } else {
        None
    }
}

/// How the findings of one rule in one path compare with the count the baseline admits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Standing {
    /// The path holds exactly what the baseline admits.
    Within,
    /// The path holds fewer findings, so the baseline must shrink to keep them from coming back.
    Shrunk,
    /// The path holds a finding the baseline doesn't admit.
    Exceeded,
}

#[must_use]
pub const fn standing(admitted: u32, found: u32) -> Standing {
    if found > admitted {
        Standing::Exceeded
    } else if found < admitted {
        Standing::Shrunk
    } else {
        Standing::Within
    }
}

/// Whether a baseline edit only removes or lowers what it admits, from `before` at the reference to `after` in the change.
#[must_use]
pub const fn shrinks(before: u32, after: u32) -> bool {
    after <= before
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// The validator accepts the encoding of every scalar value, which the caller builds by RFC 3629.
    pub fn spec_scalar(encoded: &[u8]) {
        assert!(is_utf8(encoded));
    }

    /// The validator refuses a text whose first byte can't lead any encoding.
    pub fn spec_impossible_lead(bytes: &[u8]) {
        let impossible = bytes.first().is_some_and(|&lead| {
            lead == 0xC0 || lead == 0xC1 || lead >= 0xF5 || lead & 0xC0 == 0x80
        });
        assert!(!impossible || !is_utf8(bytes));
    }

    /// A file passes only as UTF-8 without a mark that ends with a newline, and its first fault comes in that order.
    pub fn spec_fault(text: &[u8]) {
        let marked = text.get(..3) == Some(&[0xEF, 0xBB, 0xBF][..]);
        let expected = if marked {
            Some(Fault::ByteOrderMark)
        } else if !is_utf8(text) {
            Some(Fault::NotUtf8)
        } else if text.last().is_some_and(|&last| last != b'\n') {
            Some(Fault::NoFinalNewline)
        } else {
            None
        };
        assert_eq!(fault(text), expected);
    }

    /// A baseline admits no new finding, notices a vanished one, and only shrinks.
    pub fn spec_baseline(before: u32, after: u32, found: u32) {
        assert_eq!(shrinks(before, after), after <= before);
        let standing = standing(after, found);
        assert_eq!(standing == Standing::Exceeded, found > after);
        assert_eq!(standing == Standing::Shrunk, found < after);
        assert_eq!(standing == Standing::Within, found == after);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn the_validator_accepts_every_scalar_value_and_refuses_impossible_leads() {
        let point: u32 = kani::any();
        kani::assume(point <= 0x10_FFFF && !(0xD800..=0xDFFF).contains(&point));
        let mut encoded = [0u8; 4];
        let length = if point < 0x80 {
            encoded[0] = point as u8;
            1
        } else if point < 0x800 {
            encoded[0] = 0xC0 | (point >> 6) as u8;
            encoded[1] = 0x80 | (point & 0x3F) as u8;
            2
        } else if point < 0x1_0000 {
            encoded[0] = 0xE0 | (point >> 12) as u8;
            encoded[1] = 0x80 | ((point >> 6) & 0x3F) as u8;
            encoded[2] = 0x80 | (point & 0x3F) as u8;
            3
        } else {
            encoded[0] = 0xF0 | (point >> 18) as u8;
            encoded[1] = 0x80 | ((point >> 12) & 0x3F) as u8;
            encoded[2] = 0x80 | ((point >> 6) & 0x3F) as u8;
            encoded[3] = 0x80 | (point & 0x3F) as u8;
            4
        };
        spec_scalar(&encoded[..length]);
        let other: [u8; 2] = kani::any();
        spec_impossible_lead(&other);
        kani::cover!(point > 0xFFFF);
        kani::cover!(other[0] == 0xC0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_file_passes_only_as_utf8_without_a_mark_that_ends_with_a_newline() {
        let bytes: [u8; 4] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= 4);
        let text = &bytes[..length];
        spec_fault(text);
        kani::cover!(fault(text).is_none() && length == 4);
        kani::cover!(fault(text) == Some(Fault::NoFinalNewline));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_baseline_admits_no_new_finding_and_keeps_shrinking() {
        let before: u32 = kani::any();
        let after: u32 = kani::any();
        let found: u32 = kani::any();
        spec_baseline(before, after, found);
        kani::cover!(shrinks(before, after) && found < before);
        kani::cover!(standing(after, found) == Standing::Exceeded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes at every boundary of the UTF-8 encoding forms.
    const BOUNDARIES: &[u8] =
        b"\x00\x0a\x7f\x80\x8f\x90\x9f\xa0\xbb\xbf\xc0\xc1\xc2\xdf\xe0\xed\xef\xf0\xf4\xf5\xff";

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every scalar value")]
    fn every_scalar_value_and_every_short_text_meets_the_encoding_specifications() {
        for character in (0..=0x10_FFFF)
            .step_by(7)
            .chain(0xD7F0..=0xE010)
            .filter_map(char::from_u32)
        {
            let mut encoded = [0u8; 4];
            proofs::spec_scalar(character.encode_utf8(&mut encoded).as_bytes());
        }
        for text in crate::domain::words(BOUNDARIES, 4) {
            assert_eq!(
                is_utf8(&text),
                core::str::from_utf8(&text).is_ok(),
                "{text:x?}"
            );
            proofs::spec_impossible_lead(&text);
            proofs::spec_fault(&text);
        }
    }

    #[test]
    fn every_boundary_meets_the_baseline_specification() {
        for before in [0, 1, 2, u32::MAX] {
            for after in [0, 1, 2, u32::MAX] {
                for found in [0, 1, 2, u32::MAX] {
                    proofs::spec_baseline(before, after, found);
                }
            }
        }
    }

    #[test]
    fn each_fault_has_its_own_key() {
        assert_eq!(
            [Fault::ByteOrderMark, Fault::NotUtf8, Fault::NoFinalNewline].map(Fault::key),
            ["byte-order-mark", "not-utf-8", "no-final-newline"]
        );
    }

    #[test]
    fn the_encoding_refuses_a_mark_broken_utf8_and_a_missing_final_newline() {
        assert_eq!(fault(b""), None);
        assert_eq!(fault("na\u{ef}ve\n".as_bytes()), None);
        assert_eq!(fault(b"\xEF\xBB\xBFtext\n"), Some(Fault::ByteOrderMark));
        assert_eq!(fault(b"na\xEFve\n"), Some(Fault::NotUtf8));
        assert_eq!(fault(b"text"), Some(Fault::NoFinalNewline));
    }

    #[test]
    fn the_validator_refuses_overlong_forms_surrogates_and_points_past_the_range() {
        assert!(is_utf8("\u{10FFFF}".as_bytes()));
        assert!(!is_utf8(b"\xC0\xAF"));
        assert!(!is_utf8(b"\xED\xA0\x80"));
        assert!(!is_utf8(b"\xF4\x90\x80\x80"));
        assert!(!is_utf8(b"\xE2\x82"));
    }

    #[test]
    fn a_baseline_only_shrinks_and_refuses_both_new_and_vanished_findings() {
        assert_eq!(standing(2, 2), Standing::Within);
        assert_eq!(standing(2, 1), Standing::Shrunk);
        assert_eq!(standing(0, 1), Standing::Exceeded);
        assert!(shrinks(3, 2));
        assert!(!shrinks(0, 1));
    }
}
