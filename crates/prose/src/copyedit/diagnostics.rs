use super::{Annotation, Language, Lexeme, Segment};
use anyhow::Result;
use rules::copyedit_rules::{
    self as decisions, AgreementClass, AgreementContext, Determiner, ModifierRole, Protection,
    Repetition, Role, Script, Space, SpaceFault,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ops::Range;

const FUNCTION_POS: [&str; 6] = ["ADP", "AUX", "CCONJ", "DET", "PART", "SCONJ"];

fn invisible(character: char) -> bool {
    matches!(character, '\u{200b}' | '\u{feff}' | '\u{2060}' | '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn separator(character: char) -> bool {
    (character.is_whitespace() && !matches!(character, '\n' | '\r')) || invisible(character)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Review,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub start: usize,
    pub end: usize,
    pub rule: String,
    pub severity: Severity,
    pub engine: String,
    pub message: String,
    pub excerpt: String,
    pub identity: String,
}

impl Diagnostic {
    #[must_use]
    pub fn source(
        path: &str,
        source: &str,
        span: Range<usize>,
        rule: &str,
        severity: Severity,
        engine: &str,
        message: &str,
    ) -> Self {
        let prefix = &source[..span.start];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count()
            + 1;
        let excerpt = source[span.clone()].to_owned();
        let identity = crate::build_inputs::hex(&Sha256::digest(format!(
            "{path}\0{rule}\0{}\0{}\0{excerpt}",
            span.start, span.end
        )));
        Self {
            path: path.to_owned(),
            line,
            column,
            start: span.start,
            end: span.end,
            rule: rule.to_owned(),
            severity,
            engine: engine.to_owned(),
            message: message.to_owned(),
            excerpt,
            identity,
        }
    }
}

struct Context<'a> {
    segment: &'a Segment,
    annotation: &'a Annotation,
    source: &'a str,
    findings: Vec<Diagnostic>,
}

impl Context<'_> {
    fn add(
        &mut self,
        start: usize,
        end: usize,
        rule: &str,
        severity: Severity,
        message: &str,
    ) -> Result<()> {
        let span = self.segment.source_span(start, end)?;
        self.findings.push(Diagnostic::source(
            &self.segment.path,
            self.source,
            span,
            rule,
            severity,
            "spacy+rules",
            message,
        ));
        Ok(())
    }

    fn entity(&self, start: usize, end: usize) -> bool {
        self.annotation
            .entities
            .iter()
            .any(|entity| entity.start <= start && end <= entity.end)
    }

    fn simplified(&mut self) -> Result<()> {
        let Some(converted) = &self.annotation.simplified else {
            return Ok(());
        };
        if converted == &self.segment.text {
            return Ok(());
        }
        let original: Vec<_> = self.segment.text.chars().collect();
        let simplified: Vec<_> = converted.chars().collect();
        if original.len() != simplified.len() {
            self.add(0, original.len(), "Chinese.ScriptConsistency", Severity::Review, "Check simplified Chinese consistency against the dictionary conversion; preserve verified names and literal quotations.")?;
            return Ok(());
        }
        for (index, (left, right)) in original.iter().zip(&simplified).enumerate() {
            if left != right && !self.segment.quoted && !self.entity(index, index + 1) {
                self.add(index, index + 1, "Chinese.ScriptConsistency", Severity::Review, &format!("Check simplified Chinese consistency: the pinned dictionary suggests {right}; preserve verified names and literal quotations."))?;
            }
        }
        Ok(())
    }

    fn repeated_functions(&mut self) -> Result<()> {
        for pair in self.annotation.tokens.windows(2) {
            let [left, right] = pair else { continue };
            let adjacent = self
                .segment
                .text
                .chars()
                .skip(left.end)
                .take(right.start - left.end)
                .all(|character| character.is_whitespace() && character != '\n');
            let facts = Repetition {
                same_surface: left.text.eq_ignore_ascii_case(&right.text),
                role: if FUNCTION_POS.contains(&left.pos.as_str())
                    && FUNCTION_POS.contains(&right.pos.as_str())
                {
                    Role::Function
                } else {
                    Role::Content
                },
                adjacent,
                protected: self.segment.quoted || self.entity(left.start, right.end),
            };
            if decisions::repeated_function(facts) {
                self.add(left.start, right.end, "Grammar.RepeatedFunction", Severity::Review, "Check the repeated adjacent function word against the sentence's intended meaning.")?;
            }
        }
        Ok(())
    }

    fn english(&mut self) -> Result<()> {
        for (index, token) in self.annotation.tokens.iter().enumerate() {
            let head = &self.annotation.tokens[token.head];
            if token.dependency == "nsubj" && matches!(head.pos.as_str(), "VERB" | "AUX") {
                let verb = self
                    .annotation
                    .tokens
                    .iter()
                    .find(|candidate| {
                        candidate.head == token.head
                            && candidate.dependency == "aux"
                            && matches!(candidate.tag.as_str(), "VBZ" | "VBP")
                    })
                    .unwrap_or(head);
                let context =
                    AgreementContext {
                        present: matches!(verb.tag.as_str(), "VB" | "VBP" | "VBZ")
                            && !self.annotation.tokens.iter().any(|candidate| {
                                candidate.head == token.head && candidate.tag == "MD"
                            }),
                        coordinated: self.annotation.tokens.iter().any(|candidate| {
                            candidate.head == index && candidate.dependency == "conj"
                        }),
                        protected: self.segment.quoted,
                    };
                if decisions::disagrees(subject(token), predicate(verb), context) {
                    self.add(
                        token.start.min(verb.start),
                        token.end.max(verb.end),
                        "Grammar.SubjectVerbAgreement",
                        Severity::Review,
                        "Check the present-tense verb against its subject's person and number.",
                    )?;
                }
            }
            let determiner = Determiner {
                singular: token.dependency == "det"
                    && matches!(
                        token.text.to_ascii_lowercase().as_str(),
                        "a" | "an" | "this" | "that"
                    ),
                plural_noun: matches!(head.tag.as_str(), "NNS" | "NNPS")
                    && matches!(
                        head.dependency.as_str(),
                        "nsubj" | "nsubjpass" | "dobj" | "pobj" | "attr"
                    ),
                finite_clause: self.annotation.tokens.iter().any(|candidate| {
                    matches!(candidate.pos.as_str(), "VERB" | "AUX")
                        && matches!(candidate.tag.as_str(), "VBD" | "VBP" | "VBZ")
                }),
                protected: if self.segment.quoted {
                    Protection::Literal
                } else {
                    Protection::Prose
                },
            };
            if decisions::determiner_number(determiner) {
                self.add(
                    token.start.min(head.start),
                    token.end.max(head.end),
                    "Grammar.DeterminerNumber",
                    Severity::Review,
                    "Check this singular determiner with the plural noun it modifies.",
                )?;
            }
        }
        Ok(())
    }

    fn chinese(&mut self) -> Result<()> {
        for token in &self.annotation.tokens {
            let role = if token.pos == "PART" && token.tag == "DEV" && token.dependency == "mark" {
                ModifierRole::Adverbial
            } else {
                ModifierRole::Other
            };
            if decisions::chinese_modifier(
                role,
                token.text == "的",
                self.segment.quoted || self.entity(token.start, token.end),
            ) {
                self.add(token.start, token.end, "Chinese.AdverbialMarker", Severity::Review, "Check the adverbial marker: this dependency role usually takes 地 rather than attributive 的 in edited prose.")?;
            }
        }
        Ok(())
    }

    fn spacing(&mut self) -> Result<()> {
        let characters: Vec<_> = self.segment.text.chars().collect();
        let mut index = 0;
        while index < characters.len() {
            let character = characters[index];
            if !separator(character) {
                index += 1;
                continue;
            }
            let start = index;
            while characters
                .get(index)
                .is_some_and(|character| separator(*character))
            {
                index += 1;
            }
            let run = &characters[start..index];
            let space = if run.iter().copied().any(invisible) {
                Space::Invisible
            } else if run.contains(&'　') {
                Space::FullWidth
            } else if run.len() > 1 || run.contains(&'\t') {
                Space::Repeated
            } else if run
                .iter()
                .any(|character| !matches!(character, ' ' | '\u{a0}' | '\u{202f}'))
            {
                Space::Unusual
            } else {
                Space::Ordinary
            };
            let left = start
                .checked_sub(1)
                .and_then(|offset| characters.get(offset))
                .copied()
                .map_or(Script::Other, script);
            let right = characters.get(index).copied().map_or(Script::Other, script);
            let protected = self.segment.quoted
                || self.entity(start.saturating_sub(1), (index + 1).min(characters.len()));
            let fault = decisions::spacing(left, right, space, protected);
            let (rule, severity, message) = match fault {
                SpaceFault::None => continue,
                SpaceFault::Invisible => (
                    "Typography.InvisibleSpace",
                    Severity::Error,
                    "Remove the invisible formatting separator from visible prose.",
                ),
                SpaceFault::FullWidth => (
                    "Typography.FullWidthSpace",
                    Severity::Error,
                    "Use paragraph structure rather than a full-width space for alignment.",
                ),
                SpaceFault::Repeated => (
                    "Typography.RepeatedSpace",
                    Severity::Error,
                    "Use one semantic space instead of repeated spaces or tabs.",
                ),
                SpaceFault::Punctuation => (
                    "Typography.PunctuationSpace",
                    Severity::Error,
                    "Remove the space inside a bracket or next to CJK punctuation.",
                ),
                SpaceFault::CjkInterior => (
                    "Typography.CjkInteriorSpace",
                    Severity::Review,
                    "Check this space within CJK prose; preserve a verified name or intentional word separator.",
                ),
                SpaceFault::Unusual => (
                    "Typography.UnusualSpace",
                    Severity::Review,
                    "Check the unusual Unicode space; preserve required numeric grouping and replace visual alignment with paragraph structure.",
                ),
            };
            self.add(start, index, rule, severity, message)?;
        }
        Ok(())
    }
}

