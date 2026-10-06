//! Replaces installed executables, including one that runs during the replacement.
//! Windows refuses to overwrite or delete a running image but lets a process rename it, so the current binary moves aside first and a later installation deletes it.

use crate::aside_rules::{aside_name, is_aside};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Deletes the aside copies earlier installations left beside `target`.
/// A copy that still runs refuses deletion and stays for the next installation.
pub fn sweep(target: &Path) {
    let (Some(parent), Some(name)) = (target.parent(), target.file_name()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if is_aside(
            name.as_encoded_bytes(),
            entry.file_name().as_encoded_bytes(),
        ) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Sweeps old aside copies, then renames `target` aside when it exists and returns where it went.
pub fn move_aside(target: &Path) -> Result<Option<PathBuf>> {
    sweep(target);
    if fs::symlink_metadata(target).is_err() {
        return Ok(None);
    }
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .context("installed binary name is not UTF-8")?;
    let aside = target.with_file_name(aside_name(name, std::process::id()));
    fs::rename(target, &aside).with_context(|| format!("move {} aside", target.display()))?;
    Ok(Some(aside))
}

/// Puts an aside copy back when no replacement arrived at `target`.
pub fn restore(target: &Path, aside: Option<PathBuf>) {
    if let Some(aside) = aside
        && fs::symlink_metadata(target).is_err()
    {
        let _ = fs::rename(aside, target);
    }
}

/// Moves `staged` to `target`, first moving a running `target` aside.
pub fn replace(staged: &Path, target: &Path) -> Result<()> {
    let aside = move_aside(target)?;
    if let Err(error) = fs::rename(staged, target) {
        restore(target, aside);
        return Err(error).with_context(|| format!("install {}", target.display()));
    }
    Ok(())
}
