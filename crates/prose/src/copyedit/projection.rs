use anyhow::{Result, ensure};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// A pipeline's admitted text size, measured in Unicode scalar values.
pub const MAX_CHARACTERS: usize = 100_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Paragraph,
    Heading,
    Caption,
    TableCell,
    Metadata,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub id: String,
    pub path: String,
    pub text: String,
    pub kind: Kind,
    pub quoted: bool,
    locations: Vec<Range<usize>>,
}

impl Segment {
    /// Maps character offsets to their enclosing source bytes, including decoded Markdown entities.
    ///
    /// # Errors
    ///
    /// Refuses empty, inverted, or out-of-bounds offsets.
    pub fn source_span(&self, start: usize, end: usize) -> Result<Range<usize>> {
        ensure!(
            start < end && end <= self.locations.len(),
            "invalid projected source span"
        );
        let first = &self.locations[start];
        let last = &self.locations[end - 1];
        Ok(first.start..last.end)
    }
}

#[derive(Debug)]
struct Projection<'a> {
    path: &'a str,
    source: &'a str,
    text: String,
    locations: Vec<Range<usize>>,
    segments: Vec<Segment>,
    kind: Kind,
    quotes: usize,
}

impl Projection<'_> {
    fn flush(&mut self) -> Result<()> {
        let visible = self.text.trim_matches([' ', '\t', '\n', '\r']);
        let leading = self.text.chars().count()
            - self
                .text
                .trim_start_matches([' ', '\t', '\n', '\r'])
                .chars()
                .count();
        let length = visible.chars().count();
        if length > 0 && self.text.chars().any(char::is_alphanumeric) {
            ensure!(
                length <= MAX_CHARACTERS,
                "{} has a prose block beyond {MAX_CHARACTERS} characters",
                self.path
            );
            self.segments.push(Segment {
                id: format!("{}:{}", self.path, self.segments.len()),
                path: self.path.to_owned(),
                text: visible.to_owned(),
                kind: self.kind,
                quoted: self.quotes > 0,
                locations: self.locations[leading..leading + length].to_vec(),
            });
        }
        self.text.clear();
        self.locations.clear();
        Ok(())
    }

    fn append(&mut self, text: &str, range: Range<usize>) {
        let raw = &self.source[range.clone()];
        for (offset, character) in text.char_indices() {
            self.text.push(character);
            self.locations.push(if raw == text {
                range.start + offset..range.start + offset + character.len_utf8()
            } else {
                range.clone()
            });
        }
    }

    fn metadata(&mut self, range: Range<usize>) -> Result<()> {
        ensure!(
            self.source[range.clone()].chars().count() <= MAX_CHARACTERS,
            "metadata exceeds the pipeline's input limit"
        );
        for field in super::metadata::fields(&self.source[range.clone()])? {
            ensure!(
                field.text.chars().count() <= MAX_CHARACTERS,
                "metadata exceeds the pipeline's text limit"
            );
            let locations = field
                .text
                .char_indices()
                .map(|(offset, character)| {
                    if field.precise {
                        range.start + field.range.start + offset
                            ..range.start + field.range.start + offset + character.len_utf8()
                    } else {
                        range.start + field.range.start..range.start + field.range.end
                    }
                })
                .collect();
            self.segments.push(Segment {
                id: format!("{}:{}", self.path, self.segments.len()),
                path: self.path.to_owned(),
                text: field.text,
                kind: Kind::Metadata,
                quoted: false,
                locations,
            });
        }
        Ok(())
    }
}

/// Extracts visible prose, captions, headings, and table cells without code or link destinations.
///
/// # Errors
///
/// Refuses documents without applicable prose or a block larger than the pinned pipeline admits.
pub fn project(path: &str, source: &str) -> Result<Vec<Segment>> {
    let segments = project_inner(path, source, true)?;
    ensure!(
        !segments.is_empty(),
        "{path} has no applicable visible prose"
    );
    Ok(segments)
}

pub(crate) fn project_body(path: &str, source: &str) -> Result<Vec<Segment>> {
    project_inner(path, source, false)
}

pub(crate) fn project_visible(path: &str, source: &str) -> Result<Vec<Segment>> {
    project_inner(path, source, true)
}

fn project_inner(path: &str, source: &str, include_metadata: bool) -> Result<Vec<Segment>> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_MATH;
    let mut projection = Projection {
        path,
        source,
        text: String::new(),
        locations: Vec::new(),
        segments: Vec::new(),
        kind: Kind::Paragraph,
        quotes: 0,
    };
    let mut hidden: usize = 0;
    let mut metadata = false;
    let mut autolink = false;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(Tag::MetadataBlock(_)) => {
                projection.flush()?;
                metadata = true;
            }
            Event::End(TagEnd::MetadataBlock(_)) => metadata = false,
            Event::Text(_) if metadata && include_metadata => projection.metadata(range)?,
            _ if metadata => {}
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock) => {
                projection.flush()?;
                hidden += 1;
            }
            Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock) => {
                hidden = hidden.saturating_sub(1);
            }
            _ if hidden > 0 => {}
            Event::Start(Tag::Link {
                link_type: LinkType::Autolink | LinkType::Email,
                ..
            }) => {
                projection.flush()?;
                autolink = true;
            }
            Event::End(TagEnd::Link) if autolink => autolink = false,
            _ if autolink => {}
            Event::Start(Tag::BlockQuote(_)) => {
                projection.flush()?;
                projection.quotes += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                projection.flush()?;
                projection.quotes = projection.quotes.saturating_sub(1);
            }
            Event::Start(Tag::Heading { .. }) => {
                projection.flush()?;
                projection.kind = Kind::Heading;
            }
            Event::Start(Tag::Image { .. }) => {
                projection.flush()?;
                projection.kind = Kind::Caption;
            }
            Event::Start(Tag::TableCell) => {
                projection.flush()?;
                projection.kind = Kind::TableCell;
            }
            Event::Start(Tag::Paragraph | Tag::Item)
            | Event::End(
                TagEnd::Paragraph
                | TagEnd::Item
                | TagEnd::Heading(_)
                | TagEnd::Image
                | TagEnd::TableCell,
            ) => {
                projection.flush()?;
                projection.kind = Kind::Paragraph;
            }
            Event::Text(text) => projection.append(&text, range),
            Event::SoftBreak | Event::HardBreak => projection.append("\n", range),
            Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::InlineHtml(_)
            | Event::Html(_) => projection.flush()?,
            _ => {}
        }
    }
    projection.flush()?;
    Ok(projection.segments)
}
