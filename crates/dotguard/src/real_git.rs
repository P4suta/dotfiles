//! Locates the real git.
//!
//! `~/.local/bin/git` precedes everything else on `PATH`, so this binary calls git through an absolute path to avoid re-entering the wrapper.
//!
//! On macOS, `/opt/homebrew/bin/git` exists once Homebrew installs it, and the Apple shim `/usr/bin/git` exists from the start.

use std::path::PathBuf;
use std::process::Command;

/// Where Homebrew on Apple silicon, Homebrew on Intel and other local installs, and the system put git.
#[cfg(unix)]
const UNIX_GIT: [&str; 3] = [
    "/opt/homebrew/bin/git",
    "/usr/local/bin/git",
    "/usr/bin/git",
];

/// Candidate locations of the real git, in order of preference.
#[cfg(unix)]
pub fn updates() -> Vec<PathBuf> {
    UNIX_GIT.iter().map(PathBuf::from).collect()
}

/// Git for Windows installs under `%ProgramFiles%` for all users or under `%LOCALAPPDATA%` for one user.
#[cfg(windows)]
pub fn updates() -> Vec<PathBuf> {
    windows_candidates(|variable| std::env::var_os(variable))
}

/// The Windows candidates for the environment `lookup` reads.
/// A process can start without `ProgramFiles`, as a remote job does, so the default directory stays a candidate.
#[cfg(any(windows, test))]
fn windows_candidates(lookup: impl Fn(&str) -> Option<std::ffi::OsString>) -> Vec<PathBuf> {
    const GIT: &str = "Git/cmd/git.exe";
    let mut candidates: Vec<PathBuf> = [
        ("ProgramFiles", GIT),
        ("ProgramW6432", GIT),
        ("LOCALAPPDATA", "Programs/Git/cmd/git.exe"),
    ]
    .iter()
    .filter_map(|(variable, relative)| {
        lookup(variable).map(|base| PathBuf::from(base).join(relative))
    })
    .collect();
    let default = PathBuf::from(platform::home::DEFAULT_PROGRAM_FILES).join(GIT);
    if !candidates.contains(&default) {
        candidates.push(default);
    }
    candidates
}

/// The first real git on disk, or `None` on a machine with no git at all.
pub fn find() -> Option<PathBuf> {
    updates().into_iter().find(|p| p.is_file())
}

/// A `Command` for the real git, for this process's own queries.
pub fn command() -> Option<Command> {
    find().map(|path| at(&path))
}

/// A `Command` for the real git at `path`, which the wrapper must reach without its own hook PATH.
pub fn at(path: &std::path::Path) -> Command {
    platform::process::repository_command(path)
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

/// Run Git with an isolated index when checking an offered revision.
pub fn succeeds_with_env(variable: &str, value: &str, args: &[&str]) -> bool {
    command()
        .and_then(|mut command| {
            command
                .env(variable, value)
                .args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .ok()
        })
        .is_some_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::windows_candidates;
    use std::path::PathBuf;

    const DEFAULT_GIT: &str = "C:/Program Files/Git/cmd/git.exe";

    #[test]
    fn a_process_without_program_files_still_finds_the_default_git_for_windows() {
        assert_eq!(
            windows_candidates(|_| None),
            vec![PathBuf::from(DEFAULT_GIT)]
        );
        let local = PathBuf::from(r"C:\Users\owner\AppData\Local");
        let user = windows_candidates(|variable| {
            (variable == "LOCALAPPDATA").then(|| local.clone().into_os_string())
        });
        assert_eq!(
            user,
            vec![
                local.join("Programs/Git/cmd/git.exe"),
                PathBuf::from(DEFAULT_GIT)
            ]
        );
        let full = windows_candidates(|variable| {
            matches!(variable, "ProgramFiles" | "ProgramW6432").then(|| "C:/Program Files".into())
        });
        assert_eq!(full, vec![PathBuf::from(DEFAULT_GIT); 2]);
    }
}
