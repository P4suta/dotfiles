use anyhow::{Context, Result, bail, ensure};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, LinkType, Parser, Tag};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const ALT_TEXT_PROBE_URL: &str = "https://example.invalid/attachment";
const MAX_ATTACHMENTS: usize = 50;

enum Media {
    Image(String),
    Video,
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
            Ok(Media::Video)
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

fn referenced(
    markdown: &str,
    paths: &BTreeMap<PathBuf, usize>,
    directory: &Path,
    markdown_directory: &Path,
) -> BTreeSet<usize> {
    let parser = Parser::new(markdown);
    let definitions = Parser::new(markdown);
    let mut result = BTreeSet::new();
    for event in parser {
        if let Event::Start(
            Tag::Image {
                dest_url,
                link_type,
                id,
                ..
            }
            | Tag::Link {
                dest_url,
                link_type,
                id,
                ..
            },
        ) = event
        {
            let rewritable = match link_type {
                LinkType::Inline => true,
                LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut => definitions
                    .reference_definitions()
                    .get(&id)
                    .is_some_and(|definition| {
                        !markdown[definition.span.clone()]
                            .lines()
                            .skip(1)
                            .any(|line| line.trim_start().starts_with('>'))
                    }),
                _ => false,
            };
            if rewritable
                && let Some(index) =
                    referenced_attachment(&dest_url, paths, directory, markdown_directory)
            {
                result.insert(index);
            }
        }
    }
    result
}

pub(super) fn appended_markdown(
    arguments: &[String],
    markdown: &str,
    body_file: Option<&str>,
    directory: &Path,
) -> Result<Vec<String>> {
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
    let used = referenced(markdown, &paths, directory, markdown_directory);
    let mut result = Vec::new();
    for (index, attachment) in attachments.into_iter().enumerate() {
        if !used.contains(&index)
            && let Media::Image(alt) = attachment.media
        {
            ensure!(
                !alt.trim().is_empty(),
                "GitHub attachment has no filename caption"
            );
            let escaped = alt
                .replace('\\', "\\\\")
                .replace('[', "\\[")
                .replace(']', "\\]")
                .replace(['\r', '\n'], " ");
            result.push(format!("![{escaped}]({ALT_TEXT_PROBE_URL})"));
        }
    }
    Ok(result)
}
