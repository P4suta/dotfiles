//! Local screenshots prepared for GitHub document attachments.

use crate::figure;
use anyhow::{Context, Result};
use std::path::Path;

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
pub(crate) fn extension(bytes: &[u8]) -> Option<&'static str> {
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

/// Caches a screenshot locally and returns a Markdown attachment reference.
///
/// # Errors
/// Returns an error for an unreadable, empty, or unknown image, an image over the size limit, and a GitHub failure.
pub(crate) fn prepare(file: &Path, alt: &str) -> Result<String> {
    figure::check_alt(alt)?;
    let bytes = figure::read_image(file).with_context(|| format!("read {}", file.display()))?;
    let extension = extension(&bytes)
        .with_context(|| format!("{} isn't a PNG, JPEG, GIF, or WebP image", file.display()))?;
    figure::image_line(&figure::cache(&bytes, extension)?, alt)
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
