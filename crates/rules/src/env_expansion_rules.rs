extern crate alloc;
use self::alloc::string::String;
use self::alloc::vec::Vec;

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
fn name_letter(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// How many name letters `chars` holds from `start`.
fn name_length(chars: &[char], start: usize) -> usize {
    chars
        .iter()
        .skip(start)
        .take_while(|c| name_letter(**c))
        .count()
}

/// The first unexpanded variable reference in a value: `$name`, `${name}`, or `%NAME%`.
///
/// Exempt as disproportionate: Kani spends past its budget on the string search of even a three-byte word.
/// Evidence: `crates/rules/src/env_expansion_rules.rs::tests::every_marker_form_is_found`.
/// Reconsider when the bolero pilot of issue 105 adopts fuzzing.
#[must_use]
pub fn marker(value: &str) -> Option<Reference> {
    let chars: Vec<char> = value.chars().collect();
    for (index, &c) in chars.iter().enumerate() {
        let name = index + 1;
        let end = match c {
            '$' if matches!(chars.get(name), Some('{')) => {
                let length = name_length(&chars, name + 1);
                (length > 0 && chars.get(name + 1 + length) == Some(&'}'))
                    .then_some(name + length + BRACE_EDGES)
            }
            '$' if matches!(chars.get(name), Some(letter) if letter.is_ascii_alphabetic() || *letter == '_') => {
                Some(name + name_length(&chars, name))
            }
            '%' if matches!(chars.get(name), Some(letter) if letter.is_ascii_alphabetic()) => {
                let length = name_length(&chars, name);
                (chars.get(name + length) == Some(&'%')).then_some(name + length + 1)
            }
            _ => None,
        };
        if let Some(end) = end
            && let Some(found) = chars.get(index..end)
        {
            return Some(Reference(found.iter().collect()));
        }
    }
    None
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

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
