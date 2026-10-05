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

/// How Git stages the bytes: `git add` keeps CRLF already in the index under `text=auto`, and `git add --renormalize` does not.
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
pub fn declared_exempt(attributes: Attributes) -> bool {
    attributes.eol == Eol::Crlf || attributes.text == Text::Unset || attributes.binary
}

/// Explicit `text`, or `eol=lf` without a `text` value, makes Git convert without content detection.
pub fn forces_text(attributes: Attributes) -> bool {
    attributes.text == Text::Set
        || (attributes.text == Text::Unspecified && attributes.eol == Eol::Lf)
}

/// Git's `convert_is_binary`: a lone CR, a NUL, or more than one nonprintable byte per 128 printable ones.
pub fn auto_binary(content: Content) -> bool {
    content.lone_cr || content.nul || (content.printable >> 7) < content.nonprintable
}

/// Git's `has_crlf_in_index` rule: content detection keeps a path whose index entry already holds CRLF text.
pub fn index_keeps(staging: Staging, index: Index) -> bool {
    staging == Staging::Add && index == Index::CrlfText
}

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

/// Classifies bytes as Git's `gather_stats` does; LF and both bytes of a CRLF pair count as neither printable nor nonprintable.
pub fn content(bytes: &[u8]) -> Content {
    let mut found = Content {
        crlf: false,
        lone_cr: false,
        nul: false,
        printable: 0,
        nonprintable: 0,
    };
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if !kept(bytes, index) => {
                found.crlf = true;
                index += 1;
            }
            b'\r' => found.lone_cr = true,
            b'\n' => {}
            0x08 | b'\t' | 0x0c | 0x1b => found.printable += 1,
            0 => {
                found.nul = true;
                found.nonprintable += 1;
            }
            0x7f | 0x01..=0x1f => found.nonprintable += 1,
            _ => found.printable += 1,
        }
        index += 1;
    }
    if bytes.last() == Some(&0x1a) {
        found.nonprintable -= 1;
    }
    found
}

/// Whether conversion keeps the byte at `index`: every byte except a carriage return that precedes a line feed.
pub fn kept(bytes: &[u8], index: usize) -> bool {
    !(bytes.len() > 1
        && index < bytes.len() - 1
        && bytes[index] == b'\r'
        && bytes[index + 1] == b'\n')
}

/// Removes each carriage return that precedes a line feed, as Git does when it normalizes text.
pub fn to_lf(bytes: &[u8]) -> Vec<u8> {
    (0..bytes.len())
        .filter(|&index| kept(bytes, index))
        .map(|index| bytes[index])
        .collect()
}

#[cfg(kani)]
impl kani::Arbitrary for Text {
    fn any() -> Self {
        match kani::any::<u8>() % 4 {
            0 => Self::Set,
            1 => Self::Unset,
            2 => Self::Auto,
            _ => Self::Unspecified,
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for Eol {
    fn any() -> Self {
        match kani::any::<u8>() % 3 {
            0 => Self::Lf,
            1 => Self::Crlf,
            _ => Self::Unspecified,
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for Index {
    fn any() -> Self {
        if kani::any() {
            Self::CrlfText
        } else {
            Self::Other
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for Staging {
    fn any() -> Self {
        if kani::any() {
            Self::Add
        } else {
            Self::Renormalize
        }
    }
}

#[cfg(kani)]
#[kani::proof]
fn declared_line_endings_are_never_rewritten() {
    let attributes = Attributes {
        text: kani::any(),
        eol: kani::any(),
        binary: kani::any(),
    };
    let content = Content {
        crlf: kani::any(),
        lone_cr: kani::any(),
        nul: kani::any(),
        printable: kani::any(),
        nonprintable: kani::any(),
    };
    let staging: Staging = kani::any();
    let index: Index = kani::any();
    let decided = action(attributes, staging, index, content);
    if attributes.eol == Eol::Crlf || attributes.text == Text::Unset || attributes.binary {
        assert_eq!(decided, Action::Keep);
    }
    if !content.crlf {
        assert_eq!(decided, Action::Keep);
    }
    let eol_only = attributes.text == Text::Unspecified && attributes.eol == Eol::Lf;
    if decided == Action::Normalize && attributes.text != Text::Set && !eol_only {
        assert!(!(content.nul || content.lone_cr));
        assert!(content.nonprintable == 0 || content.printable >= 128);
        assert!(staging == Staging::Renormalize || index == Index::Other);
    }
    if decided == Action::Keep
        && content.crlf
        && !attributes.binary
        && attributes.eol != Eol::Crlf
        && matches!(attributes.text, Text::Auto | Text::Unspecified)
    {
        assert!(!eol_only);
        assert!(
            content.nul
                || content.lone_cr
                || content.nonprintable > content.printable / 128
                || (staging == Staging::Add && index == Index::CrlfText)
        );
    }
    if staging == Staging::Renormalize || index == Index::Other {
        let other = if staging == Staging::Add {
            Staging::Renormalize
        } else {
            Staging::Add
        };
        assert_eq!(decided, action(attributes, other, Index::Other, content));
    }
    kani::cover!(decided == Action::Normalize && attributes.text == Text::Auto);
    kani::cover!(decided == Action::Normalize && content.nul);
    kani::cover!(decided == Action::Keep && content.crlf && attributes.eol == Eol::Crlf);
    kani::cover!(decided == Action::Keep && content.crlf && !content.nul && !content.lone_cr);
    kani::cover!(
        decided == Action::Keep
            && attributes.text == Text::Auto
            && content.crlf
            && content.nonprintable == 0
            && !content.lone_cr
            && !content.nul
    );
    kani::cover!(
        decided == Action::Normalize && attributes.text == Text::Auto && index == Index::CrlfText
    );
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(5)]
fn conversion_removes_only_carriage_returns_before_line_feeds() {
    let input: [u8; 4] = kani::any();
    let index: usize = kani::any();
    let keep = kept(&input, index);
    let pair = index < 3 && input[index] == b'\r' && input[index + 1] == b'\n';
    assert_eq!(keep, !pair);
    let carriage_return = index < 4 && input[index] == b'\r';
    if carriage_return && !content(&input).lone_cr {
        assert!(!keep);
    }
    kani::cover!(keep && carriage_return);
    kani::cover!(!keep);
    kani::cover!(keep && index >= 4);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(5)]
fn auto_detection_follows_git_byte_classes() {
    let input: [u8; 4] = kani::any();
    let found = content(&input);
    let mut lone_cr = false;
    let mut control = false;
    let mut crlf = false;
    let mut nul = false;
    let mut index = 0;
    while index < 4 {
        let byte = input[index];
        let pair = index < 3 && byte == b'\r' && input[index + 1] == b'\n';
        crlf |= pair;
        nul |= byte == 0;
        lone_cr |= byte == b'\r' && !pair;
        let class = byte == 0x7f
            || (byte < 0x20 && !matches!(byte, 0x08 | b'\t' | b'\n' | 0x0c | b'\r' | 0x1b));
        control |= class && !(index == 3 && byte == 0x1a);
        index += 1;
    }
    assert_eq!(found.crlf, crlf);
    assert_eq!(found.lone_cr, lone_cr);
    assert_eq!(found.nul, nul);
    assert!(found.printable + found.nonprintable <= 4);
    assert_eq!(auto_binary(found), lone_cr || control);
    kani::cover!(auto_binary(found) && !found.lone_cr && !found.nul);
    kani::cover!(!auto_binary(found) && found.crlf && input[3] == 0x1a);
}
