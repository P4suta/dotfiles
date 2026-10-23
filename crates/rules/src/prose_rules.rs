extern crate alloc;

use alloc::vec::Vec;

pub const SENTENCE_PER_LINE: &str = "Dotfiles.SentencePerLine";
pub const MULTIPLE_SENTENCES: &str =
    "Write one sentence per line; start each sentence on its own line.";
pub const WRAPPED_SENTENCE: &str = "Write one sentence per line; keep the whole sentence on one line and split it into shorter sentences when the channel limits line length.";

const ABBREVIATIONS: [&str; 16] = [
    "e.g", "i.e", "etc", "vs", "cf", "approx", "no", "fig", "mr", "mrs", "ms", "dr", "st", "inc",
    "ltd", "u.s",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Line {
    Prose,
    List,
    LeadIn,
    Boundary,
}

/// Code, tables, HTML, and front matter reach this classifier blanked by the Markdown adapter.
#[must_use]
pub fn classify(line: &str) -> Line {
    let text = line.trim();
    if text.is_empty()
        || text.starts_with(['>', '|'])
        || (text.starts_with("{{") && text.ends_with("}}"))
        || (text.starts_with('#')
            && (text.trim_start_matches('#').is_empty()
                || text.trim_start_matches('#').starts_with([' ', '\t'])))
        || text
            .chars()
            .all(|character| matches!(character, '-' | '=' | '*' | '_' | ' '))
        || (((text.starts_with("**") && text.ends_with("**"))
            || (text.starts_with("__") && text.ends_with("__")))
            && !ends_sentence(text))
    {
        Line::Boundary
    } else if text
        .strip_prefix(['-', '+', '*'])
        .is_some_and(|rest| rest.starts_with(' '))
        || (text.starts_with(|character: char| character.is_ascii_digit())
            && text
                .trim_start_matches(|character: char| character.is_ascii_digit())
                .strip_prefix(['.', ')'])
                .is_some_and(|rest| rest.starts_with(' ')))
    {
        Line::List
    } else if text.ends_with([':', '：']) {
        Line::LeadIn
    } else {
        Line::Prose
    }
}

fn closing(character: char) -> bool {
    matches!(
        character,
        ')' | ']' | '"' | '\'' | '’' | '”' | '」' | '』' | '）' | '*' | '_'
    )
}

#[must_use]
pub fn ends_sentence(line: &str) -> bool {
    line.trim_end()
        .trim_end_matches(closing)
        .ends_with(['.', '!', '?', '。', '！', '？'])
}

fn boundary(before: &str, character: char, after: &str) -> bool {
    if matches!(character, '。' | '！' | '？') {
        return true;
    }
    if !matches!(character, '.' | '!' | '?')
        || !before
            .trim_end()
            .chars()
            .next_back()
            .is_some_and(|character| {
                character.is_ascii_alphanumeric()
                    || !character.is_ascii()
                    || closing(character)
                    || matches!(character, '.' | '!' | '?')
            })
        || !after.trim_start_matches(closing).starts_with(' ')
    {
        return false;
    }
    if character != '.' {
        return true;
    }
    let word = before
        .rsplit(|character: char| !character.is_ascii_alphanumeric() && character != '.')
        .next()
        .unwrap_or_default();
    !(ABBREVIATIONS
        .iter()
        .any(|abbreviation| word.eq_ignore_ascii_case(abbreviation))
        || (!word.is_empty()
            && word
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.'))
        || (word.len() == 1 && word.bytes().all(|byte| byte.is_ascii_uppercase())))
}

/// One-based character columns where another sentence starts in a visible prose line.
#[must_use]
pub fn sentence_starts(line: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    for (offset, character) in line.char_indices() {
        let before = line.get(..offset).unwrap_or_default();
        let after = line
            .get(offset + character.len_utf8()..)
            .unwrap_or_default();
        if boundary(before, character, after)
            && let Some((start, _)) = after.char_indices().find(|&(_, character)| {
                !character.is_whitespace()
                    && !closing(character)
                    && !matches!(character, '。' | '！' | '？')
            })
        {
            let column = line
                .get(..offset + character.len_utf8() + start)
                .unwrap_or_default()
                .chars()
                .count()
                + 1;
            if starts.last() != Some(&column) {
                starts.push(column);
            }
        }
    }
    starts
}

#[must_use]
pub const fn wraps(previous: Line, next: Line, terminated: bool) -> bool {
    matches!(previous, Line::Prose) && matches!(next, Line::Prose) && !terminated
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutViolation {
    MultipleSentences,
    WrappedSentence,
}

impl LayoutViolation {
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::MultipleSentences => MULTIPLE_SENTENCES,
            Self::WrappedSentence => WRAPPED_SENTENCE,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutFinding {
    pub line: usize,
    pub column: usize,
    pub violation: LayoutViolation,
}

/// Layout decisions over Markdown after the adapter blanks protected spans and preserves line breaks.
#[must_use]
pub fn sentence_lines(visible: &str) -> Vec<LayoutFinding> {
    let mut findings = Vec::new();
    let mut previous = Line::Boundary;
    let mut terminated = false;
    for (index, text) in visible.lines().enumerate() {
        let mut kind = classify(text);
        if previous == Line::List && text.starts_with([' ', '\t']) && kind == Line::Prose {
            kind = Line::List;
        }
        if wraps(previous, kind, terminated) {
            findings.push(LayoutFinding {
                line: index,
                column: 1,
                violation: LayoutViolation::WrappedSentence,
            });
        }
        if kind != Line::Boundary {
            findings.extend(
                sentence_starts(text)
                    .into_iter()
                    .map(|column| LayoutFinding {
                        line: index + 1,
                        column,
                        violation: LayoutViolation::MultipleSentences,
                    }),
            );
        }
        previous = kind;
        terminated = ends_sentence(text);
    }
    findings
}

/// An exemption needs a stated reason, and an exemption that matches nothing fails as stale.
#[must_use]
pub fn exemption_valid(reason_present: bool, used: bool) -> bool {
    reason_present && used
}

/// The personal writing standard governs a destination that a personal owner holds, unless that destination forks another repository.
/// Other destinations follow their own rules.
#[must_use]
pub fn standard_applies(personal_owner: bool, fork: bool) -> bool {
    personal_owner && !fork
}

/// The owner a remote names: a personal or foreign GitHub owner, or an address that names no GitHub owner, such as a bare-repository hub.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Owner {
    Personal,
    Foreign,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Standard {
    Applies,
    Skips,
    /// The remotes name no destination, so the gate reports the setting that decides it.
    Undetermined,
}

/// Whether a repository's commits take the personal standard.
/// The repository's `prose.standard` setting decides when present.
/// Otherwise another remote with a foreign owner marks a fork, `origin` names the destination, and a personal GitHub remote stands in for a missing `origin` or one that names no GitHub owner.
#[must_use]
pub fn repository_standard(
    declared: Option<bool>,
    origin: Option<Owner>,
    other_personal: bool,
    other_foreign: bool,
) -> Standard {
    match (declared, origin) {
        (None, _) if other_foreign => Standard::Skips,
        (Some(true), _) | (None, Some(Owner::Personal)) => Standard::Applies,
        (Some(false), _) | (None, Some(Owner::Foreign)) => Standard::Skips,
        (None, Some(Owner::Unresolved) | None) if other_personal => Standard::Applies,
        (None, Some(Owner::Unresolved) | None) => Standard::Undetermined,
    }
}

pub const HOOK_PREFIX: &[u8] = b"prose reply --client ";

/// The command prefix that the retired prose installer registered.
#[must_use]
pub fn reply_hook(command: &[u8]) -> bool {
    command.starts_with(HOOK_PREFIX)
}

/// A hook group stays unless removing its reply handlers emptied it.
/// An existing empty group stays unchanged.
#[must_use]
pub fn keep_hook_group(before: usize, after: usize) -> bool {
    before == 0 || after != 0
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const LINE_KINDS: [Line; 4] = [Line::Prose, Line::List, Line::LeadIn, Line::Boundary];

    pub fn spec_wraps(previous: Line, next: Line, terminated: bool) {
        let expected = matches!(
            (previous, next, terminated),
            (Line::Prose, Line::Prose, false)
        );
        assert_eq!(wraps(previous, next, terminated), expected);
    }

    pub fn spec_classification() {
        assert_eq!(classify("a"), Line::Prose);
        assert_eq!(classify("# a"), Line::Boundary);
        assert_eq!(classify("**a**"), Line::Boundary);
        assert_eq!(classify("__a__"), Line::Boundary);
        assert_eq!(classify("- a"), Line::List);
        assert_eq!(classify("1. a"), Line::List);
        assert_eq!(classify("2) a"), Line::List);
        assert_eq!(classify("a:"), Line::LeadIn);
        assert_eq!(classify("a："), Line::LeadIn);
        assert_eq!(classify("> a"), Line::Boundary);
        assert_eq!(classify("| a |"), Line::Boundary);
        assert_eq!(classify("---"), Line::Boundary);
        assert_eq!(classify("{{a}}"), Line::Boundary);
        assert_eq!(classify(""), Line::Boundary);
    }

    pub fn spec_layout(visible: &str, expected: Option<LayoutViolation>) {
        let findings = sentence_lines(visible);
        assert_eq!(findings.first().map(|finding| finding.violation), expected);
        if let Some(finding) = findings.first() {
            assert_eq!(finding.line, 1);
            assert_ne!(finding.violation.message(), "");
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_unterminated_adjacent_prose_lines_wrap() {
        let previous: Line = kani::any();
        let next: Line = kani::any();
        let terminated: bool = kani::any();
        spec_wraps(previous, next, terminated);
        kani::cover!(wraps(previous, next, terminated));
        kani::cover!(!wraps(previous, next, terminated));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn markdown_line_shapes_keep_their_boundaries() {
        spec_classification();
        kani::cover!(classify("a") == Line::Prose);
        kani::cover!(classify("a:") == Line::LeadIn);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn sentence_endings_keep_quoted_and_emphasized_punctuation() {
        let complete: bool = kani::any();
        let line = if complete { "a。**" } else { "a" };
        assert_eq!(ends_sentence(line), complete);
        kani::cover!(ends_sentence(line));
        kani::cover!(!ends_sentence(line));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(24)]
    fn english_sentence_boundaries_do_not_require_a_capital() {
        let complete: bool = kani::any();
        let line = if complete { "go. go" } else { "go" };
        let starts = sentence_starts(line);
        assert_eq!(starts.is_empty(), !complete);
        kani::cover!(starts.is_empty());
        kani::cover!(!starts.is_empty());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn japanese_sentence_boundaries_do_not_require_a_space() {
        let complete: bool = kani::any();
        let line = if complete { "あ。い。" } else { "あ。" };
        let starts = sentence_starts(line);
        assert_eq!(starts.is_empty(), !complete);
        kani::cover!(starts.is_empty());
        kani::cover!(!starts.is_empty());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(24)]
    fn wrapped_prose_reports_its_first_line() {
        let wrapped: bool = kani::any();
        let text = if wrapped { "go\non." } else { "go.\non." };
        spec_layout(text, wrapped.then_some(LayoutViolation::WrappedSentence));
        kani::cover!(sentence_lines(text).is_empty());
        kani::cover!(!sentence_lines(text).is_empty());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(24)]
    fn multiple_sentences_reach_the_layout_decision() {
        let multiple: bool = kani::any();
        let text = if multiple { "go. go" } else { "go." };
        spec_layout(text, multiple.then_some(LayoutViolation::MultipleSentences));
        kani::cover!(sentence_lines(text).is_empty());
        kani::cover!(!sentence_lines(text).is_empty());
    }

    pub const ORIGINS: [Option<Owner>; 4] = [
        Some(Owner::Personal),
        Some(Owner::Foreign),
        Some(Owner::Unresolved),
        None,
    ];

    pub fn spec_standard_applies(personal: bool, fork: bool) {
        assert_eq!(standard_applies(personal, fork), personal && !fork);
    }

    pub fn spec_exemption_valid(reason: bool, used: bool) {
        assert_eq!(exemption_valid(reason, used), reason && used);
    }

    /// A declared standard wins, a foreign remote marks a fork, and only remotes that name no destination leave it undetermined.
    pub fn spec_repository_standard(
        declared: Option<bool>,
        origin: Option<Owner>,
        personal: bool,
        foreign: bool,
    ) {
        let standard = repository_standard(declared, origin, personal, foreign);
        if let Some(declared) = declared {
            assert_eq!(
                standard,
                if declared {
                    Standard::Applies
                } else {
                    Standard::Skips
                }
            );
        } else {
            let unresolved = matches!(origin, Some(Owner::Unresolved) | None);
            assert_eq!(
                standard == Standard::Applies,
                !foreign && (origin == Some(Owner::Personal) || (unresolved && personal))
            );
            assert_eq!(
                standard == Standard::Undetermined,
                unresolved && !personal && !foreign
            );
            assert!(origin != Some(Owner::Foreign) || standard == Standard::Skips);
        }
    }

    pub fn spec_hook_group(before: usize, after: usize) {
        assert_eq!(!keep_hook_group(before, after), before != 0 && after == 0);
    }

    pub fn spec_reply_hook(command: &[u8]) {
        assert_eq!(
            reply_hook(command),
            command.get(..HOOK_PREFIX.len()) == Some(HOOK_PREFIX)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(23)]
    fn reply_hook_matching_checks_the_complete_prefix_and_preserves_other_commands() {
        const LENGTH: usize = HOOK_PREFIX.len() + 1;
        let command: [u8; LENGTH] = kani::any();
        let length = usize::from(kani::any::<u8>()) % (LENGTH + 1);
        spec_reply_hook(&command[..length]);
        kani::cover!(reply_hook(&command));
        kani::cover!(!reply_hook(&command));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_personal_non_fork_destinations_take_the_standard_and_exemptions_need_a_reason_and_a_match()
     {
        let first: bool = kani::any();
        let second: bool = kani::any();
        spec_standard_applies(first, second);
        spec_exemption_valid(first, second);
        let applies = standard_applies(first, second);
        let exempt = exemption_valid(first, second);
        kani::cover!(applies);
        kani::cover!(!exempt & first);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_declared_standard_wins_and_an_unresolved_destination_is_reported() {
        let declared: Option<bool> = kani::any();
        let origin = ORIGINS[usize::from(kani::any::<u8>() % 4)];
        let personal: bool = kani::any();
        let foreign: bool = kani::any();
        spec_repository_standard(declared, origin, personal, foreign);
        let standard = repository_standard(declared, origin, personal, foreign);
        kani::cover!(standard == Standard::Applies && origin.is_none());
        kani::cover!(standard == Standard::Skips && declared.is_none());
        kani::cover!(standard == Standard::Undetermined);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_groups_emptied_by_reply_removal_are_dropped() {
        let before: usize = kani::any();
        let after: usize = kani::any();
        spec_hook_group(before, after);
        let keep = keep_hook_group(before, after);
        let has_handlers = keep & (after != 0);
        let originally_empty = keep & (before == 0);
        kani::cover!(has_handlers);
        kani::cover!(originally_empty);
        kani::cover!(!keep);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HOOK_PREFIX, LayoutViolation, Line, classify, ends_sentence, proofs, reply_hook,
        sentence_lines, sentence_starts,
    };
    use crate::domain::flags;

    #[test]
    fn every_input_meets_the_specifications() {
        proofs::spec_classification();
        for previous in proofs::LINE_KINDS {
            for next in proofs::LINE_KINDS {
                for terminated in [false, true] {
                    proofs::spec_wraps(previous, next, terminated);
                }
            }
        }
        let declarations = [None, Some(false), Some(true)];
        for [first, second] in flags() {
            proofs::spec_standard_applies(first, second);
            proofs::spec_exemption_valid(first, second);
            for declared in declarations {
                for origin in proofs::ORIGINS {
                    proofs::spec_repository_standard(declared, origin, first, second);
                }
            }
        }
        for before in [0, 1, usize::MAX] {
            for after in [0, 1, usize::MAX] {
                proofs::spec_hook_group(before, after);
            }
        }
        for command in [
            &b"prose reply --client claude"[..],
            &b"prose reply --client codex"[..],
            &b"skill-ops stop"[..],
            &b"prose check --channel document guide.md"[..],
            &b"echo prose reply --client claude"[..],
        ] {
            proofs::spec_reply_hook(command);
        }
        for length in 0..HOOK_PREFIX.len() {
            assert!(!reply_hook(&HOOK_PREFIX[..length]));
        }
        assert!(reply_hook(HOOK_PREFIX));
    }

    #[test]
    fn prose_layout_uses_sentence_terminators_in_both_languages() {
        for line in [
            "a.",
            "a!",
            "a?",
            "あ。",
            "あ！",
            "あ？",
            "a.)",
            "あ。」",
            "**a.**",
        ] {
            assert!(ends_sentence(line), "{line}");
        }
        for line in ["", "a", "あ", "a:", "あ："] {
            assert!(!ends_sentence(line), "{line}");
        }
        for line in [
            "go. next",
            "go! next",
            "go? next",
            "読む。書く。",
            "読む。 書く。",
            "読む！書く？",
            "漢字. next",
            "かな! next",
            "確認? next",
            "go... next",
            "go?! next",
        ] {
            assert_eq!(sentence_starts(line).len(), 1, "{line}");
        }
        for line in [
            "Use e.g. Vale.",
            "Version 1.2. Then",
            "1. Install it.",
            "Use a.b now.",
            "Go!",
            "Done.",
        ] {
            assert!(sentence_starts(line).is_empty(), "{line}");
        }
        assert_eq!(classify(". text"), Line::Prose);
        proofs::spec_layout("go\non.", Some(LayoutViolation::WrappedSentence));
        proofs::spec_layout("go. next", Some(LayoutViolation::MultipleSentences));
    }

    #[test]
    fn structural_lines_and_paragraph_breaks_do_not_wrap() {
        for text in [
            "a.\nb.",
            "a\n\nb.",
            "# a\nb.",
            "**a**\nb.",
            "a:\nb.",
            "a：\nb.",
            "- a\n  b.",
            "1. a\n   b.",
            "a\n- b.",
            "> a\n> b.",
            "| a |\n| b |",
            "a\n",
        ] {
            assert!(sentence_lines(text).is_empty(), "{text}");
        }
        let findings = sentence_lines("\na\nb.");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
        assert_eq!(findings[0].column, 1);
    }
}
