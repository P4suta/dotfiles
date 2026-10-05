//! Locates tools and builds the environment for child processes.
//!
//! Hook processes start from a non-interactive `sh` that may never have run `mise activate`, so this module restores the mise shims, `~/.local/bin`, and Homebrew to `PATH` the way `scripts/_common.sh` does for the shell gates.
//!
//! `MISE_AUTO_INSTALL=false` stops each shim run from downloading missing tools.

use std::path::PathBuf;
use std::process::Command;

/// Reads `HOME`, or `USERPROFILE` on Windows, where native processes often lack `HOME`.
pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| {
            if cfg!(windows) {
                std::env::var_os("USERPROFILE")
            } else {
                None
            }
        })
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// The PATH children of this gate should see.
pub fn hook_path() -> String {
    let home = home();
    let mut dirs: Vec<PathBuf> = Vec::new();
    let shims = home.join(if cfg!(windows) {
        "AppData/Local/mise/shims"
    } else {
        ".local/share/mise/shims"
    });
    if shims.is_dir() {
        dirs.push(shims);
    }
    dirs.push(home.join(".local/bin"));
    if let Ok(p) = std::env::var("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    for extra in ["/opt/homebrew/bin", "/opt/homebrew/sbin"] {
        let pb = PathBuf::from(extra);
        if pb.is_dir() && !dirs.contains(&pb) {
            dirs.push(pb);
        }
    }
    std::env::join_paths(dirs).map_or_else(|_| String::new(), |s| s.to_string_lossy().into_owned())
}

/// A `Command` with the hook `PATH` and `MISE_AUTO_INSTALL=false`.
pub fn child(program: &str) -> Command {
    let mut c = Command::new(program);
    c.env("PATH", hook_path()).env("MISE_AUTO_INSTALL", "false");
    c
}

/// The first `program` visible on that PATH.
pub fn which(program: &str) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{program}.exe"), program.to_owned()]
    } else {
        vec![program.to_owned()]
    };
    std::env::split_paths(&hook_path())
        .flat_map(|d| names.iter().map(move |name| d.join(name)))
        .find(|p| p.is_file())
}
