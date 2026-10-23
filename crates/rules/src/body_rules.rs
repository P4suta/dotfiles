/// The reason the checker refuses a document bound for GitHub.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// A line claims authorship for an agent: a byline trailer, a generated-with footer, or a session URL.
    Attribution,
    /// The hook couldn't inspect the published body.
    UnreadableBody,
    /// The prose checker reported findings in the body.
    InvalidProse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse(Reason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Inspection {
    Clear,
    Findings,
    Unreadable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Quote {
    Unquoted,
    Single,
    Double,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum EscapeMode {
    Unquoted,
    Double,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WordStep {
    Quote(Quote),
    Escape(EscapeMode),
    Emit,
    Opaque,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscapeStep {
    Emit,
    KeepBackslash,
    Continuation,
    Opaque,
}

/// Classifies one character in a static Bash argument.
#[must_use]
pub const fn word_step(quote: Quote, character: char) -> WordStep {
    if character == '\0' {
        return WordStep::Opaque;
    }
    match (quote, character) {
        (Quote::Single, '\'') | (Quote::Double, '"') => WordStep::Quote(Quote::Unquoted),
        (Quote::Unquoted, '\'') => WordStep::Quote(Quote::Single),
        (Quote::Unquoted, '"') => WordStep::Quote(Quote::Double),
        (Quote::Unquoted, '\\') => WordStep::Escape(EscapeMode::Unquoted),
        (Quote::Double, '\\') => WordStep::Escape(EscapeMode::Double),
        (Quote::Unquoted | Quote::Double, '$' | '`')
        | (
            Quote::Unquoted,
            '*' | '?' | '[' | ']' | '{' | '}' | '~' | '(' | ')' | '<' | '>' | '|' | '&' | ';' | ' '
            | '\t' | '\r' | '\n',
        ) => WordStep::Opaque,
        (Quote::Unquoted | Quote::Single | Quote::Double, _) => WordStep::Emit,
    }
}

/// Removes a supported Bash escape without expanding its value.
#[must_use]
pub const fn escape_step(mode: EscapeMode, character: char) -> EscapeStep {
    match (mode, character) {
        (_, '\0') => EscapeStep::Opaque,
        (_, '\n') => EscapeStep::Continuation,
        (EscapeMode::Double, '$' | '`' | '"' | '\\') | (EscapeMode::Unquoted, _) => {
            EscapeStep::Emit
        }
        (EscapeMode::Double, _) => EscapeStep::KeepBackslash,
    }
}

#[must_use]
pub const fn publication(inspection: Inspection) -> Verdict {
    match inspection {
        Inspection::Clear => Verdict::Admit,
        Inspection::Findings => Verdict::Refuse(Reason::InvalidProse),
        Inspection::Unreadable => Verdict::Refuse(Reason::UnreadableBody),
    }
}

/// The checker never sends a PR, issue, or comment body that carries an attribution line.
#[must_use]
pub fn verdict(attributed: bool) -> Verdict {
    if attributed {
        Verdict::Refuse(Reason::Attribution)
    } else {
        Verdict::Admit
    }
}

#[must_use]
pub const fn next_action(reason: Reason) -> &'static str {
    match reason {
        Reason::Attribution => {
            "Delete that line from the document and send it again; the author's own Co-authored-by lines and prose that mentions Claude are not affected."
        }
        Reason::UnreadableBody => {
            "Use a literal, readable body file; publish issue or PR documents through `pr-workflow`."
        }
        Reason::InvalidProse => {
            "Fix the prose findings in the body; publish issue or PR documents through `pr-workflow`."
        }
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_word_step(quote: Quote, character: char) {
        let step = word_step(quote, character);
        if character == '\0' {
            assert_eq!(step, WordStep::Opaque);
            return;
        }
        match quote {
            Quote::Single => assert_eq!(
                step,
                if character == '\'' {
                    WordStep::Quote(Quote::Unquoted)
                } else {
                    WordStep::Emit
                }
            ),
            Quote::Double => {
                assert_eq!(step == WordStep::Opaque, matches!(character, '$' | '`'));
                assert_eq!(
                    step == WordStep::Emit,
                    !matches!(character, '$' | '`' | '\\' | '"')
                );
                assert_eq!(
                    step == WordStep::Escape(EscapeMode::Double),
                    character == '\\'
                );
                assert_eq!(step == WordStep::Quote(Quote::Unquoted), character == '"');
            }
            Quote::Unquoted => {
                assert_eq!(
                    step == WordStep::Emit,
                    !matches!(
                        character,
                        '$' | '`'
                            | '*'
                            | '?'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '~'
                            | '('
                            | ')'
                            | '<'
                            | '>'
                            | '|'
                            | '&'
                            | ';'
                            | ' '
                            | '\t'
                            | '\r'
                            | '\n'
                            | '\\'
                            | '\''
                            | '"'
                    )
                );
                assert_eq!(
                    step == WordStep::Opaque,
                    matches!(
                        character,
                        '$' | '`'
                            | '*'
                            | '?'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '~'
                            | '('
                            | ')'
                            | '<'
                            | '>'
                            | '|'
                            | '&'
                            | ';'
                            | ' '
                            | '\t'
                            | '\r'
                            | '\n'
                    )
                );
                assert_eq!(
                    step == WordStep::Escape(EscapeMode::Unquoted),
                    character == '\\'
                );
                assert_eq!(step == WordStep::Quote(Quote::Single), character == '\'');
                assert_eq!(step == WordStep::Quote(Quote::Double), character == '"');
            }
        }
    }

    pub fn spec_escape_step(mode: EscapeMode, character: char) {
        let step = escape_step(mode, character);
        assert_eq!(step == EscapeStep::Opaque, character == '\0');
        assert_eq!(step == EscapeStep::Continuation, character == '\n');
        if !matches!(character, '\0' | '\n') {
            assert_eq!(
                step == EscapeStep::Emit,
                mode == EscapeMode::Unquoted || matches!(character, '$' | '`' | '"' | '\\')
            );
            assert_eq!(
                step == EscapeStep::KeepBackslash,
                mode == EscapeMode::Double && !matches!(character, '$' | '`' | '"' | '\\')
            );
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(1)]
    fn static_word_steps_preserve_quoting_and_refuse_expansion() {
        let quote: Quote = kani::any();
        let character: char = kani::any();
        spec_word_step(quote, character);
        kani::cover!(word_step(quote, character) == WordStep::Emit);
        kani::cover!(word_step(quote, character) == WordStep::Opaque);
        kani::cover!(word_step(quote, character) == WordStep::Escape(EscapeMode::Double));
        kani::cover!(word_step(quote, character) == WordStep::Quote(Quote::Single));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(1)]
    fn escapes_preserve_double_quoted_backslashes_and_remove_continuations() {
        let mode: EscapeMode = kani::any();
        let character: char = kani::any();
        spec_escape_step(mode, character);
        kani::cover!(escape_step(mode, character) == EscapeStep::Emit);
        kani::cover!(escape_step(mode, character) == EscapeStep::KeepBackslash);
        kani::cover!(escape_step(mode, character) == EscapeStep::Continuation);
        kani::cover!(escape_step(mode, character) == EscapeStep::Opaque);
    }

    pub fn spec_publication(inspection: Inspection) {
        let result = publication(inspection);
        assert_eq!(result == Verdict::Admit, inspection == Inspection::Clear);
        assert_eq!(
            result == Verdict::Refuse(Reason::UnreadableBody),
            inspection == Inspection::Unreadable
        );
        assert_eq!(
            result == Verdict::Refuse(Reason::InvalidProse),
            inspection == Inspection::Findings
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(1)]
    fn only_a_complete_clear_inspection_admits_publication() {
        let inspection: Inspection = kani::any();
        spec_publication(inspection);
        let result = publication(inspection);
        kani::cover!(result == Verdict::Admit);
        kani::cover!(result == Verdict::Refuse(Reason::UnreadableBody));
        kani::cover!(result == Verdict::Refuse(Reason::InvalidProse));
    }

    pub fn spec_verdict(attributed: bool) {
        let result = verdict(attributed);
        assert_eq!(result == Verdict::Admit, !attributed);
        if let Verdict::Refuse(reason) = result {
            assert!(next_action(reason).starts_with("Delete that line"));
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn attributed_bodies_are_never_admitted_and_each_refusal_names_the_deletion() {
        let attributed: bool = kani::any();
        spec_verdict(attributed);
        let result = verdict(attributed);
        kani::cover!(result == Verdict::Admit);
        kani::cover!(result == Verdict::Refuse(Reason::Attribution));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_quote_steps_cover_ascii_and_unicode_boundaries() {
        for character in (u8::MIN..=127)
            .map(char::from)
            .chain(['é', '\u{2028}', char::MAX])
        {
            for quote in [Quote::Unquoted, Quote::Single, Quote::Double] {
                proofs::spec_word_step(quote, character);
            }
            for mode in [EscapeMode::Unquoted, EscapeMode::Double] {
                proofs::spec_escape_step(mode, character);
            }
        }
    }

    #[test]
    fn publication_requires_a_complete_clear_body_inspection() {
        for inspection in [
            Inspection::Clear,
            Inspection::Findings,
            Inspection::Unreadable,
        ] {
            proofs::spec_publication(inspection);
        }
    }

    #[test]
    fn every_input_meets_the_specification() {
        for attributed in [false, true] {
            proofs::spec_verdict(attributed);
        }
    }

    #[test]
    fn an_attributed_body_is_refused_and_a_clean_one_admitted() {
        assert_eq!(verdict(true), Verdict::Refuse(Reason::Attribution));
        assert_eq!(verdict(false), Verdict::Admit);
    }

    #[test]
    fn the_refusal_names_deleting_the_line() {
        assert!(next_action(Reason::Attribution).starts_with("Delete that line"));
    }
}
