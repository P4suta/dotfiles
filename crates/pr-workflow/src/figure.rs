use crate::ValidatedDocument;
use anyhow::{Context, Result, ensure};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

pub(crate) const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const CACHE: &str = ".local/state/pr-workflow/figures";
pub(crate) const ATTACHMENT_URL: &str = "https://github.com/user-attachments/assets/";
const MINIMUM_CODE_FENCE: usize = 3;

pub(crate) fn private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))?;
    }
    Ok(())
}

pub(crate) fn check_alt(alt: &str) -> Result<()> {
    ensure!(
        !alt.trim().is_empty() && !alt.contains(['\n', '[', ']']),
        "an image needs a nonempty description without newlines or brackets"
    );
    Ok(())
}

pub(crate) fn cache(bytes: &[u8], extension: &str) -> Result<PathBuf> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_IMAGE_BYTES,
        "an image needs between 1 and {MAX_IMAGE_BYTES} bytes"
    );
    let directory = platform::home::home()
        .context("figure cache needs the user home directory")?
        .join(CACHE);
    private_directory(&directory)?;
    let name = crate::build_inputs::hex(&Sha256::digest(bytes));
    let path = directory.join(format!("{name}.{extension}"));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(read_image(&path)? == bytes, "cached image content changed");
        }
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}

pub(crate) fn read_image(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(u64::try_from(MAX_IMAGE_BYTES)?.saturating_add(1))
        .read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_IMAGE_BYTES,
        "an image needs between 1 and {MAX_IMAGE_BYTES} bytes"
    );
    Ok(bytes)
}

pub(crate) fn image_line(path: &Path, alt: &str) -> Result<String> {
    image_with_title(path, alt, "")
}

fn image_with_title(path: &Path, alt: &str, title: &str) -> Result<String> {
    check_alt(alt)?;
    let path = path
        .to_str()
        .context("image path must be UTF-8")?
        .replace('\\', "/");
    ensure!(!path.contains(['\n', '<', '>']), "unsupported image path");
    let title = if title.is_empty() {
        String::new()
    } else {
        format!(" \"{}\"", title.replace('\\', "\\\\").replace('"', "\\\""))
    };
    Ok(format!("![{}](<{path}>{title})", alt.trim()))
}

pub(crate) fn source_details(source: &str) -> String {
    details(source, "D2 source", "d2")
}

pub(crate) fn text_details(source: &str) -> String {
    details(source, "File source", "text")
}

fn details(source: &str, summary: &str, language: &str) -> String {
    let width = source
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(width.saturating_add(1).max(MINIMUM_CODE_FENCE));
    format!(
        "<details><summary>{summary}</summary>\n\n{fence}{language}\n{}\n{fence}\n\n</details>",
        source.trim_end()
    )
}

fn local_image(address: &str) -> bool {
    (!address.is_empty() && !address.starts_with(['#', '/']) && !address.contains(':'))
        || (Path::new(address).is_absolute() && !address.starts_with("//"))
}

fn staged_image(directory: &Path, address: &str) -> Result<PathBuf> {
    let path = directory.join(address);
    let bytes =
        read_image(&path).with_context(|| format!("read local image {}", path.display()))?;
    let extension = crate::screenshot::extension(&bytes)
        .or_else(|| {
            std::str::from_utf8(&bytes)
                .ok()
                .filter(|svg| svg.contains("<svg") && svg.contains("</svg>"))
                .map(|_| "svg")
        })
        .context("an attachment must be an SVG, PNG, JPEG, GIF, or WebP image")?;
    cache(&bytes, extension)
}

pub(crate) fn prepare_document(document: &mut ValidatedDocument, body_file: &Path) -> Result<()> {
    let directory = body_file
        .parent()
        .context("body file has no parent directory")?;
    let mut replacements = Vec::new();
    let mut images = BTreeSet::new();
    let mut active: Option<(usize, String, String, String)> = None;
    for (event, range) in Parser::new(&document.body).into_offset_iter() {
        match event {
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) if local_image(&dest_url) => {
                active = Some((
                    range.start,
                    dest_url.into_string(),
                    String::new(),
                    title.into_string(),
                ));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, alt, _)) = &mut active {
                    alt.push_str(&text);
                }
            }
            Event::End(TagEnd::Image) => {
                if let Some((start, address, alt, title)) = active.take() {
                    check_alt(&alt)?;
                    let image = staged_image(directory, &address)?;
                    replacements.push((start..range.end, image_with_title(&image, &alt, &title)?));
                    images.insert(image);
                }
            }
            _ => {}
        }
    }
    for (range, replacement) in replacements.into_iter().rev() {
        document.body.replace_range(range, &replacement);
    }
    document.attachments = images.into_iter().collect();
    Ok(())
}

