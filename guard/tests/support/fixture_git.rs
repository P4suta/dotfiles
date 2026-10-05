//! The one way a test starts a process that runs Git against a fixture repository, shared by the guard and xtask test suites.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

/// Starts `program` so every Git it runs sees only the fixture: its repository, `global` as the only configuration, and no user attributes or ignore rules.
/// A commit hook exports its repository through the variables `git rev-parse --local-env-vars` lists, and a caller may pass configuration through other `GIT_CONFIG_*` variables; every inherited `GIT_` variable is removed, which covers both.
/// Git reads attributes and ignore rules from `$XDG_CONFIG_HOME/git`, or `~/.config/git`, whatever `GIT_CONFIG_GLOBAL` names, so the configuration home moves to a path beside `global` that holds nothing.
#[must_use]
#[allow(
    clippy::disallowed_methods,
    reason = "the shared constructor for fixture Git processes"
)]
pub fn command(program: impl AsRef<OsStr>, global: &Path) -> Command {
    let mut command = Command::new(program);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_CONFIG_GLOBAL", global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("XDG_CONFIG_HOME", global.with_extension("config-home"));
    command
}
