//! Screenshots that personal PR and issue bodies show from the repository's `screenshots` branch.

use crate::figure;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::path::Path;

/// The branch of a personal repository that holds each screenshot under its content hash.
const BRANCH: &str = "screenshots";
/// The largest screenshot this command stores, which keeps a body quick to load.
const MAX_BYTES: usize = 10 * 1024 * 1024;

/// The image formats a browser shows inline, each with its file signature.
/// <https://www.w3.org/TR/png-3/#5PNG-file-signature>, <https://www.w3.org/Graphics/JPEG/jfif3.pdf>, <https://developers.google.com/speed/webp/docs/riff_container>
const FORMATS: [(&str, &[u8]); 4] = [
    ("png", b"\x89PNG\r\n\x1a\n"),
    ("jpg", b"\xff\xd8\xff"),
    ("gif", b"GIF8"),
    ("webp", b"RIFF"),
];
/// The marker at bytes 8 to 12 of a WebP file, after the size of its container.
const WEBP_MARKER: &[u8] = b"WEBP";
/// The offset of the WebP marker.
const WEBP_MARKER_OFFSET: usize = 8;

/// The extension of an image whose bytes start with a known signature.
fn extension(bytes: &[u8]) -> Option<&'static str> {
    FORMATS
        .iter()
        .find(|(extension, signature)| {
            bytes.starts_with(signature)
                && (*extension != "webp"
                    || bytes.get(WEBP_MARKER_OFFSET..WEBP_MARKER_OFFSET + WEBP_MARKER.len())
                        == Some(WEBP_MARKER))
        })
        .map(|(extension, _)| *extension)
}

/// Stores a screenshot on the repository's `screenshots` branch and returns the body's image line.
///
/// # Errors
/// Returns an error for an unreadable, empty, or unknown image, an image over the size limit, and a GitHub failure.
pub(crate) fn publish(repository: &str, file: &Path, alt: &str) -> Result<String> {
    figure::check_alt(alt)?;
    let bytes = std::fs::read(file).with_context(|| format!("read {}", file.display()))?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_BYTES,
        "a screenshot needs between 1 byte and {MAX_BYTES} bytes, and {} has {}",
        file.display(),
        bytes.len()
    );
    let extension = extension(&bytes)
        .with_context(|| format!("{} isn't a PNG, JPEG, GIF, or WebP image", file.display()))?;
    let name = format!(
        "{}.{extension}",
        crate::build_inputs::hex(&Sha256::digest(&bytes))
    );
    figure::store(
        repository,
        BRANCH,
        &[figure::File {
            name: name.clone(),
            content: figure::Content::Binary(&bytes),
        }],
    )?;
    Ok(figure::image_line(repository, BRANCH, &name, alt))
}

#[cfg(test)]
mod tests {
    use super::extension;

    #[test]
    fn known_signatures_name_their_format() {
        assert_eq!(extension(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(extension(b"\xff\xd8\xff\xe0rest"), Some("jpg"));
        assert_eq!(extension(b"GIF89arest"), Some("gif"));
        assert_eq!(extension(b"RIFF\0\0\0\0WEBPrest"), Some("webp"));
    }

    #[test]
    fn other_bytes_name_no_format() {
        for bytes in [
            &b""[..],
            b"\x89PNG",
            b"RIFF\0\0\0\0WAVErest",
            b"RIFF",
            b"<svg></svg>",
        ] {
            assert_eq!(extension(bytes), None, "{bytes:?}");
        }
    }
}
