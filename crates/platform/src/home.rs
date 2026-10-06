//! The user's home, the known folders under it, and the `PATH` every managed tool starts with.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Where Windows lives when `SystemRoot` names no directory.
pub const DEFAULT_SYSTEM_ROOT: &str = "C:\\Windows";

/// Where Homebrew installs on Apple silicon.
const HOMEBREW_BIN: &str = "/opt/homebrew/bin";
const HOMEBREW_SBIN: &str = "/opt/homebrew/sbin";

/// The user's home: `USERPROFILE` on Windows, where native processes often lack `HOME`, and `HOME` elsewhere.
/// Windows falls back to `HOME`, which Git Bash sets.
#[must_use]
pub fn home() -> Option<PathBuf> {
    let variable = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    if cfg!(windows) {
        variable("USERPROFILE").or_else(|| variable("HOME"))
    } else {
        variable("HOME")
    }
    .map(PathBuf::from)
}

/// The Windows local app data folder, from `LOCALAPPDATA` or its default under `home`.
#[must_use]
pub fn local_app_data(home: &Path) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map_or_else(|| home.join("AppData/Local"), PathBuf::from)
}

/// The Scoop installation, from `SCOOP` or its default under `home`.
#[must_use]
pub fn scoop_root(home: &Path) -> PathBuf {
    std::env::var_os("SCOOP")
        .filter(|value| !value.is_empty())
        .map_or_else(|| home.join("scoop"), PathBuf::from)
}

/// The mise shims, which resolve each pinned tool for the directory a command runs in.
#[must_use]
pub fn mise_shims(home: &Path) -> PathBuf {
    home.join(if cfg!(windows) {
        "AppData/Local/mise/shims"
    } else {
        ".local/share/mise/shims"
    })
}

/// The directories under `home` that hold the tools this repository installs, in lookup order.
/// Each one derives from `home` alone, so a process started for another home finds only that home's tools.
#[must_use]
pub fn tool_directories(home: &Path) -> Vec<PathBuf> {
    let mut directories = vec![
        home.join(".local/bin"),
        home.join(".cargo/bin"),
        home.join(".nix-profile/bin"),
        mise_shims(home),
    ];
    if cfg!(target_os = "macos") {
        directories.extend([HOMEBREW_BIN.into(), HOMEBREW_SBIN.into()]);
    }
    if cfg!(windows) {
        directories.extend([
            home.join("scoop/shims"),
            home.join("AppData/Local/Programs/Herdr/bin"),
            home.join("AppData/Local/Microsoft/WinGet/Links"),
        ]);
    }
    directories
}

/// The tool directories followed by `inherited`, without the retired launcher directory.
///
/// # Errors
///
/// Returns an error when a directory holds the platform's `PATH` separator.
pub fn path_with(home: &Path, inherited: &OsStr) -> Result<OsString, std::env::JoinPathsError> {
    let mut directories = tool_directories(home);
    directories.extend(std::env::split_paths(inherited));
    directories.retain(|directory| !directory.ends_with(".local/share/dotfiles/bin"));
    std::env::join_paths(directories)
}

/// The `PATH` a managed process starts with: the tool directories, then this process's `PATH`.
///
/// # Errors
///
/// Returns an error when a directory holds the platform's `PATH` separator.
pub fn managed_path(home: &Path) -> Result<OsString, std::env::JoinPathsError> {
    path_with(home, &std::env::var_os("PATH").unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tool_directories_precede_the_inherited_path_without_the_retired_launchers() {
        let home = Path::new("/home/fixture");
        let inherited = std::env::join_paths([
            Path::new("/usr/bin"),
            &home.join(".local/share/dotfiles/bin"),
        ])
        .unwrap();
        let path: Vec<PathBuf> =
            std::env::split_paths(&path_with(home, &inherited).unwrap()).collect();
        assert_eq!(path[0], home.join(".local/bin"));
        assert_eq!(
            path.last().map(PathBuf::as_path),
            Some(Path::new("/usr/bin"))
        );
        assert!(!path.iter().any(|entry| entry.ends_with("dotfiles/bin")));
        assert!(path.contains(&mise_shims(home)));
    }
}
