/// Whether one comment line holds a machine directive rather than prose.
///
/// A directive takes one line in one of three forms.
/// The first names a namespace and a name, such as `chezmoi:modify-template` or `dotguard:allow-foreign`, and arguments may follow.
/// The second names a tool followed only by `key=value` settings, such as `shellcheck disable=SC2034`.
/// The third holds one lowercase hyphenated word, such as `prettier-ignore`.
#[must_use]
pub fn is_directive(line: &[u8]) -> bool {
    let mut first: Option<Word> = None;
    let mut word = Word::EMPTY;
    let mut others = false;
    let mut settings = true;
    let mut rest = line;
    loop {
        let byte = match rest {
            [byte, tail @ ..] => {
                rest = tail;
                Some(*byte)
            }
            [] => None,
        };
        match byte {
            Some(b'\n' | b'\r') => return false,
            Some(byte) if !blank(byte) => {
                word = word.push(byte);
                continue;
            }
            _ => {}
        }
        if word.started() {
            if first.is_none() {
                first = Some(word);
            } else {
                others = true;
                settings = settings && word.setting();
            }
            word = Word::EMPTY;
        }
        if byte.is_none() {
            break;
        }
    }
    first.is_some_and(|first| {
        first.namespaced()
            || (!others && first.hyphenated())
            || (others && settings && first.named())
    })
}

fn blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// A byte a lowercase name holds: a small letter, a digit, `-`, or `_`.
fn name_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
}

/// The first separator, such as `:` or `=`, that a word holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Split {
    Unseen,
    /// `head` holds whether the part before the separator forms a name.
    /// `next` holds whether a small letter follows the separator, once a byte follows it.
    Seen {
        head: bool,
        next: Option<bool>,
    },
}

impl Split {
    fn push(self, byte: u8, separator: u8, name: Option<bool>) -> Self {
        match self {
            Self::Unseen if byte == separator => Self::Seen {
                head: name == Some(true),
                next: None,
            },
            Self::Seen { head, next: None } => Self::Seen {
                head,
                next: Some(byte.is_ascii_lowercase()),
            },
            other => other,
        }
    }
}

/// Where a word holds a hyphen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hyphen {
    Absent,
    Inside,
    Last,
}

/// What the bytes of one word read so far show, so the rule reads each byte once and never slices.
#[derive(Clone, Copy)]
struct Word {
    /// `None` before the first byte, then whether every byte so far forms a lowercase name: a small letter, then name bytes.
    name: Option<bool>,
    colon: Split,
    equals: Split,
    hyphen: Hyphen,
}

impl Word {
    const EMPTY: Self = Self {
        name: None,
        colon: Split::Unseen,
        equals: Split::Unseen,
        hyphen: Hyphen::Absent,
    };

    fn push(self, byte: u8) -> Self {
        Self {
            name: Some(match self.name {
                None => byte.is_ascii_lowercase(),
                Some(name) => name && name_byte(byte),
            }),
            colon: self.colon.push(byte, b':', self.name),
            equals: self.equals.push(byte, b'=', self.name),
            hyphen: match (byte, self.hyphen) {
                (b'-', _) => Hyphen::Last,
                (_, Hyphen::Absent) => Hyphen::Absent,
                _ => Hyphen::Inside,
            },
        }
    }

    fn started(self) -> bool {
        self.name.is_some()
    }

    fn named(self) -> bool {
        self.name == Some(true)
    }

    /// `namespace:name`, where the part after the colon starts with a small letter.
    fn namespaced(self) -> bool {
        self.colon
            == Split::Seen {
                head: true,
                next: Some(true),
            }
    }

    /// `key=value` with a lowercase key and a value.
    fn setting(self) -> bool {
        matches!(
            self.equals,
            Split::Seen {
                head: true,
                next: Some(_)
            }
        )
    }

