//! Path spellings the tools agree on across hosts.

use std::io::Result;
use std::path::{Path, PathBuf};

/// The chezmoi source state of a checkout, which `.chezmoiroot` names.
pub const SOURCE_STATE: &str = "home";

#[must_use]
pub fn source_state(root: &Path) -> PathBuf {
    root.join(SOURCE_STATE)
}

/// Whether `path` ends in `.<extension>`, compared exactly, because chezmoi and the gates treat names case-sensitively on every host.
#[must_use]
pub fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|found| found == extension)
}

/// Windows `canonicalize` returns verbatim drive paths that cargo rejects.
#[must_use]
pub fn without_verbatim_prefix(path: &Path) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest)
            if rest.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                && rest.as_bytes().get(1) == Some(&b':') =>
        {
            PathBuf::from(rest)
        }
        _ => path.to_path_buf(),
    }
}

/// `canonicalize` without the Windows verbatim drive prefix.
///
/// # Errors
///
/// Returns an error when `path` doesn't exist or resolving it fails.
#[expect(
    clippy::disallowed_methods,
    reason = "the one place that canonicalizes a path, so every caller gets the spelling without the verbatim prefix"
)]
pub fn canonical(path: &Path) -> Result<PathBuf> {
    Ok(without_verbatim_prefix(&path.canonicalize()?))
}
