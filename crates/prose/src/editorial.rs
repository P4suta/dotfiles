//! Local editorial candidates without external language engines or automatic rewriting.

use crate::copyedit::{Kind, Segment, project_body};
use anyhow::{Result, ensure};
use pulldown_cmark::{BrokenLink, Event, LinkType, Options, Parser, Tag, TagEnd};
use rules::editorial_rules::{
    Context, MIN_DETAILED_REPORT_CHARACTERS, marker_review, report_length_review,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ops::Range;

const REPORT_VERSION: u32 = 1;
const MAX_SOURCE_BYTES: usize = 2_000_000;
const DUPLICATE_CHARACTERS: usize = 60;

#[derive(Clone, Copy, Debug, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Expository,
    General,
    #[serde(rename = "detailed-report")]
    DetailedReport,
}

impl Profile {
    const fn expository(self) -> bool {
        matches!(self, Self::Expository | Self::DetailedReport)
    }
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub line: usize,
    pub column: usize,
    pub rule: String,
    pub message: String,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct Source {
    pub path: String,
    pub sha256: String,
    pub blocks: usize,
    pub authored_body_characters: usize,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub version: u32,
    pub profile: Profile,
    pub sources: Vec<Source>,
}

impl Report {
    #[must_use]
    pub fn new(profile: Profile, sources: Vec<Source>) -> Self {
        Self {
            version: REPORT_VERSION,
            profile,
            sources,
        }
    }

    #[must_use]
    pub fn clean(&self) -> bool {
        !self.sources.is_empty() && self.sources.iter().all(|source| source.findings.is_empty())
    }
}

struct Marker {
    rule: &'static str,
    message: &'static str,
    phrases: &'static [&'static str],
}

const MARKERS: &[Marker] = &[
    Marker {
        rule: "editorial.personal-reaction",
        message: "Replace the author's reaction with the supported result or remove it.",
        phrases: &[
            "筆者には",
            "私には",
            "in my opinion",
            "i think that",
            "i believe that",
            "在我看来",
            "我\u{8ba4}\u{4e3a}",
        ],
    },
    Marker {
        rule: "editorial.vague-benefit",
        message: "State the specific distinction, mechanism, or result instead of promising understanding.",
        phrases: &[
            "理解しやすい",
            "手掛かりになる",
            "手がかりになる",
            "helps you understand",
            "easier to understand",
            "有助于理解",
        ],
    },
    Marker {
        rule: "editorial.external-handoff",
        message: "Supply the requested facts, example, or verification in this text and cite the supporting source.",
        phrases: &[
            "参照してほしい",
            "詳しくは",
            "確認する必要がある",
            "please refer to",
            "see the documentation for details",
            "\u{8be6}情\u{8bf7}",
            "\u{8bf7}参\u{9605}",
        ],
    },
    Marker {
        rule: "editorial.reader-admonition",
        message: "Replace the generic reminder with the concrete condition and its observable consequence.",
        phrases: &[
            "混同しないように",
            "留意する必要がある",
            "it is important to note",
            "it's important to note",
            "\u{5e94}当注意的是",
        ],
    },
    Marker {
        rule: "editorial.process-commentary",
        message: "Remove routine research-process declarations and present the sourced information directly.",
        phrases: &[
            "事実関係は公式資料に基づ",
            "筆者の解釈として示す",
            "これは筆者の解釈",
            "facts are based on official sources",
            "the following is my interpretation",
        ],
    },
];

fn finding(source: &str, range: Range<usize>, rule: &'static str, message: &str) -> Finding {
    let prefix = source.get(..range.start).unwrap_or_default();
    Finding {
        line: prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
        column: prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count()
            + 1,
        rule: rule.to_owned(),
        message: message.to_owned(),
        snippet: source.get(range).unwrap_or_default().to_owned(),
    }
}

fn unfinished(text: &str) -> bool {
    text.split(|character: char| !character.is_alphanumeric() && character != '_')
        .any(|word| matches!(word, "TODO" | "TBD" | "FIXME"))
        || ["要出典", "要確認", "待\u{8865}充", "待核\u{5b9e}"]
            .iter()
            .any(|word| text.contains(word))
}

