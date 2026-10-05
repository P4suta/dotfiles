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

/// The byte facts Git's `text=auto` detection uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Content {
    pub crlf: bool,
    pub lone_cr: bool,
    pub nul: bool,
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

/// Explicit `text` converts every CRLF; otherwise content Git would treat as binary is kept.
pub fn action(attributes: Attributes, content: Content) -> Action {
    if declared_exempt(attributes) || !content.crlf {
        Action::Keep
    } else if attributes.text == Text::Set || !(content.nul || content.lone_cr) {
        Action::Normalize
    } else {
        Action::Keep
    }
}

pub fn content(bytes: &[u8]) -> Content {
    let mut found = Content {
        crlf: false,
        lone_cr: false,
        nul: false,
    };
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            0 => found.nul = true,
            b'\r' if !kept(bytes, index) => found.crlf = true,
            b'\r' => found.lone_cr = true,
            _ => {}
        }
        index += 1;
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
    };
    let decided = action(attributes, content);
    let exempt = attributes.eol == Eol::Crlf || attributes.text == Text::Unset || attributes.binary;
    assert_eq!(declared_exempt(attributes), exempt);
    if exempt || !content.crlf {
        assert_eq!(decided, Action::Keep);
    }
    if decided == Action::Normalize {
        assert!(attributes.text == Text::Set || !(content.nul || content.lone_cr));
    }
    kani::cover!(decided == Action::Normalize && attributes.text != Text::Set);
    kani::cover!(decided == Action::Keep && content.crlf && attributes.eol == Eol::Crlf);
    kani::cover!(decided == Action::Keep && content.crlf && !exempt);
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
