use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, ops::Range};
use yaml_rust2::{
    Yaml,
    parser::{Event, Parser},
    scanner::{Marker, TScalarStyle},
};

const HUMAN_FIELDS: [&str; 8] = [
    "title",
    "description",
    "excerpt",
    "custom_excerpt",
    "meta_title",
    "meta_description",
    "feature_image_alt",
    "feature_image_caption",
];

pub struct Field {
    pub text: String,
    pub range: Range<usize>,
    pub precise: bool,
}

fn offset(source: &str, marker: Marker) -> Result<usize> {
    let mut lines = source.split_inclusive('\n');
    let mut offset = 0;
    for _ in 1..marker.line() {
        offset += lines.next().context("YAML line exceeds its source")?.len();
    }
    let line = lines.next().unwrap_or_default();
    let column = line
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(line.len()))
        .nth(marker.col())
        .context("YAML column exceeds its source")?;
    Ok(offset + column)
}

fn field(source: &str, text: &str, style: TScalarStyle, mut range: Range<usize>) -> Field {
    if matches!(
        style,
        TScalarStyle::SingleQuoted | TScalarStyle::DoubleQuoted
    ) {
        range.start += 1;
    }
    let precise = source[range.clone()].starts_with(text);
    if precise {
        range.end = range.start + text.len();
    }
    Field {
        text: text.to_owned(),
        range,
        precise,
    }
}

pub fn fields(source: &str) -> Result<Vec<Field>> {
    let mut parser = Parser::new_from_str(source);
    let mut events = Vec::new();
    loop {
        let item = parser.next_token()?;
        let ended = item.0 == Event::StreamEnd;
        events.push(item);
        if ended {
            break;
        }
    }
    let mut depth = 0;
    let mut key = None;
    let mut selected = BTreeSet::new();
    let mut fields = Vec::new();
    let mut documents = 0;
    for (index, (event, marker)) in events.iter().enumerate() {
        match event {
            Event::DocumentStart => {
                documents += 1;
                ensure!(
                    documents == 1,
                    "article metadata must contain one YAML document"
                );
            }
            Event::MappingStart(..) | Event::SequenceStart(..) => {
                ensure!(
                    depth > 0 || matches!(event, Event::MappingStart(..)),
                    "article metadata must be a YAML mapping"
                );
                if depth == 1 {
                    let name = key.take().context("metadata value lost its key")?;
                    ensure!(
                        !HUMAN_FIELDS.contains(&name),
                        "human metadata field must be a string: {name}"
                    );
                }
                depth += 1;
            }
            Event::MappingEnd | Event::SequenceEnd => depth -= 1,
            Event::Scalar(text, style, anchor, tag) if depth == 1 => {
                if let Some(name) = key.take() {
                    if HUMAN_FIELDS.contains(&name) {
                        ensure!(
                            selected.insert(name),
                            "duplicate human metadata field: {name}"
                        );
                        ensure!(
                            *anchor == 0 && tag.is_none(),
                            "human metadata fields must use literal strings"
                        );
                        ensure!(
                            *style != TScalarStyle::Plain
                                || Yaml::from_str(text).as_str().is_some(),
                            "human metadata field must be a string: {name}"
                        );
                        if !text.is_empty() {
                            let end = events
                                .get(index + 1)
                                .context("YAML value has no boundary")?
                                .1;
                            fields.push(field(
                                source,
                                text,
                                *style,
                                offset(source, *marker)?..offset(source, end)?,
                            ));
                        }
                    }
                } else {
                    key = Some(text.as_str());
                }
            }
            Event::Alias(_) if depth == 1 => {
                let name = key.take().context("metadata alias lost its key")?;
                ensure!(
                    !HUMAN_FIELDS.contains(&name),
                    "human metadata fields must not use aliases: {name}"
                );
            }
            _ => {}
        }
    }
    Ok(fields)
}
