//! Locates the real git.
//!
//! `~/.local/bin/git` precedes everything else on `PATH`, so this binary calls git through an absolute path to avoid re-entering the wrapper.
//!
//! On macOS, `/opt/homebrew/bin/git` exists once Homebrew installs it, and the Apple shim `/usr/bin/git` exists from the start.

use std::path::PathBuf;
use std::process::Command;

/// Candidate locations of the real git, in order of preference.
#[cfg(unix)]
pub fn candidates() -> Vec<PathBuf> {
    [
        "/opt/homebrew/bin/git",
        "/usr/local/bin/git",
        "/usr/bin/git",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

/// Git for Windows installs under `%ProgramFiles%` for all users or under `%LOCALAPPDATA%` for one user.
#[cfg(windows)]
pub fn candidates() -> Vec<PathBuf> {
    [
        ("ProgramFiles", "Git/cmd/git.exe"),
        ("ProgramW6432", "Git/cmd/git.exe"),
        ("LOCALAPPDATA", "Programs/Git/cmd/git.exe"),
    ]
    .iter()
    .filter_map(|(variable, relative)| {
        std::env::var_os(variable).map(|base| PathBuf::from(base).join(relative))
    })
    .collect()
}

/// The first real git on disk, or `None` on a machine with no git at all.
pub fn find() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

/// A `Command` for the real git, for this process's own queries.
pub fn command() -> Option<Command> {
    find().map(Command::new)
}

/// Run the real git and return its stdout, or `None` if it exited non-zero.
pub fn capture(args: &[&str]) -> Option<String> {
    let out = command()?.args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

/// Run the real git for its exit status alone.
pub fn succeeds(args: &[&str]) -> bool {
    command()
        .and_then(|mut c| {
            c.args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .ok()
        })
        .is_some_and(|s| s.success())
}
