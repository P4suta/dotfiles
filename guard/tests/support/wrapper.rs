//! The installed Git wrapper form of the built dotguard binary.

use std::path::Path;

/// Places dotguard at `git`, the name it answers to as the wrapper.
/// Unix links it instead of copying.
/// A test thread that forks while it holds a copy open for writing hands that descriptor to its child, and running the copy then fails with `ETXTBSY`.
/// Windows has no such failure, and a copy needs no privilege there.
///
/// # Panics
///
/// Panics when placing the wrapper fails.
pub fn install(git: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_dotguard"), git).unwrap();
    #[cfg(not(unix))]
    std::fs::copy(env!("CARGO_BIN_EXE_dotguard"), git).unwrap();
}
