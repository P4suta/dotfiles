//! The installed Git wrapper form of the built dotguard binary.

use std::path::{Path, PathBuf};

/// Places dotguard as `git` under `directory`, the name it answers to as the wrapper, the way Windows installs it.
/// Call it only from the one preparation step a test binary runs before it starts any process.
/// A child forked on Linux holds every descriptor of this process until it executes.
/// An executable written while another test spawns can then stay open for writing when it runs, and executing it fails with `ETXTBSY`.
///
/// # Panics
///
/// Panics when placing the wrapper fails.
pub fn place(directory: &Path) -> PathBuf {
    std::fs::create_dir_all(directory).unwrap();
    let git = directory.join(format!("git{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_dotguard"), &git).unwrap();
    git
}

/// The directory a test binary prepares its executables in, emptied first.
///
/// # Panics
///
/// Panics when emptying the directory fails.
pub fn preparation(name: &str) -> PathBuf {
    let directory =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    match std::fs::remove_dir_all(&directory) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => panic!("{error}"),
        _ => directory,
    }
}
