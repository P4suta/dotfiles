//! Reply prose excludes code and quoted artifacts before the language decision.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Language {
    Japanese,
    English,
}

/// Four words admit short acknowledgments while catching status sentences.
pub const SENTENCE_WORDS: usize = 4;
const FENCE_MARKERS: usize = 3;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    pub english_lines: usize,
    pub japanese_lines: usize,
    pub english_words: usize,
}

impl Counts {
    fn line(self, japanese: bool, words: usize) -> Self {
        if japanese {
            Self {
                japanese_lines: self.japanese_lines.saturating_add(1),
                ..self
            }
        } else if words != 0 {
            Self {
                english_lines: self.english_lines.saturating_add(1),
                english_words: self.english_words.saturating_add(words),
                ..self
            }
        } else {
            self
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Rewrite,
}

#[must_use]
pub const fn decide(language: Language, counts: Counts) -> Verdict {
    if matches!(language, Language::Japanese)
        && counts.english_words >= SENTENCE_WORDS
        && counts.english_lines > counts.japanese_lines
    {
        Verdict::Rewrite
    } else {
        Verdict::Admit
    }
}

fn japanese(character: char) -> bool {
    matches!(character,
        '\u{3041}'..='\u{3096}' | '\u{30a1}'..='\u{30fa}' | '\u{ff66}'..='\u{ff9d}'
        | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}'
        | '\u{20000}'..='\u{323af}')
}

fn fence(line: &str) -> Option<(char, usize)> {
    let marker = line.chars().next()?;
    let length = line.chars().take_while(|&value| value == marker).count();
    (matches!(marker, '`' | '~') && length >= FENCE_MARKERS).then_some((marker, length))
}

fn prose_token(token: &str) -> &str {
    if let Some((label, _)) = token.split_once("](") {
        return label;
    }
    if let Some((prefix, _)) = token.split_once("://") {
        return prefix.trim_end_matches(|character: char| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        });
    }
    if token.contains(['/', '\\', '_'])
        || token.starts_with(['#', '@'])
        || token.contains("::")
        || token
            .trim_end_matches(['.', ',', ';', ':', ')', ']', '>'])
            .contains('.')
    {
        ""
    } else {
        token
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Prose {
    japanese: bool,
    words: usize,
    tokens: usize,
}

fn latin_words(token: &str) -> usize {
    let mut words: usize = 0;
    let mut in_word = false;
    for byte in token.bytes() {
        let letter = byte.is_ascii_alphabetic();
        if letter && !in_word {
            words = words.saturating_add(1);
        }
        in_word = letter;
    }
    words
}

impl Prose {
    fn combine(self, other: Self) -> Self {
        Self {
            japanese: self.japanese || other.japanese,
            words: self.words.saturating_add(other.words),
            tokens: self.tokens.saturating_add(other.tokens),
        }
    }

    fn token(self, token: &str) -> Self {
        if token.is_empty() {
            return self;
        }
        if token.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            return self.combine(Self {
                japanese: false,
                words: 1,
                tokens: 1,
            });
        }
        let token = prose_token(token);
        if token.is_empty() {
            return self;
        }
        self.combine(Self {
            japanese: token.chars().any(japanese),
            words: latin_words(token),
            tokens: 1,
        })
    }

    fn identifiers(self) -> bool {
        !self.japanese && (self.tokens <= 1 || self.words == 0)
    }
}

fn code_span(inline: usize, ticks: usize) -> usize {
    if inline == 0 {
        ticks
    } else if inline == ticks {
        0
    } else {
        inline
    }
}

fn line_prose(line: &str, mut inline: usize) -> (Prose, usize) {
    let mut total = Prose::default();
    let mut cell = Prose::default();
    let mut identifiers = true;
    let mut table = false;
    let mut ticks = 0;
    let mut start = 0;
    for (end, character) in line.char_indices() {
        if character == '`' {
            if ticks == 0 && inline == 0 {
                cell = cell.token(line.get(start..end).unwrap_or_default());
            }
            ticks += 1;
            start = end + character.len_utf8();
            continue;
        }
        if ticks != 0 {
            inline = code_span(inline, ticks);
            ticks = 0;
            start = end;
        }
        if inline != 0 {
            start = end + character.len_utf8();
            continue;
        }
        if character == '|' || character.is_whitespace() {
            cell = cell.token(line.get(start..end).unwrap_or_default());
            start = end + character.len_utf8();
            if character == '|' {
                table = true;
                identifiers &= cell.identifiers();
                total = total.combine(cell);
                cell = Prose::default();
            }
        }
    }
    if ticks != 0 {
        inline = code_span(inline, ticks);
    } else if inline == 0 {
        cell = cell.token(line.get(start..).unwrap_or_default());
    }
    if table && identifiers && cell.identifiers() {
        (Prose::default(), inline)
    } else {
        (total.combine(cell), inline)
    }
}

fn count(reply: &str) -> Counts {
    let mut counts = Counts::default();
    let mut fenced = None;
    let mut inline = 0;
    for line in reply.lines() {
        let line = line.trim();
        if line.starts_with('>') {
            continue;
        }
        let marker = fence(line);
        if let Some((opening, length)) = fenced {
            if marker.is_some_and(|(closing, size)| {
                closing == opening && size >= length && line.trim_matches(closing).trim().is_empty()
            }) {
                fenced = None;
            }
            continue;
        }
        if marker.is_some() && inline == 0 {
            fenced = marker;
            continue;
        }
        let (prose, next_inline) = line_prose(line, inline);
        inline = next_inline;
        counts = counts.line(prose.japanese, prose.words);
    }
    counts
}

#[must_use]
pub fn check(language: Language, reply: &str) -> Verdict {
    match language {
        Language::English => Verdict::Admit,
        Language::Japanese => decide(language, count(reply)),
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const LINES: usize = 4;
    pub const WORD_COUNTS: u8 = 6;

    pub fn spec_counting(lines: [u8; LINES], language: Language) {
        let mut counts = Counts::default();
        let mut english_lines = 0;
        let mut japanese_lines = 0;
        let mut english_words = 0;
        for line in lines {
            let japanese = line & 1 != 0;
            let words = usize::from((line >> 1) % WORD_COUNTS);
            counts = counts.line(japanese, words);
            match (japanese, words) {
                (true, _) => japanese_lines += 1,
                (false, 0) => {}
                (false, _) => {
                    english_lines += 1;
                    english_words += words;
                }
            }
        }
        assert_eq!(
            counts,
            Counts {
                english_lines,
                japanese_lines,
                english_words
            }
        );
        let refuses =
            language == Language::Japanese && english_words >= 4 && english_lines > japanese_lines;
        assert_eq!(decide(language, counts) == Verdict::Rewrite, refuses);
    }

    pub fn spec_reply(reply: &str, japanese: Verdict) {
        assert_eq!(check(Language::Japanese, reply), japanese);
        assert_eq!(check(Language::English, reply), Verdict::Admit);
        #[cfg(kani)]
        {
            kani::cover!(check(Language::English, reply) == Verdict::Admit);
            kani::cover!(check(Language::Japanese, "a b c d") == Verdict::Rewrite);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(5)]
    fn bounded_lines_refuse_only_sentence_length_english_majorities() {
        let lines: [u8; LINES] = kani::any();
        let language: Language = kani::any();
        spec_counting(lines, language);
        let mut counts = Counts::default();
        for line in lines {
            counts = counts.line(line & 1 != 0, usize::from((line >> 1) % WORD_COUNTS));
        }
        kani::cover!(decide(language, counts) == Verdict::Rewrite);
        kani::cover!(
            language == Language::Japanese
                && counts.japanese_lines != 0
                && decide(language, counts) == Verdict::Admit
        );
        kani::cover!(
            language == Language::English
                && counts.english_words >= SENTENCE_WORDS
                && decide(language, counts) == Verdict::Admit
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn plain_prose_reaches_the_decision() {
        spec_reply("a b c d", Verdict::Rewrite);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn bullet_prose_reaches_the_decision() {
        spec_reply("- a b c d", Verdict::Rewrite);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(12)]
    fn table_prose_reaches_the_decision() {
        spec_reply("| a b c d |", Verdict::Rewrite);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn inline_code_has_no_prose() {
        spec_reply("`a b c d`", Verdict::Admit);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn quoted_text_has_no_prose() {
        spec_reply("> a b c d", Verdict::Admit);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn fenced_code_has_no_prose() {
        spec_reply("```\na b c d\n```", Verdict::Admit);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn japanese_prose_keeps_english_identifiers() {
        spec_reply("かな CI PR", Verdict::Admit);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn an_empty_reply_has_no_prose() {
        spec_reply("", Verdict::Admit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_markdown_prose_is_refused() {
        for reply in [
            "## Change summary\nThe requested change is now complete for #300.",
            "- Added the requested hook.\n- Verified it with `just check`.",
            "| Item | Result |\n| --- | --- |\n| #300 | The requested change is complete |",
            "The verification is now complete.\n```text\n[##########] 100%\n```\nAll required checks have passed.",
            "The change is ready for review.\n日本語の行です。\nAll checks passed successfully.",
            "All checks have [passed](https://example.invalid/日本語).",
            "All checks have [passed](/fixture/report.md).",
        ] {
            assert_eq!(
                check(Language::Japanese, reply),
                Verdict::Rewrite,
                "{reply}"
            );
        }
    }

    #[test]
    fn japanese_prose_keeps_english_identifiers_and_artifact_drafts() {
        for reply in [
            "CI と PR、pre-push、CodeRabbit を確認しました。",
            "## 検証結果\n`just check` は成功しました。\n```sh\njust check\n```",
            "変更しました。\n```text\nfeat: add the requested hook\n\n## Why\nThe hook checks the final reply.\n```",
            "確認しました。\n> The pull request is ready for review.\n> All required checks have passed.",
            "日本語の本文です。\nThe change is now ready.",
            "確認しました。https://example.invalid/report\nAll checks passed successfully.",
            "かな\nカナ\n漢字\nｶﾅ\n𠮷",
        ] {
            assert_eq!(check(Language::Japanese, reply), Verdict::Admit, "{reply}");
        }
    }

    #[test]
    fn empty_code_references_identifier_tables_and_acknowledgments_pass() {
        for reply in [
            "",
            " ",
            "OK.",
            "Done, thank you.",
            "```sh\njust check\n```",
            "~~~text\nThe requested change is now complete.\n~~~",
            "````md\n```text\nThe requested change is now complete.\n```\n````",
            "`the entire reply is code`",
            "``the `whole reply` is code``",
            "`code spans\nseveral lines`",
            "> The requested change is now complete.",
            "https://example.invalid/the/requested/change/is/complete",
            "/Users/fixture/the/requested/change/is/complete C:\\fixture\\code ./src/hook.rs",
            "| #300 | CI | `just check` | src/hook.rs |\n| --- | --- | --- | --- |",
        ] {
            assert_eq!(check(Language::Japanese, reply), Verdict::Admit, "{reply}");
        }
    }

    #[test]
    fn stripped_japanese_does_not_disguise_english_prose() {
        for reply in [
            "`日本語` The requested change is now complete.",
            "The requested change is now complete.\n```text\n日本語\n```",
            "The requested change is now complete.\n> 日本語",
            "The requested change is now complete. https://example.invalid/日本語",
        ] {
            assert_eq!(
                check(Language::Japanese, reply),
                Verdict::Rewrite,
                "{reply}"
            );
        }
    }

    #[test]
    fn threshold_ties_and_english_configuration_match_the_specification() {
        proofs::spec_reply("a b c d", Verdict::Rewrite);
        assert_eq!(check(Language::Japanese, "One two three"), Verdict::Admit);
        assert_eq!(
            check(Language::Japanese, "One two three four"),
            Verdict::Rewrite
        );
        assert_eq!(
            check(Language::Japanese, "One two\nthree four"),
            Verdict::Rewrite
        );
        assert_eq!(
            check(Language::Japanese, "One\u{3000}two\u{85}three\tfour"),
            Verdict::Rewrite
        );
        assert_eq!(
            check(Language::Japanese, "One two three four\n確認しました。"),
            Verdict::Admit
        );
        assert_eq!(
            check(Language::English, "One two three four"),
            Verdict::Admit
        );
        assert_eq!(
            check(Language::English, "日本語でも通ります。"),
            Verdict::Admit
        );
    }

    #[test]
    fn every_bounded_counting_input_meets_the_specification() {
        let cases = proofs::WORD_COUNTS * 2;
        for first in 0..cases {
            for second in 0..cases {
                for third in 0..cases {
                    for fourth in 0..cases {
                        for language in [Language::Japanese, Language::English] {
                            proofs::spec_counting([first, second, third, fourth], language);
                        }
                    }
                }
            }
        }
    }
}