fn publication_shape(body: &str) -> String {
    let mut replacements = Vec::new();
    let mut active: Option<(usize, String, String)> = None;
    for (event, range) in Parser::new(body).into_offset_iter() {
        match event {
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) if local_image(&dest_url) || dest_url.starts_with(ATTACHMENT_URL) => {
                active = Some((range.start, String::new(), title.into_string()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, alt, _)) = &mut active {
                    alt.push_str(&text);
                }
            }
            Event::End(TagEnd::Image) => {
                if let Some((start, alt, title)) = active.take() {
                    replacements.push((
                        start..range.end,
                        format!("![{alt}](attachment \"{title}\")"),
                    ));
                }
            }
            _ => {}
        }
    }
    let mut shape = body.to_owned();
    for (range, replacement) in replacements.into_iter().rev() {
        shape.replace_range(range, &replacement);
    }
    shape.trim_end().to_owned()
}

pub(crate) fn verify_publication(
    document: &ValidatedDocument,
    kind: &str,
    repository: &str,
    number: u64,
) -> Result<()> {
    if document.attachments.is_empty() {
        return Ok(());
    }
    let response = crate::gh(&[
        kind.into(),
        "view".into(),
        number.to_string().into(),
        "--repo".into(),
        repository.into(),
        "--json".into(),
        "body".into(),
    ])?;
    let response: serde_json::Value = serde_json::from_str(&response)?;
    let body = response["body"]
        .as_str()
        .context("published document has no body")?;
    verify_body(&document.body, body)
        .with_context(|| format!("inspect {repository} #{number} before retrying"))
}

pub(crate) fn verify_body(expected: &str, body: &str) -> Result<()> {
    ensure!(
        publication_shape(body) == publication_shape(expected),
        "published attachment or document content differs"
    );
    for event in Parser::new(body) {
        if let Event::Start(Tag::Image { dest_url, .. }) = event {
            ensure!(
                !local_image(&dest_url),
                "published document still references a local image"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_and_hidden_images_are_not_local_attachments() {
        for address in [
            "https://example.com/image.svg",
            "//example.com/a.png",
            "data:image/png;base64,AAA",
            "#image",
            "",
        ] {
            assert!(!local_image(address), "{address}");
        }
        assert!(local_image("after.png"));
        assert!(local_image("./figures/after.png"));
    }

    #[test]
    fn source_fences_preserve_embedded_markdown() {
        let source = "label: |md\n```rust\nvalue\n```\n|";
        let details = source_details(source);
        assert!(details.contains("````d2\n"));
        assert!(details.contains(source));
        assert!(details.ends_with("````\n\n</details>"));
    }

    #[test]
    fn publication_comparison_preserves_text_and_alternative_text() {
        let local = "Before.\n\n![Failure](</cache/image.svg>)\n\nAfter.\n";
        let uploaded =
            "Before.\n\n![Failure](https://github.com/user-attachments/assets/fixture)\n\nAfter.\n";
        assert_eq!(publication_shape(local), publication_shape(uploaded));
        assert_ne!(
            publication_shape(local),
            publication_shape(&uploaded.replace("Failure", "Success"))
        );
        assert_ne!(
            publication_shape(local),
            publication_shape(&uploaded.replace("Before.", "Changed."))
        );
    }

    #[test]
    fn publication_comparison_preserves_image_titles() {
        let local = "![Failure](</cache/image.svg> \"Retry state\")";
        let uploaded =
            "![Failure](https://github.com/user-attachments/assets/fixture \"Retry state\")";
        assert_eq!(publication_shape(local), publication_shape(uploaded));
        assert_ne!(
            publication_shape(local),
            publication_shape(&uploaded.replace("Retry state", "Success state"))
        );
    }

    #[test]
    fn image_reads_reject_empty_and_oversize_files() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("image.png");
        let file = fs::File::create(&path)?;
        assert!(read_image(&path).is_err());
        file.set_len(u64::try_from(MAX_IMAGE_BYTES)?.saturating_add(1))?;
        assert!(read_image(&path).is_err());
        Ok(())
    }
}
