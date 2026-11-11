use anyhow::{Context, Result, bail, ensure};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, LinkType, Parser, Tag, TagEnd};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

const ASSET_PROBE_URL: &str = "https://example.invalid/attachment";
const MAX_ATTACHMENTS: usize = 50;

enum Media {
    Image(String),
    Video(String),
}

struct Attachment {
    path: PathBuf,
    media: Media,
}

fn absolute(path: &Path, directory: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute() || !matches!(path.components().next(), Some(Component::Prefix(_))),
        "GitHub attachment has an unresolved drive-relative path"
    );
    let joined = if path.is_absolute() {
        path.to_owned()
    } else {
        directory.join(path)
    };
    let mut result = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}

fn media(path: &Path, alt: &str) -> Result<Media> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("GitHub attachment has no filename caption")?;
    let (stem, extension) = filename.rsplit_once('.').unwrap_or((filename, ""));
    match extension.to_ascii_lowercase().as_str() {
        "mp4" | "mov" | "webm" => {
            ensure!(
                alt.is_empty(),
                "GitHub video attachment cannot have alt text"
            );
            Ok(Media::Video(filename.to_owned()))
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => Ok(Media::Image(if alt.is_empty() {
            stem.replace('.', " ")
        } else {
            alt.to_owned()
        })),
        _ => bail!("GitHub attachment has an unsupported media type"),
    }
}

fn resolve(argument: &str, directory: &Path) -> Result<Attachment> {
    let splits = argument
        .match_indices('#')
        .rev()
        .filter(|(index, _)| *index > 0)
        .map(|(index, _)| (&argument[..index], &argument[index + 1..]));
    for (file, alt) in std::iter::once((argument, "")).chain(splits) {
        let path = absolute(&super::super::native_path(file), directory)?;
        if let Ok(metadata) = path.metadata() {
            ensure!(
                metadata.is_file(),
                "GitHub attachment is not a regular file"
            );
            return Ok(Attachment {
                media: media(&path, alt)?,
                path,
            });
        }
    }
    bail!("cannot inspect GitHub attachment file {argument}")
}

fn spellings(destination: &str) -> Vec<String> {
    let mut unescaped = String::new();
    let mut characters = destination.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\' && characters.peek().is_some_and(char::is_ascii_punctuation) {
            unescaped.push(characters.next().unwrap());
        } else {
            unescaped.push(character);
        }
    }
    let mut values = vec![destination.to_owned(), unescaped];
    for value in values.clone() {
        if let Ok(decoded) = percent_decode_str(&value).decode_utf8() {
            values.push(decoded.into_owned());
        }
    }
    values
}

fn referenced_attachment(
    destination: &str,
    paths: &BTreeMap<PathBuf, usize>,
    directory: &Path,
    markdown_directory: &Path,
) -> Option<usize> {
    if destination.starts_with('#')
        || destination.starts_with("//")
        || url::Url::parse(destination).is_ok_and(|url| url.scheme().len() > 1 || url.has_host())
    {
        return None;
    }
    let candidates = spellings(destination);
    for base in [markdown_directory, directory] {
        for candidate in &candidates {
            if let Ok(path) = absolute(&super::super::native_path(candidate), base)
                && let Some(index) = paths.get(&path)
            {
                return Some(*index);
            }
        }
    }
    None
}

struct Edit {
    range: Range<usize>,
    text: String,
}

struct References {
    used: BTreeSet<usize>,
    edits: Vec<Edit>,
}

fn escape_alt(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(['\r', '\n'], " ")
}

fn standalone(markdown: &str, paragraph: Option<&Range<usize>>, node: &Range<usize>) -> bool {
    paragraph.is_some_and(|paragraph| {
        paragraph.start <= node.start
            && node.end <= paragraph.end
            && markdown[paragraph.start..node.start]
                .bytes()
                .chain(markdown[node.end..paragraph.end].bytes())
                .all(|byte| b" \t\r\n".contains(&byte))
    })
}

fn video_label(markdown: &str, node: &Range<usize>, filename: &str) -> Option<Edit> {
    // Inline images and links expose the same label to the prose checker.
    markdown[node.clone()].starts_with("![](").then(|| Edit {
        range: node.start + 2..node.start + 2,
        text: escape_alt(filename),
    })
}

