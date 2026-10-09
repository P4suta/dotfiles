//! Isolated content of the Git index for commit checks.

use crate::tool::Tool;
use anyhow::{Result, ensure};
use std::path::Path;

/// Exports the index and records its paths in a temporary repository without changing the source index.
///
/// # Errors
///
/// Returns an error when Git can't export or index the snapshot.
pub fn export(root: &Path) -> Result<tempfile::TempDir> {
    let snapshot = tempfile::tempdir()?;
    let prefix = format!(
        "--prefix={}/",
        snapshot.path().to_string_lossy().replace('\\', "/")
    );
    let status = Tool::Git
        .command()
        .current_dir(root)
        .args(["checkout-index", "--all"])
        .arg(prefix)
        .status()?;
    ensure!(status.success(), "Git could not export the staged content");
    for args in [vec!["init", "-q"], vec!["add", "--all", "--force"]] {
        let status = Tool::Git
            .command()
            .current_dir(snapshot.path())
            .args(args)
            .status()?;
        ensure!(status.success(), "Git could not index the staged snapshot");
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_valid_working_file_cannot_hide_an_invalid_staged_script() {
        let source = tempfile::tempdir().unwrap();
        for args in [vec!["init", "-q"], vec!["config", "core.autocrlf", "false"]] {
            assert!(
                Tool::Git
                    .command()
                    .current_dir(source.path())
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let path = source.path().join("script.sh");
        fs::write(&path, "#!/bin/sh\nif then\n").unwrap();
        assert!(
            Tool::Git
                .command()
                .current_dir(source.path())
                .args(["add", "script.sh"])
                .status()
                .unwrap()
                .success()
        );
        fs::write(&path, "#!/bin/sh\nprintf '%s' ok\n").unwrap();
        let snapshot = export(source.path()).unwrap();
        let scope = vec!["script.sh".to_owned()];
        assert!(
            crate::lint::findings(source.path(), crate::lint::Stage::Shellcheck, &scope)
                .unwrap()
                .is_empty()
        );
        assert!(
            !crate::lint::findings(snapshot.path(), crate::lint::Stage::Shellcheck, &scope)
                .unwrap()
                .is_empty()
        );
    }
}
