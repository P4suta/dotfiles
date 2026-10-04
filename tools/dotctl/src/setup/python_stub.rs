//! Removes the Windows Store python App Execution Aliases.
//!
//! For an uninstalled Store Python, `WindowsApps` holds 0-byte reparse point stubs (`python.exe` / `python3.exe`) that shadow a plain `python` earlier on PATH and fail with exit code 9009.
//! Removing them lets the mise-managed interpreter answer to `python`.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// `FILE_ATTRIBUTE_REPARSE_POINT`.
/// Named here rather than pulled from a Windows crate: one constant is not worth a dependency.
const REPARSE_POINT: u32 = 0x400;

const STUBS: &[&str] = &["python.exe", "python3.exe"];

/// The safety valve: a real interpreter must never be deleted, so both halves of the Store stub signature have to hold.
fn looks_like_stub(len: u64, attributes: u32) -> bool {
    len == 0 && attributes & REPARSE_POINT != 0
}

pub fn run() -> Result<i32> {
    let Some(dir) = windows_apps_dir() else {
        println!(">>> no %LOCALAPPDATA%, nothing to do");
        return Ok(0);
    };

    for name in STUBS {
        let path = dir.join(name);
        // symlink_metadata, not metadata: following the reparse point of an uninstalled app fails, and the attributes are the thing we need.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if looks_like_stub(meta.len(), attributes_of(&meta)) {
            println!(">>> removing Windows Store python stub: {}", path.display());
            if let Err(err) = std::fs::remove_file(&path) {
                eprintln!("    could not remove it: {err}");
            }
        } else {
            eprintln!(
                "skip (real file, not a Store stub): {} (len={})",
                path.display(),
                meta.len()
            );
        }
    }
    Ok(0)
}

#[cfg(windows)]
fn attributes_of(meta: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(not(windows))]
fn attributes_of(_meta: &std::fs::Metadata) -> u32 {
    0
}

fn windows_apps_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|local| local.join("Microsoft").join("WindowsApps"))
        .filter(|dir: &PathBuf| Path::new(dir).is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_empty_reparse_point_is_a_stub() {
        assert!(looks_like_stub(0, REPARSE_POINT));
        assert!(looks_like_stub(0, REPARSE_POINT | 0x20));
        // A real interpreter: bytes on disk, no reparse point.
        assert!(!looks_like_stub(98_304, 0x20));
        // Half a signature is not a signature.
        assert!(!looks_like_stub(98_304, REPARSE_POINT));
        assert!(!looks_like_stub(0, 0x20));
    }
}