fn referenced(
    markdown: &str,
    paths: &BTreeMap<PathBuf, usize>,
    attachments: &[Attachment],
    directory: &Path,
    markdown_directory: &Path,
) -> Result<References> {
    let parser = Parser::new(markdown);
    let definitions = Parser::new(markdown);
    let mut result = References {
        used: BTreeSet::new(),
        edits: Vec::new(),
    };
    let mut paragraph = None;
    let mut dropped_through = 0;
    for (event, range) in parser.into_offset_iter() {
        match &event {
            Event::Start(Tag::Paragraph) => paragraph = Some(range.clone()),
            Event::End(TagEnd::Paragraph) => paragraph = None,
            _ => {}
        }
        let (image, destination, link_type) = match event {
            Event::Start(Tag::Image {
                dest_url,
                link_type,
                ..
            }) => (true, dest_url, link_type),
            Event::Start(Tag::Link {
                dest_url,
                link_type,
                ..
            }) => (false, dest_url, link_type),
            _ => continue,
        };
        if let Some(index) =
            referenced_attachment(&destination, paths, directory, markdown_directory)
        {
            if image && matches!(&attachments[index].media, Media::Video(_)) {
                ensure!(
                    link_type == LinkType::Inline,
                    "GitHub cannot embed a video as a reference-style image"
                );
            }
            if range.start < dropped_through && link_type == LinkType::Inline {
                continue;
            }
            if image && let Media::Video(filename) = &attachments[index].media {
                if standalone(markdown, paragraph.as_ref(), &range) {
                    dropped_through = range.end;
                    result.edits.push(Edit {
                        range,
                        text: ASSET_PROBE_URL.to_owned(),
                    });
                } else {
                    result.edits.extend(video_label(markdown, &range, filename));
                }
            }
            let rewritable = match link_type {
                LinkType::Inline => true,
                LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut => definitions
                    .reference_definitions()
                    .iter()
                    .any(|(_, definition)| {
                        definition.dest == destination
                            && !markdown[definition.span.clone()]
                                .lines()
                                .skip(1)
                                .any(|line| line.trim_start().starts_with('>'))
                    }),
                _ => false,
            };
            if rewritable {
                result.used.insert(index);
            }
        }
    }
    Ok(result)
}

pub(super) fn project(
    arguments: &[String],
    markdown: &str,
    body_file: Option<&str>,
    directory: &Path,
) -> Result<String> {
    ensure!(
        arguments.len() <= MAX_ATTACHMENTS,
        "too many GitHub attachments"
    );
    let attachments = arguments
        .iter()
        .map(|argument| resolve(argument, directory))
        .collect::<Result<Vec<_>>>()?;
    let mut paths = BTreeMap::new();
    for (index, attachment) in attachments.iter().enumerate() {
        ensure!(
            paths.insert(attachment.path.clone(), index).is_none(),
            "GitHub attachments repeat a file path"
        );
    }
    let body_path = body_file
        .map(|file| absolute(&super::super::native_path(file), directory))
        .transpose()?;
    let markdown_directory = body_path
        .as_deref()
        .and_then(Path::parent)
        .unwrap_or(directory);
    let References { used, mut edits } = referenced(
        markdown,
        &paths,
        &attachments,
        directory,
        markdown_directory,
    )?;
    edits.sort_by_key(|edit| edit.range.start);
    let mut projected = markdown.to_owned();
    for edit in edits.into_iter().rev() {
        projected.replace_range(edit.range, &edit.text);
    }
    let mut result = Vec::new();
    for (index, attachment) in attachments.into_iter().enumerate() {
        if !used.contains(&index) {
            match attachment.media {
                Media::Image(alt) => {
                    ensure!(
                        !alt.trim().is_empty(),
                        "GitHub attachment has no filename caption"
                    );
                    result.push(format!("![{}]({ASSET_PROBE_URL})", escape_alt(&alt)));
                }
                Media::Video(_) => result.push(ASSET_PROBE_URL.to_owned()),
            }
        }
    }
    if result.is_empty() {
        return Ok(projected);
    }
    let prefix = projected.trim_end_matches([' ', '\t', '\r', '\n']);
    let suffix = result.join("\n\n");
    Ok(if prefix.is_empty() {
        suffix
    } else {
        format!("{prefix}\n\n{suffix}")
    })
}
