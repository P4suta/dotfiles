//! The staged change of a merge counts only what differs from the merged default branch.

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;

/// Run Git in a fixture checkout without the host's global hooks or identity.
fn git(directory: &Path, global: &Path, arguments: &[&str]) -> Result<()> {
    let output = fixture_git::command("git", global)
        .current_dir(directory)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn commit(directory: &Path, global: &Path, files: &[(&str, &str)], message: &str) -> Result<()> {
    for (path, text) in files {
        fs::write(directory.join(path), text)?;
    }
    git(directory, global, &["add", "--all"])?;
    git(directory, global, &["commit", "--quiet", "-m", message])
}

#[test]
fn a_merge_of_the_default_branch_counts_only_what_differs_from_it() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let global = scope.path().join("gitconfig");
    fs::write(
        &global,
        "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n",
    )?;
    let repository = scope.path().join("repository");
    fs::create_dir(&repository)?;
    let at = |arguments: &[&str]| git(&repository, &global, arguments);
    at(&["init", "--quiet", "--initial-branch=main"])?;
    commit(
        &repository,
        &global,
        &[
            ("own.txt", "base"),
            ("upstream.txt", "base"),
            ("resolved.txt", "base"),
        ],
        "base",
    )?;
    at(&["switch", "--quiet", "-c", "side"])?;
    commit(&repository, &global, &[("side.txt", "side")], "side")?;
    at(&["switch", "--quiet", "main"])?;
    at(&["switch", "--quiet", "-c", "feature"])?;
    commit(&repository, &global, &[("own.txt", "feature")], "feature")?;
    at(&["switch", "--quiet", "main"])?;
    commit(
        &repository,
        &global,
        &[("upstream.txt", "main"), ("resolved.txt", "main")],
        "upstream",
    )?;
    at(&["update-ref", "refs/remotes/origin/main", "main"])?;
    at(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ])?;
    at(&["switch", "--quiet", "feature"])?;

    at(&["merge", "--quiet", "--no-commit", "--no-ff", "origin/main"])?;
    fs::write(repository.join("resolved.txt"), "by hand")?;
    at(&["add", "resolved.txt"])?;
    assert_eq!(
        platform::issue_context::paths(&repository, None, true)?,
        ["resolved.txt"]
    );
    at(&["merge", "--abort"])?;

    at(&["merge", "--quiet", "--no-commit", "--no-ff", "side"])?;
    assert_eq!(
        platform::issue_context::paths(&repository, None, true)?,
        ["side.txt"]
    );
    Ok(())
}