    fn hyphenated(self) -> bool {
        self.named() && self.hyphen == Hyphen::Inside
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    fn lower(byte: u8) -> bool {
        byte.is_ascii_lowercase()
    }

    #[cfg(test)]
    fn name_byte(byte: u8) -> bool {
        lower(byte) || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
    }

    /// A line counts as a directive exactly when it takes one of the three forms, so prose with a capital, a space after a colon, or a sentence never counts.
    #[cfg(test)]
    pub fn spec_is_directive(line: &[u8]) {
        let result = is_directive(line);
        let blank = |byte: &u8| *byte == b' ' || *byte == b'\t';
        let start = line.iter().position(|byte| !blank(byte));
        let end = line.iter().rposition(|byte| !blank(byte));
        let (Some(start), Some(end)) = (start, end) else {
            assert!(!result);
            return;
        };
        let body = line.get(start..=end).unwrap_or_default();
        if body.iter().any(|byte| *byte == b'\n' || *byte == b'\r') {
            assert!(!result);
            return;
        }
        let first_end = body.iter().position(blank).unwrap_or(body.len());
        let (first, rest) = body.split_at(first_end);
        let is_name = |word: &[u8]| {
            word.first().is_some_and(|byte| lower(*byte))
                && word.iter().all(|byte| name_byte(*byte))
        };
        let colon = first.iter().position(|byte| *byte == b':');
        let namespaced = colon.is_some_and(|colon| {
            let (namespace, after) = first.split_at(colon);
            is_name(namespace) && after.get(1).is_some_and(|byte| lower(*byte))
        });
        let settings = rest.iter().any(|byte| !blank(byte))
            && rest
                .split(|byte| blank(byte))
                .filter(|word| !word.is_empty())
                .all(|word| {
                    word.iter()
                        .position(|byte| *byte == b'=')
                        .is_some_and(|equals| {
                            let (key, value) = word.split_at(equals);
                            is_name(key) && value.len() > 1
                        })
                });
        let alone = !rest.iter().any(|byte| !blank(byte));
        let hyphenated =
            alone && is_name(first) && first.contains(&b'-') && first.last() != Some(&b'-');
        assert_eq!(
            result,
            namespaced || (is_name(first) && settings) || hyphenated
        );
        if result {
            assert!(first.first().is_some_and(|byte| lower(*byte)));
        }
    }

    /// Kani checks the shape every directive shares, because the full specification's word splitting runs past the proof budget.
    /// The tests check the full specification over every short line.
    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(5)]
    fn a_directive_starts_with_a_lowercase_name_and_prose_never_matches() {
        let bytes: [u8; 3] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        let line = &bytes[..length];
        let result = is_directive(line);
        if result {
            let mut rest = line;
            while let [b' ' | b'\t', tail @ ..] = rest {
                rest = tail;
            }
            assert!(matches!(rest, [first, ..] if lower(*first)));
            let mut rest = line;
            while let [byte, tail @ ..] = rest {
                assert!(*byte != b'\n' && *byte != b'\r');
                rest = tail;
            }
        }
        kani::cover!(result);
        kani::cover!(!result & (length == bytes.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every short line")]
    fn every_short_line_meets_the_specification_and_examples_hold() {
        for line in crate::domain::words(b"aA:-= \n", 4) {
            proofs::spec_is_directive(&line);
        }
        for line in [
            "chezmoi:modify-template",
            " chezmoi:template:left-delimiter=\"[[\" right-delimiter=\"]]\" ",
            "\tdotguard:allow-foreign\t",
            "shellcheck\tdisable=SC2034",
            "shellcheck source=lib.sh shell=bash",
            "prettier-ignore",
            "eslint-disable-next-line",
        ] {
            assert!(is_directive(line.as_bytes()), "{line}");
        }
        for line in [
            "",
            "Claude Code rewrites this file at runtime.",
            "Note: the hook runs first.",
            "https://example.com/page",
            "well-known values follow",
            "shellcheck disable",
            "prettier-",
            "chezmoi:modify-template\nmore",
            "ns:Upper",
            "a:b\r",
            "\t",
        ] {
            assert!(!is_directive(line.as_bytes()), "{line}");
        }
    }
}
