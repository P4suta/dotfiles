//! Locating the real git.
//!
//! `~/.local/bin/git` is on PATH ahead of everything else, so resolving "git" through PATH from inside this binary would re-enter the wrapper.
//! Every git invocation here therefore goes through an absolute path.
//!
//! macOS has two candidates and which one is real depends on how far Homebrew provisioning has got: `/opt/homebrew/bin/git` once Homebrew is installed, `/usr/bin/git` (Apple's shim) on every Mac from the start.

use std::path::{Path, PathBuf};
use std::process::Command;

const CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/git",
    "/usr/local/bin/git",
    "/usr/bin/git",
];

/// The first real git on disk, or `None` on a machine with no git at all.
pub fn find() -> Option<PathBuf> {
    CANDIDATES
        .iter()
        .map(Path::new)
        .find(|p| p.is_file())
        .map(Path::to_path_buf)
}

/// A `Command` for the real git, for this process's own queries.
///
/// Callers that cannot continue without git should treat `None` as fatal; the hook entry points instead degrade to a warning, because a missing git means the git operation we were asked to gate is not happening either.
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