fn subject(token: &Lexeme) -> AgreementClass {
    match token.text.to_ascii_lowercase().as_str() {
        "he" | "she" | "it" => AgreementClass::ThirdSingular,
        "i" | "you" | "we" | "they" => AgreementClass::Other,
        _ => match token.tag.as_str() {
            "NN" | "NNP" => AgreementClass::ThirdSingular,
            "NNS" | "NNPS" => AgreementClass::Other,
            _ => AgreementClass::Unknown,
        },
    }
}

fn predicate(token: &Lexeme) -> AgreementClass {
    match token.tag.as_str() {
        "VBZ" => AgreementClass::ThirdSingular,
        "VB" | "VBP" => AgreementClass::Other,
        _ => AgreementClass::Unknown,
    }
}

fn script(character: char) -> Script {
    match character {
        'a'..='z' | 'A'..='Z' => Script::Latin,
        '0'..='9' => Script::Digit,
        '（' | '(' | '「' | '『' | '【' | '《' | '“' => Script::Opening,
        '）' | ')' | '」' | '』' | '】' | '》' | '”' => Script::Closing,
        '。' | '、' | '，' | '！' | '？' | '；' | '：' => Script::CjkPunctuation,
        '\u{3400}'..='\u{4dbf}'
        | '\u{4e00}'..='\u{9fff}'
        | '\u{3040}'..='\u{30ff}'
        | '\u{20000}'..='\u{323af}' => Script::Cjk,
        _ => Script::Other,
    }
}

pub fn check(
    language: Language,
    segment: &Segment,
    annotation: &Annotation,
    source: &str,
) -> Result<Vec<Diagnostic>> {
    let mut context = Context {
        segment,
        annotation,
        source,
        findings: Vec::new(),
    };
    context.repeated_functions()?;
    context.spacing()?;
    if language == Language::Chinese {
        context.simplified()?;
        context.chinese()?;
    }
    if language == Language::English {
        context.english()?;
    }
    Ok(context.findings)
}