fn source_link(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

fn caption(text: &str) -> bool {
    let text = text.trim_start().to_lowercase();
    ["source:", "source：", "出典", "来源"]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

fn encloses(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

#[derive(Default)]
struct Structure {
    broken: Vec<Range<usize>>,
    headings: Vec<(u8, Range<usize>)>,
    body: Vec<Range<usize>>,
    paragraphs: Vec<Range<usize>>,
    quotes: Vec<Range<usize>>,
    links: Vec<Range<usize>>,
    definitions: BTreeMap<String, Range<usize>>,
    references: Vec<(String, Range<usize>)>,
}

fn parse_structure(source: &str) -> Structure {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH;
    let mut broken = Vec::new();
    let events = {
        let callback = |link: BrokenLink<'_>| {
            if matches!(
                link.link_type,
                LinkType::Reference
                    | LinkType::ReferenceUnknown
                    | LinkType::Collapsed
                    | LinkType::CollapsedUnknown
            ) {
                broken.push(link.span);
            }
            None
        };
        Parser::new_with_broken_link_callback(source, options, Some(callback))
            .into_offset_iter()
            .collect::<Vec<_>>()
    };
    let mut result = Structure {
        broken,
        ..Structure::default()
    };
    let mut quote_start = None;
    let mut quote_depth: usize = 0;
    for (event, range) in &events {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                result.headings.push((*level as u8, range.clone()));
            }
            Event::Start(Tag::Paragraph) => {
                result.paragraphs.push(range.clone());
                result.body.push(range.clone());
            }
            Event::Start(Tag::CodeBlock(_) | Tag::Table(_)) => result.body.push(range.clone()),
            Event::Start(Tag::Link { dest_url, .. }) if source_link(dest_url) => {
                result.links.push(range.clone());
            }
            Event::Start(Tag::FootnoteDefinition(id)) => {
                result.definitions.insert(id.to_string(), range.clone());
            }
            Event::FootnoteReference(id) => result.references.push((id.to_string(), range.clone())),
            Event::Start(Tag::BlockQuote(_)) => {
                if quote_depth == 0 {
                    quote_start = Some(range.start);
                }
                quote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                quote_depth = quote_depth.saturating_sub(1);
                if quote_depth == 0
                    && let Some(start) = quote_start.take()
                {
                    result.quotes.push(start..range.end);
                }
            }
            _ => {}
        }
    }
    result
}

fn attributed(source: &str, structure: &Structure, quote: &Range<usize>) -> bool {
    let adjacent = structure
        .paragraphs
        .iter()
        .filter(|range| range.start >= quote.end)
        .min_by_key(|range| range.start)
        .into_iter()
        .chain(
            structure
                .paragraphs
                .iter()
                .filter(|range| range.end <= quote.start)
                .max_by_key(|range| range.end),
        );
    structure.links.iter().any(|link| encloses(quote, link))
        || adjacent.into_iter().any(|range| {
            let gap = if range.start >= quote.end {
                quote.end..range.start
            } else {
                range.end..quote.start
            };
            caption(source.get(range.clone()).unwrap_or_default())
                && source.get(gap).is_some_and(|text| text.trim().is_empty())
                && structure.links.iter().any(|link| encloses(range, link))
        })
        || structure.references.iter().any(|(id, range)| {
            encloses(quote, range)
                && structure.definitions.get(id).is_some_and(|definition| {
                    structure
                        .links
                        .iter()
                        .any(|link| encloses(definition, link))
                })
        })
}

fn structure(source: &str, profile: Profile) -> Vec<Finding> {
    let parsed = parse_structure(source);
    let mut findings = Vec::new();
    for range in &parsed.broken {
        findings.push(finding(
            source,
            range.clone(),
            "editorial.broken-reference",
            "Define this Markdown reference or replace it with the intended source link.",
        ));
    }
    for (id, range) in &parsed.references {
        if !parsed.definitions.contains_key(id) {
            findings.push(finding(
                source,
                range.clone(),
                "editorial.broken-reference",
                "Define the cited footnote.",
            ));
        }
    }
    for (index, (level, range)) in parsed.headings.iter().enumerate() {
        let next = parsed.headings.get(index + 1);
        let end = next.map_or(source.len(), |(_, next)| next.start);
        if next.is_none_or(|(next, _)| next <= level)
            && !parsed
                .body
                .iter()
                .any(|body| range.end <= body.start && body.start < end)
        {
            findings.push(finding(
                source,
                range.clone(),
                "editorial.empty-section",
                "Supply the section's promised content or remove the heading.",
            ));
        }
    }
    if profile.expository() {
        for quote in &parsed.quotes {
            if !attributed(source, &parsed, quote) {
                findings.push(finding(source, quote.clone(), "editorial.unattributed-quote", "Attach a source link to this quotation, an adjacent source caption, or its linked footnote."));
            }
        }
    }
    findings
}

