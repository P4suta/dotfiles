//! The installed pr-workflow reports a failure that no rule classifies as one complete refusal, last on standard error.

use anyhow::Result;
use platform::refusal::Refusal;
use std::fs;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[test]
fn an_unclassified_failure_is_reported_as_a_complete_refusal() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let global = scope.path().join("gitconfig");
    fs::write(&global, "")?;
    let output = fixture_git::command(env!("CARGO_BIN_EXE_pr-workflow"), &global)
        .current_dir(scope.path())
        .env("HOME", scope.path())
        .env("USERPROFILE", scope.path())
        .args([
            "check",
            "--repo",
            "owner/repository",
            "--title",
            "feat: fixture",
            "--body-file",
            "missing.md",
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refused = Refusal::parse(stderr.lines().last().unwrap_or_default())
        .unwrap_or_else(|| panic!("the last line is not a refusal: {stderr}"));
    assert!(refused.is_complete(), "{refused:?}");
    assert_eq!(refused.rule, "pr.failed");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        "pr-workflow check --help"
    );
    assert_eq!(
        refused.evidence,
        [
            "command: pr-workflow check --repo owner/repository --title 'feat: fixture' --body-file missing.md"
        ]
    );
    Ok(())
}
