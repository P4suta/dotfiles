extern crate alloc;
use self::alloc::string::String;

/// The path environment values a managed tool reads must hold expanded paths, not templates another layer must expand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse,
}

#[must_use]
pub fn verdict(unexpanded: bool) -> Verdict {
    if unexpanded {
        Verdict::Refuse
    } else {
        Verdict::Admit
    }
}
/// What a tool does with a value that still holds a variable reference.
pub const NEXT_ACTION: &str = "Export the variable with the path expanded by the shell that sets it, not a template another layer must expand.";

/// The variable reference a value still holds, such as `$name`, `${name}`, or `%NAME%`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference(String);

impl Reference {
    /// The reference text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The braces around the name of a `${name}` reference.
const BRACE_EDGES: usize = 2;
/// The letters a variable name holds, which a reference may use once it has started.
fn name_letter(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// How many name letters `bytes` holds from `start`.
fn name_length(bytes: &[u8], start: usize) -> usize {
    bytes
        .iter()
        .skip(start)
        .take_while(|c| name_letter(**c))
        .count()
}

/// The first unexpanded variable reference in a value: `$name`, `${name}`, or `%NAME%`.
///
/// Kani: proved for inputs up to 4 bytes.
#[must_use]
pub fn marker(value: &str) -> Option<Reference> {
    let bytes = value.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        let name = index + 1;
        let end = match byte {
            b'$' if matches!(bytes.get(name), Some(b'{')) => {
                let length = name_length(bytes, name + 1);
                (length > 0 && bytes.get(name + 1 + length) == Some(&b'}'))
                    .then_some(name + length + BRACE_EDGES)
            }
            b'$' if matches!(bytes.get(name), Some(letter) if letter.is_ascii_alphabetic() || *letter == b'_') => {
                Some(name + name_length(bytes, name))
            }
            b'%' if matches!(bytes.get(name), Some(letter) if letter.is_ascii_alphabetic()) => {
                let length = name_length(bytes, name);
                (bytes.get(name + length) == Some(&b'%')).then_some(name + length + 1)
            }
            _ => None,
        };
        if let Some(end) = end
            && let Some(found) = value.get(index..end)
        {
            return Some(Reference(String::from(found)));
        }
    }
    None
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    fn reference_marker(value: &str) -> Option<&str> {
        let bytes = value.as_bytes();
        for start in 0..bytes.len() {
            let tail = bytes.get(start..)?;
            let (name, close) = match tail {
                [b'$', b'{', ..] => (2, Some(b'}')),
                [b'$', first, ..] if first.is_ascii_alphabetic() || *first == b'_' => (1, None),
                [b'%', first, ..] if first.is_ascii_alphabetic() => (1, Some(b'%')),
                _ => continue,
            };
            let letters = tail
                .get(name..)?
                .iter()
                .take_while(|letter| letter.is_ascii_alphanumeric() || **letter == b'_')
                .count();
            if letters == 0 {
                continue;
            }
            let end = name + letters;
            if let Some(close) = close {
                if tail.get(end) == Some(&close) {
                    return value.get(start..start + end + 1);
                }
            } else {
                return value.get(start..start + end);
            }
        }
        None
    }

    pub fn spec_marker(value: &str) -> bool {
        let found = marker(value);
        assert_eq!(
            found.as_ref().map(Reference::as_str),
            reference_marker(value)
        );
        found.is_some()
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(5)]
    fn variable_markers_match_the_byte_specification_through_four_bytes() {
        let bytes: [u8; 4] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        if let Ok(value) = core::str::from_utf8(&bytes[..length]) {
            let found = spec_marker(value);
            kani::cover!(found);
            kani::cover!(!found);
        }
    }

    /// The verdict never admits a value that still holds a variable reference.
    pub fn spec_verdict(unexpanded: bool) {
        let result = verdict(unexpanded);
        let expected = if unexpanded {
            Verdict::Refuse
        } else {
            Verdict::Admit
        };
        assert_eq!(result, expected);
        assert!(NEXT_ACTION.len() > 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn unexpanded_values_are_never_admitted() {
        let unexpanded: bool = kani::any();
        spec_verdict(unexpanded);
        let result = verdict(unexpanded);
        kani::cover!(result == Verdict::Admit);
        kani::cover!(result == Verdict::Refuse);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_markers_enumerate_a_small_alphabet_through_four_bytes() {
        for bytes in crate::domain::words(b"$%{}a", 4) {
            proofs::spec_marker(core::str::from_utf8(&bytes).unwrap());
        }
    }

    #[test]
    fn every_input_meets_the_verdict_specification() {
        for [unexpanded] in crate::domain::flags() {
            super::proofs::spec_verdict(unexpanded);
        }
    }

    #[test]
    fn every_marker_form_is_found() {
        for (value, expected) in [
            ("D:\\cargo$k", Some("$k")),
            ("D:/cargo/${name}/x", Some("${name}")),
            ("prefix %NAME% suffix", Some("%NAME%")),
            ("$UNDER_SCORE1", Some("$UNDER_SCORE1")),
            ("a$b c$d", Some("$b")),
            ("é${a}界", Some("${a}")),
        ] {
            assert_eq!(
                marker(value).as_ref().map(Reference::as_str),
                expected,
                "{value}"
            );
        }
    }

    #[test]
    fn expanded_values_have_no_marker() {
        for value in [
            "D:\\cargo\\lane",
            "D:/cargo/lane",
            "100%",
            "cost: $5",
            "$",
            "${}",
            "%",
            "%1two%",
            "",
        ] {
            assert_eq!(marker(value), None, "{value}");
        }
    }
}
