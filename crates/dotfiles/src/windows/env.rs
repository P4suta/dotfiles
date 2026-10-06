//! The Windows setup's view of the platform home, known folders, and managed `PATH`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

/// A process for `program` with the managed `PATH` and no automatic mise installation.
pub fn command(program: &str) -> Command {
    let mut cmd = platform::process::command(program);
    cmd.env("PATH", augmented_path());
    cmd.env("MISE_AUTO_INSTALL", "false");
    cmd
}

/// The managed tool directories followed by this process's `PATH`.
pub fn augmented_path() -> OsString {
    home_dir()
        .and_then(|home| platform::home::managed_path(&home).ok())
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default()
}

pub fn scoop_root() -> PathBuf {
    home_dir().map_or_else(
        || PathBuf::from("scoop"),
        |home| platform::home::scoop_root(&home),
    )
}

pub fn local_app_data() -> Option<PathBuf> {
    home_dir().map(|home| platform::home::local_app_data(&home))
}

pub fn home_dir() -> Option<PathBuf> {
    platform::home::home()
}
