use super::{Diagnostic, Language, NlpReport, Segment, Severity, projection::Kind};
use anyhow::{Context, Result};
use rules::copyedit_rules::{self, Complexity, Reuse, ReuseKind};
use std::collections::{BTreeMap, BTreeSet};

const SHINGLE_WORDS: usize = 3;
const CONTENT_POS: [&str; 6] = ["NOUN", "PROPN", "VERB", "ADJ", "ADV", "NUM"];
const CLAUSES: [&str; 4] = ["acl", "advcl", "ccomp", "xcomp"];
const NEGATORS: [&str; 18] = [
    "not", "n't", "no", "never", "without", "ない", "ぬ", "ず", "ん", "まい", "不", "没", "没有",
    "未", "\u{522b}", "勿", "禁止", "不得",
];
const SEMANTIC_MORPHOLOGY: [&str; 4] = ["Tense=", "Aspect=", "Polarity=", "Mood="];

struct Paragraph<'a> {
    segment: &'a Segment,
    normalized: String,
    shingles: BTreeSet<Vec<String>>,
    words: u32,
    anchors: BTreeSet<String>,
}

fn count(value: usize) -> Result<u32> {
    value
        .try_into()
        .context("linguistic count exceeds its input domain")
}

fn paragraph<'a>(segment: &'a Segment, annotation: &super::Annotation) -> Result<Paragraph<'a>> {
    let words: Vec<_> = annotation
        .tokens
        .iter()
        .filter(|token| CONTENT_POS.contains(&token.pos.as_str()))
        .map(|token| {
            if token.lemma.is_empty() {
                &token.text
            } else {
                &token.lemma
            }
            .to_lowercase()
        })
        .collect();
    Ok(Paragraph {
        segment,
        normalized: segment
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        shingles: words
            .windows(SHINGLE_WORDS)
            .map(<[String]>::to_vec)
            .collect(),
        words: count(words.len())?,
        anchors: annotation
            .entities
            .iter()
            .map(|entity| format!("entity:{}:{}", entity.label, entity.text))
            .chain(annotation.tokens.iter().flat_map(|token| {
                let lemma = if token.lemma.is_empty() {
                    &token.text
                } else {
                    &token.lemma
                }
                .to_lowercase();
                let mut anchors = Vec::new();
                if matches!(token.pos.as_str(), "NUM" | "AUX") || NEGATORS.contains(&lemma.as_str())
                {
                    anchors.push(format!("{}:{lemma}", token.pos));
                }
                anchors.extend(
                    token
                        .morphology
                        .split('|')
                        .filter(|feature| {
                            SEMANTIC_MORPHOLOGY
                                .iter()
                                .any(|prefix| feature.starts_with(prefix))
                        })
                        .map(|feature| format!("{lemma}:{feature}")),
                );
                anchors
            }))
            .collect(),
    })
}

fn repeated(
    current: &Paragraph<'_>,
    previous: &[Paragraph<'_>],
    source: &str,
) -> Result<Option<Diagnostic>> {
    for earlier in previous
        .iter()
        .filter(|earlier| earlier.segment.path == current.segment.path)
    {
        let shared = current.shingles.intersection(&earlier.shingles).count();
        let facts = Reuse {
            exact: current.normalized == earlier.normalized,
            facts_match: current.anchors == earlier.anchors,
            characters: count(current.segment.text.chars().count())?,
            words: current.words.min(earlier.words),
            shared: count(shared)?,
            union: count(current.shingles.len() + earlier.shingles.len() - shared)?,
        };
        let (rule, explanation) = match copyedit_rules::duplication(facts) {
            ReuseKind::None => continue,
            ReuseKind::Exact => (
                "Content.RepeatedParagraph",
                "This paragraph repeats earlier prose",
            ),
            ReuseKind::Near => (
                "Content.LemmaOverlap",
                "This paragraph shares at least ninety percent of its content-word trigrams with earlier prose",
            ),
        };
        let earlier_span = earlier
            .segment
            .source_span(0, earlier.segment.text.chars().count())?;
        let earlier_line = source[..earlier_span.start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let span = current
            .segment
            .source_span(0, current.segment.text.chars().count())?;
        return Ok(Some(Diagnostic::source(
            &current.segment.path,
            source,
            span,
            rule,
            Severity::Review,
            "spacy+document-rules",
            &format!(
                "{explanation} at line {earlier_line}; check whether both passages add distinct information."
            ),
        )));
    }
    Ok(None)
}

fn complexity(
    language: Language,
    segment: &Segment,
    annotation: &super::Annotation,
    source: &str,
) -> Result<Vec<Diagnostic>> {
    let mut findings = Vec::new();
    for sentence in &annotation.sentences {
        let tokens: Vec<_> = annotation
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| sentence.start <= token.start && token.end <= sentence.end)
            .collect();
        let facts = Complexity {
            words: count(
                tokens
                    .iter()
                    .filter(|(_, token)| !matches!(token.pos.as_str(), "SPACE" | "PUNCT"))
                    .count(),
            )?,
            clauses: count(
                tokens
                    .iter()
                    .filter(|(_, token)| {
                        CLAUSES.contains(&token.dependency.split(':').next().unwrap_or_default())
                    })
                    .count(),
            )?,
            distance: count(
                tokens
                    .iter()
                    .map(|(index, token)| index.abs_diff(token.head))
                    .max()
                    .unwrap_or_default(),
            )?,
        };
        if copyedit_rules::complex_sentence(language.core(), facts) {
            findings.push(Diagnostic::source(&segment.path, source, segment.source_span(sentence.start, sentence.end)?, "Content.ClauseChain", Severity::Review, "spacy+document-rules", "Check the long sentence's linked clauses and distant dependencies; split independent points while preserving their logical relationships."));
        }
    }
    Ok(findings)
}

pub fn check(
    language: Language,
    segments: &[Segment],
    nlp: &NlpReport,
    sources: &BTreeMap<&String, &String>,
) -> Result<Vec<Diagnostic>> {
    let mut previous = Vec::new();
    let mut findings = Vec::new();
    for (segment, annotation) in segments.iter().zip(&nlp.annotations) {
        if !matches!(segment.kind, Kind::Paragraph) || segment.quoted {
            continue;
        }
        let source = sources
            .get(&segment.path)
            .context("paragraph lost its source")?;
        findings.extend(complexity(language, segment, annotation, source)?);
        let current = paragraph(segment, annotation)?;
        if let Some(finding) = repeated(&current, &previous, source)? {
            findings.push(finding);
        }
        previous.push(current);
    }
    Ok(findings)
}
