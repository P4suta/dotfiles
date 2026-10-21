//! Locates tools and builds the environment for child processes.
//!
//! Hook processes start from a non-interactive `sh` that may never have run `mise activate`, so this module restores the managed tool directories to `PATH`.
//!
//! Child processes prevent mise from installing missing tools or pruning installations.

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

/// A `Command` with the hook `PATH` and automatic mise installation and pruning off.
pub fn child(program: &str) -> Command {
    with_hook_environment(platform::process::command(program))
}

/// Doppler with the configured user scope and the hook environment.
pub fn doppler() -> Option<Command> {
    platform::process::doppler_for_user().map(with_hook_environment)
}

fn with_hook_environment(mut c: Command) -> Command {
    c.env("PATH", hook_path())
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_NOT_FOUND_AUTO_INSTALL", "false")
        .env("MISE_AUTO_PRUNE", "false");
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
