//! Shared terminology and Japanese reference policy for prose and answer evidence.

use crate::copyedit::{Annotation, Diagnostic, Language, Segment, Severity};
use anyhow::{Result, ensure};
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rules::editorial_rules::{Context, Layout, layout_review};
use serde::Deserialize;
use std::collections::BTreeSet;

const POLICY: &str = include_str!("../../../home/dot_config/prose/house.toml");
const SCHEMA: u32 = 1;
const MAX_POLICY_BYTES: usize = 65_536;
const MAX_TERMS: usize = 128;
const MAX_FORM_CHARACTERS: usize = 128;
pub const DEMONSTRATIVE: &str = "House.JapaneseDemonstrative";
pub const TERMINOLOGY: &str = "House.TechnicalTerm";
pub const LIST: &str = "House.List";
pub const TABLE: &str = "House.Table";
const MAX_LAYOUT_BYTES: usize = 1_048_576;

/// Checks authored Markdown and HTML layout while preserving code, metadata, and quotations.
///
/// # Errors
///
/// Refuses a source beyond the document size limit.
pub fn layout_findings(path: &str, source: &str) -> Result<Vec<Diagnostic>> {
    ensure!(
        source.len() <= MAX_LAYOUT_BYTES,
        "prose layout exceeds its size limit"
    );
    let options = Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut events = Vec::new();
    let mut metadata = false;
    let mut quotes = 0_usize;
    let mut locations = Vec::new();
    let mut html_start = None;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match &event {
            Event::Start(Tag::MetadataBlock(_)) => metadata = true,
            Event::End(TagEnd::MetadataBlock(_)) => metadata = false,
            _ if metadata => continue,
            Event::Start(Tag::BlockQuote(_)) => quotes += 1,
            Event::End(TagEnd::BlockQuote(_)) => quotes = quotes.saturating_sub(1),
            Event::Start(Tag::List(_)) if quotes == 0 => {
                locations.push((Layout::List, range.start));
            }
            Event::Start(Tag::Table(_)) if quotes == 0 => {
                locations.push((Layout::Table, range.start));
            }
            Event::Html(_) | Event::InlineHtml(_) if quotes == 0 => {
                html_start.get_or_insert(range.start);
            }
            _ => {}
        }
        if !matches!(
            event,
            Event::Start(Tag::MetadataBlock(_)) | Event::End(TagEnd::MetadataBlock(_))
        ) {
            events.push(event);
        }
    }
    let mut rendered = String::new();
    pulldown_cmark::html::push_html(&mut rendered, events.into_iter());
    let document =
        html5ever::parse_document(RcDom::default(), html5ever::ParseOpts::default()).one(rendered);
    let mut layouts = Vec::new();
    let mut pending = vec![(document.document.clone(), Context::Authored)];
    while let Some((node, inherited)) = pending.pop() {
        let name = match &node.data {
            NodeData::Element { name, .. } => name.local.as_ref(),
            _ => "",
        };
        let context = if inherited == Context::Quotation
            || matches!(name, "blockquote" | "pre" | "code" | "script" | "style")
        {
            Context::Quotation
        } else {
            Context::Authored
        };
        let layout = match name {
            "ul" | "ol" | "dl" => Layout::List,
            "table" => Layout::Table,
            _ => Layout::Flow,
        };
        if layout_review(context, layout) && !layouts.contains(&layout) {
            layouts.push(layout);
        }
        pending.extend(
            node.children
                .borrow()
                .iter()
                .cloned()
                .map(|child| (child, context)),
        );
    }
    Ok(layouts.into_iter().filter_map(|layout| {
        let start = locations.iter().find(|(candidate, _)| *candidate == layout)
            .map_or_else(|| html_start.unwrap_or_default(), |(_, start)| *start);
        let end = start + source.get(start..).and_then(|text| text.chars().next()).map_or(0, char::len_utf8);
        let (rule, message) = match layout {
            Layout::List => (LIST, "Use connected paragraphs or a diagram of the actual relationships instead of a bulleted or numbered list."),
            Layout::Table => (TABLE, "Express the comparison in connected prose or a suitable diagram instead of a table."),
            Layout::Flow => return None,
        };
        Some(Diagnostic::source(path, source, start..end, rule, Severity::Error, "pulldown-cmark+html5ever", message))
    }).collect())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    schema_version: u32,
    standard: String,
    edition: String,
    japanese: Japanese,
    term: Vec<Term>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Japanese {
    demonstratives: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Term {
    preferred: String,
    aliases: Vec<String>,
    source: String,
}

impl Policy {
    /// Reads the bounded terminology schema and refuses ambiguous or unsourced forms.
    ///
    /// # Errors
    ///
    /// Refuses unknown fields, unsupported versions, empty data, duplicate aliases, and invalid source identities.
    pub fn parse(source: &str) -> Result<Self> {
        ensure!(
            source.len() <= MAX_POLICY_BYTES,
            "house policy exceeds its size limit"
        );
        let policy: Self = toml::from_str(source)?;
        ensure!(
            policy.schema_version == SCHEMA
                && policy.standard.starts_with("https://")
                && !policy.edition.trim().is_empty(),
            "house policy needs its supported schema and identified standard"
        );
        ensure!(
            !policy.term.is_empty()
                && policy.term.len() <= MAX_TERMS
                && !policy.japanese.demonstratives.is_empty(),
            "house policy has empty or excessive coverage"
        );
        let mut forms = BTreeSet::new();
        for form in &policy.japanese.demonstratives {
            ensure!(
                valid_form(form) && forms.insert(form.as_str()),
                "invalid or duplicate Japanese demonstrative"
            );
        }
        let preferred: BTreeSet<_> = policy
            .term
            .iter()
            .map(|term| term.preferred.as_str())
            .collect();
        ensure!(
            preferred.len() == policy.term.len(),
            "duplicate preferred technical term"
        );
        let mut aliases = BTreeSet::new();
        for term in &policy.term {
            ensure!(
                valid_form(&term.preferred)
                    && term.source.starts_with("https://")
                    && !term.aliases.is_empty(),
                "technical term needs a preferred form, source, and aliases"
            );
            for alias in &term.aliases {
                ensure!(
                    valid_form(alias)
                        && !preferred.contains(alias.as_str())
                        && aliases.insert(alias.as_str()),
                    "technical aliases must be unique and cannot reject a preferred form"
                );
            }
        }
        Ok(policy)
    }

    pub(crate) fn embedded() -> Result<Self> {
        Self::parse(POLICY)
    }

    pub(crate) fn configured(bundle: &crate::Bundle) -> Result<Self> {
        let configured = std::fs::read_to_string(bundle.config.join("house.toml"))?;
        ensure!(
            configured.replace("\r\n", "\n") == POLICY.replace("\r\n", "\n"),
            "shared house policy differs from the checker build; install matching prose and dotfiles tools"
        );
        Self::embedded()
    }

    fn candidate(&self, segment: &Segment) -> bool {
        !segment.quoted
            && japanese(&segment.text)
            && (self
                .japanese
                .demonstratives
                .iter()
                .any(|word| segment.text.contains(word))
                || self
                    .term
                    .iter()
                    .flat_map(|term| &term.aliases)
                    .any(|alias| segment.text.contains(alias)))
    }

    /// Checks validated word boundaries and grammatical roles without editing literal source material.
    ///
    /// # Errors
    ///
    /// Refuses invalid annotations and projected spans before producing a source diagnostic.
    pub fn findings(
        &self,
        language: Language,
        segment: &Segment,
        annotation: &Annotation,
        source: &str,
    ) -> Result<Vec<Diagnostic>> {
        annotation.validate(segment)?;
        let applies = language == Language::Japanese && japanese(&segment.text);
        let mut findings = Vec::new();
        let mut covered = 0;
        for token in &annotation.tokens {
            if token.start < covered {
                continue;
            }
            let demonstrative = self.japanese.demonstratives.contains(&token.text)
                && (matches!(token.pos.as_str(), "DET" | "PRON" | "ADV" | "INTJ")
                    || matches!(token.tag.as_str(), "連体詞" | "代名詞")
                    || matches!(token.text.as_str(), "そのまま" | "それぞれ"));
            let term = self
                .term
                .iter()
                .flat_map(|term| term.aliases.iter().map(move |alias| (alias, term)))
                .filter(|(alias, _)| {
                    let length = alias.chars().count();
                    segment
                        .text
                        .chars()
                        .skip(token.start)
                        .take(length)
                        .eq(alias.chars())
                        && annotation
                            .tokens
                            .iter()
                            .any(|end| end.end == token.start + length)
                })
                .max_by_key(|(alias, _)| alias.chars().count());
            let violation = rules::copyedit_rules::house_style_violation(
                applies,
                segment.quoted,
                demonstrative || term.is_some(),
            );
            if !violation {
                continue;
            }
            let (end, rule, message) = if let Some((alias, term)) = term {
                (
                    token.start + alias.chars().count(),
                    TERMINOLOGY,
                    format!(
                        "Use the shared technical term {:?} instead of {:?}; preserve a literal API, UI label, or source quotation explicitly.",
                        term.preferred, alias
                    ),
                )
            } else {
                (token.end, DEMONSTRATIVE, "Name the concrete object, operation, or condition instead of using a Japanese demonstrative.".to_owned())
            };
            covered = end;
            let span = segment.source_span(token.start, end)?;
            ensure!(
                source.get(span.clone()).is_some(),
                "house diagnostic span is outside its source"
            );
            findings.push(Diagnostic::source(
                &segment.path,
                source,
                span,
                rule,
                Severity::Error,
                "shared-house-policy+spacy",
                &message,
            ));
        }
        Ok(findings)
    }
}

fn valid_form(value: &str) -> bool {
    !value.trim().is_empty()
        && value.trim() == value
        && value.chars().count() <= MAX_FORM_CHARACTERS
        && !value.chars().any(char::is_control)
}

fn japanese(text: &str) -> bool {
    text.chars().any(|character| matches!(character, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'))
}

/// Checks the shared policy before a prose or answer gate can accept authored text.
///
/// # Errors
///
/// Refuses invalid policy, source projection, unavailable native analysis, and incomplete annotations.
pub fn check(
    path: &str,
    source: &str,
    bundle: impl FnOnce() -> Result<crate::Bundle>,
) -> Result<Vec<Diagnostic>> {
    let mut layout = layout_findings(path, source)?;
    let policy = Policy::embedded()?;
    let projected = crate::copyedit::project_visible(path, source)?;
    let applicable = projected.iter().any(|segment| {
        !segment.quoted
            && (segment
                .text
                .chars()
                .any(|character| matches!(character, '\u{3040}'..='\u{30ff}'))
                || policy.candidate(segment))
    });
    if !applicable {
        return Ok(layout);
    }
    let bundle = bundle()?;
    let policy = Policy::configured(&bundle)?;
    let (_, findings) = crate::copyedit::check_house_style(
        &bundle,
        Language::Japanese,
        &[crate::copyedit::Document {
            path: path.to_owned(),
            text: source.to_owned(),
        }],
    )?;
    layout.extend(findings);
    let mut findings = layout;
    let segments: Vec<_> = projected
        .into_iter()
        .filter(|segment| policy.candidate(segment))
        .collect();
    if segments.is_empty() {
        return Ok(findings);
    }
    let nlp = crate::copyedit::analyze_house(&bundle, Language::Japanese, &segments)?;
    for (segment, annotation) in segments.iter().zip(&nlp.annotations) {
        findings.extend(policy.findings(Language::Japanese, segment, annotation, source)?);
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_parsing_rejects_authored_lists_and_tables_and_preserves_literal_forms() -> Result<()>
    {
        for (source, expected) in [
            ("A paragraph.\n\n- An item.\n", LIST),
            ("A paragraph.\n\n1. An item.\n", LIST),
            ("| Name | Value |\n| --- | --- |\n| A | B |\n", TABLE),
            ("<UL><LI>An item.</LI></UL>", LIST),
            ("<dl><dt>Name</dt><dd>Value</dd></dl>", LIST),
            ("<table><tr><td>A</td></tr></table>", TABLE),
        ] {
            let findings = layout_findings("fixture.md", source)?;
            assert!(
                findings.iter().any(|finding| finding.rule == expected),
                "{source}: {findings:?}"
            );
        }
        for source in [
            "A paragraph with `- a marker` and `| a separator |`.\n",
            "```md\n- A literal item.\n\n| A | B |\n| - | - |\n```\n",
            "> - A quoted item.\n>\n> | A | B |\n> | - | - |\n",
            "<blockquote><ul><li>A quoted item.</li></ul></blockquote>",
            "<pre><code><ul><li>A literal item.</li></ul></code></pre>",
            "<script>const literal = '<ul><li>Literal</li></ul>';</script>",
            "<!-- <ul><li>A comment.</li></ul> -->",
            "&lt;ul&gt;A literal tag.&lt;/ul&gt;",
            "---\ntitle: Literal metadata\nitems:\n  - A metadata item\n---\n\nA paragraph.\n",
        ] {
            assert!(
                layout_findings("fixture.md", source)?.is_empty(),
                "{source}"
            );
        }
        assert!(layout_findings("large.md", &"x".repeat(MAX_LAYOUT_BYTES + 1)).is_err());
        Ok(())
    }

    #[test]
    fn policy_refuses_unknown_versions_fields_and_conflicting_term_forms() {
        assert!(Policy::parse(POLICY).is_ok());
        for invalid in [
            POLICY.replace("schema_version = 1", "schema_version = 2"),
            format!("unknown = true\n{POLICY}"),
            POLICY.replace("aliases = [\"メソッド\"]", "aliases = [\"method\"]"),
            POLICY.replace("preferred = \"method\"", "preferred = \"interface\""),
            POLICY.replace("aliases = [\"メソッド\"]", "aliases = []"),
        ] {
            assert!(Policy::parse(&invalid).is_err());
        }
    }
}
