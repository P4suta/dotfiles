//! Processes whose incident records belong to an owned test fixture.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const INCIDENT_STATE: &str = "SKILL_OPS_STATE";
pub const MACHINE_STATE: &str = ".local/state/skill-ops";

/// The canonical directory of an owned temporary fixture.
#[must_use]
pub fn directory(scope: &Path) -> PathBuf {
    let scope = scope.canonicalize().expect("an existing fixture directory");
    let temporary = std::env::temp_dir()
        .canonicalize()
        .expect("an existing temporary directory");
    assert!(
        scope.starts_with(&temporary) && scope != temporary,
        "the incident store must belong to an owned temporary fixture"
    );
    if let Some(state) = platform::home::home().map(|home| home.join(MACHINE_STATE)) {
        let state = state.canonicalize().unwrap_or(state);
        assert!(
            !scope.starts_with(state),
            "a fixture cannot use the machine incident store"
        );
    }
    scope
}

#[must_use]
pub fn command(program: impl AsRef<OsStr>, scope: &Path) -> Command {
    let scope = directory(scope);
    let mut command = platform::process::command(program);
    command.env(INCIDENT_STATE, scope.join("state"));
    command
}
