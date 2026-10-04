//! Locating tools and building the environment children run in.
//!
//! Hook processes start from a non-interactive `sh` reached through `core.hooksPath` / lefthook, which may never have run `mise activate`, so the shims, `~/.local/bin` and Homebrew are put back the way `scripts/_common.sh` does for the shell gates — one definition, shared by every module here that spawns a linter or a wrapper.
//!
//! `MISE_AUTO_INSTALL=false` rides along: mise otherwise tries to install every missing tool each time a shim runs, which turns a lint into unexpected downloads.

use std::path::PathBuf;
use std::process::Command;

/// The PATH children of this gate should see.
pub fn hook_path() -> String {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = Vec::new();
    let shims = home.join(".local/share/mise/shims");
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

/// A `Command` pre-configured with the hook PATH and no mise auto-install.
pub fn child(program: &str) -> Command {
    let mut c = Command::new(program);
    c.env("PATH", hook_path()).env("MISE_AUTO_INSTALL", "false");
    c
}

/// The first `program` visible on that PATH.
pub fn which(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&hook_path())
        .map(|d| d.join(program))
        .find(|p| p.is_file())
}
