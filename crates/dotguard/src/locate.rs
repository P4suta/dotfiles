//! Locates tools and builds the environment for child processes.
//!
//! Hook processes start from a non-interactive `sh` that may never have run `mise activate`, so this module restores the managed tool directories to `PATH`.
//!
//! `MISE_AUTO_INSTALL=false` stops each shim run from downloading missing tools.

use std::path::PathBuf;
use std::process::Command;

/// The user's home, or an empty path when the environment names none.
pub fn home() -> PathBuf {
    platform::home::home().unwrap_or_default()
}

/// The PATH children of this gate should see.
pub fn hook_path() -> String {
    platform::home::managed_path(&home())
        .map_or_else(|_| String::new(), |s| s.to_string_lossy().into_owned())
}

/// A `Command` with the hook `PATH` and `MISE_AUTO_INSTALL=false`.
pub fn child(program: &str) -> Command {
    let mut c = platform::process::command(program);
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
