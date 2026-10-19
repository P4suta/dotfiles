//! Cleanup whose failure leaves the caller's own result standing, such as removing a temporary file.
//! The failure goes to standard error with what failed, so nothing discards it unseen.
//!
//! This module uses std only, so `dotguard` takes it without the platform crate's tools feature.

use std::fmt::Display;
use std::io;
use std::path::Path;

/// Reports `result`'s failure to do `what` on standard error, and returns its value when it succeeded.
pub fn report<T, E: Display>(result: Result<T, E>, what: impl Display) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            eprintln!("warning: could not {what}: {error}");
            None
        }
    }
}

/// Treats a missing path as already removed, which gives a removal what it wants.
///
/// # Errors
///
/// Returns every error but `NotFound`.
pub fn absent_ok(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Removes the file at `path` and reports any failure but its absence.
pub fn remove_file(path: &Path) {
    report(
        absent_ok(std::fs::remove_file(path)),
        format_args!("remove {}", path.display()),
    );
}

/// Removes the tree at `path` and reports any failure but its absence.
pub fn remove_dir_all(path: &Path) {
    report(
        absent_ok(std::fs::remove_dir_all(path)),
        format_args!("remove {}", path.display()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_path_counts_as_removed_and_other_failures_stay() {
        let missing = io::Error::from(io::ErrorKind::NotFound);
        assert!(absent_ok(Err(missing)).is_ok());
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert!(absent_ok(Err(denied)).is_err());
        assert_eq!(report::<u8, io::Error>(Ok(3), "read"), Some(3));
        assert_eq!(
            report::<u8, _>(Err(io::Error::from(io::ErrorKind::Other)), "read"),
            None
        );
    }

    #[test]
    fn removing_an_absent_path_reports_nothing_and_a_present_one_goes() -> io::Result<()> {
        let directory = std::env::temp_dir().join(format!("cleanup-{}", std::process::id()));
        std::fs::create_dir_all(&directory)?;
        let file = directory.join("stale");
        std::fs::write(&file, "x")?;
        remove_file(&file);
        assert!(!file.exists());
        remove_file(&file);
        remove_dir_all(&directory);
        assert!(!directory.exists());
        Ok(())
    }
}