fn authored_body_characters(segments: &[Segment]) -> usize {
    segments
        .iter()
        .filter(|segment| !segment.quoted && matches!(segment.kind, Kind::Paragraph))
        .map(|segment| {
            segment
                .text
                .chars()
                .filter(|character| !character.is_whitespace())
                .count()
        })
        .sum()
}

/// Inspects authored Markdown body text and reference structure without changing the source.
///
/// # Errors
///
/// Refuses source or projected blocks beyond the admitted size limits.
pub fn inspect(path: &str, source: &str, profile: Profile) -> Result<Source> {
    ensure!(
        source.len() <= MAX_SOURCE_BYTES,
        "{path} exceeds {MAX_SOURCE_BYTES} bytes"
    );
    let segments = project_body(path, source)?;
    let mut findings = structure(source, profile);
    let authored_body_characters = authored_body_characters(&segments);
    if report_length_review(
        matches!(profile, Profile::DetailedReport),
        authored_body_characters,
    ) {
        findings.push(finding(
            source,
            0..0,
            "editorial.report-too-short",
            &format!("Expand the detailed report's authored explanation: {authored_body_characters} non-whitespace body characters; at least {MIN_DETAILED_REPORT_CHARACTERS} required. Code, quotations, headings, metadata, captions, and link destinations don't count."),
        ));
    }
    let mut paragraphs = BTreeMap::new();
    for segment in &segments {
        let context = if segment.quoted {
            Context::Quotation
        } else {
            Context::Authored
        };
        for marker in MARKERS {
            let lower = segment.text.to_ascii_lowercase();
            if let Some((offset, phrase)) = marker
                .phrases
                .iter()
                .filter_map(|phrase| lower.find(phrase).map(|offset| (offset, phrase)))
                .min_by_key(|(offset, _)| *offset)
                && marker_review(true, profile.expository(), context, true)
            {
                let start = lower.get(..offset).unwrap_or_default().chars().count();
                let end = start + phrase.chars().count();
                findings.push(finding(
                    source,
                    segment.source_span(start, end)?,
                    marker.rule,
                    marker.message,
                ));
            }
        }
        if marker_review(
            unfinished(&segment.text),
            profile.expository(),
            context,
            false,
        ) {
            findings.push(finding(
                source,
                segment.source_span(0, segment.text.chars().count())?,
                "editorial.unfinished-text",
                "Complete this editorial placeholder before delivery.",
            ));
        }
        if profile.expository() && !segment.quoted && matches!(segment.kind, Kind::Paragraph) {
            let normalized = segment
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if normalized.chars().count() >= DUPLICATE_CHARACTERS {
                let range = segment.source_span(0, segment.text.chars().count())?;
                if let Some(previous) = paragraphs.insert(normalized, range.start) {
                    let line = source
                        .get(..previous)
                        .unwrap_or_default()
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count()
                        + 1;
                    findings.push(finding(source, range, "editorial.duplicate-paragraph", &format!("This paragraph repeats line {line}; retain both only if each occurrence serves a stated purpose.")));
                }
            }
        }
    }
    if !segments
        .iter()
        .any(|segment| !matches!(segment.kind, Kind::Heading | Kind::Metadata))
    {
        findings.push(finding(
            source,
            0..0,
            "editorial.empty-document",
            "Supply the article body before requesting an editorial pass.",
        ));
    }
    findings.sort_by(|left, right| {
        (left.line, left.column, &left.rule).cmp(&(right.line, right.column, &right.rule))
    });
    Ok(Source {
        path: path.to_owned(),
        sha256: crate::build_inputs::hex(&Sha256::digest(source.as_bytes())),
        blocks: segments.len(),
        authored_body_characters,
        findings,
    })
}
