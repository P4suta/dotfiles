use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const CODE: &str = "code";
const ROOT_DOMAIN: &[u8] = b"dotfiles-cargo-source-root-v1\0";

/// Keep retained Cargo artifacts separate for each physical Source root.
/// Validation skips backdated content replacement within one mutable root.
///
/// # Errors
///
/// Refuses an unresolved or nondirectory Source root and a relative cache base.
pub fn code(root: &Path, cache: &Path) -> Result<PathBuf> {
    ensure!(cache.is_absolute(), "Cargo cache base must be absolute");
    let root = crate::paths::canonical(root)?;
    ensure!(root.is_dir(), "Cargo Source root must be a directory");
    Ok(cache.join(CODE).join(source_key(&root)))
}

/// The partition key of a Source root: the SHA256 of its path bytes under a named domain, in hex.
/// Callers pass the canonical root, so two names of one directory share a key.
#[must_use]
pub fn source_key(root: &Path) -> String {
    let mut identity = Sha256::new();
    identity.update(ROOT_DOMAIN);
    identity.update(root.as_os_str().as_encoded_bytes());
    crate::build_inputs::hex(&identity.finalize())
}
