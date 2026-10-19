use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::Path;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const CHILD_ROOT: &str = "DOTFILES_SIGNED_SOURCE_ROOT";
const CHILD_HEAD: &str = "DOTFILES_SIGNED_SOURCE_HEAD";
const CHILD_TREE: &str = "DOTFILES_SIGNED_SOURCE_TREE";
const ORIGINAL: &str = "original source";
const REPLACEMENT: &str = "replacement source";
const TRANSACTION: &str = "transaction source";

fn git(root: &Path, global: &Path, arguments: &[&str], index: Option<&Path>) -> Result<String> {
    let mut command = fixture_git::command("git", global);
    command.current_dir(root).args(arguments);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "fixture Git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn replace_source(root: &Path, global: &Path) -> Result<(String, String)> {
    git(root, global, &["init", "--quiet"], None)?;
    for (key, value) in [
        ("user.name", "Fixture"),
        ("user.email", "fixture@example.com"),
    ] {
        git(root, global, &["config", key, value], None)?;
    }
    fs::write(root.join("source.txt"), ORIGINAL)?;
    git(root, global, &["add", "source.txt"], None)?;
    let original_tree = git(root, global, &["write-tree"], None)?;
    let head = git(
        root,
        global,
        &["commit-tree", &original_tree, "-m", "original"],
        None,
    )?;
    git(root, global, &["update-ref", "HEAD", &head], None)?;
    fs::write(root.join("source.txt"), REPLACEMENT)?;
    git(root, global, &["add", "source.txt"], None)?;
    let replacement_tree = git(root, global, &["write-tree"], None)?;
    let replacement = git(
        root,
        global,
        &["commit-tree", &replacement_tree, "-m", "replacement"],
        None,
    )?;
    git(root, global, &["replace", &head, &replacement], None)?;
    assert_ne!(original_tree, replacement_tree);
    assert_eq!(
        git(
            root,
            global,
            &["rev-parse", &format!("{head}^{{tree}}")],
            None
        )?,
        replacement_tree
    );
    assert_eq!(
        git(root, global, &["show", &format!("{head}:source.txt")], None)?,
        REPLACEMENT
    );
    Ok((head, original_tree))
}

#[test]
fn signed_source_ignores_replaced_objects_and_preserves_the_transaction_index() -> Result<()> {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = Path::new(&root);
        let head = std::env::var(CHILD_HEAD)?;
        let tree = std::env::var(CHILD_TREE)?;
        let reference = format!("{head}^{{tree}}");
        for actual in [
            platform::signed_git::git(root, &["rev-parse", &reference], None)?,
            platform::signed_git::index_git(root, &["rev-parse", &reference])?,
        ] {
            assert_eq!(actual.trim(), tree);
        }
        let reference = format!("{head}:source.txt");
        for actual in [
            platform::signed_git::git(root, &["show", &reference], None)?,
            platform::signed_git::index_git(root, &["show", &reference])?,
        ] {
            assert_eq!(actual, ORIGINAL);
        }
        assert_eq!(
            platform::signed_git::git(root, &["show", ":source.txt"], None)?,
            REPLACEMENT
        );
        assert_eq!(
            platform::signed_git::index_git(root, &["show", ":source.txt"])?,
            TRANSACTION
        );
        return Ok(());
    }

    let scope = tempfile::tempdir()?;
    let global = scope.path().join("gitconfig");
    fs::write(&global, "")?;
    let root = scope.path().join("repository");
    fs::create_dir(&root)?;
    let (head, original_tree) = replace_source(&root, &global)?;
    let index = scope.path().join("transaction-index");
    git(&root, &global, &["read-tree", "--empty"], Some(&index))?;
    fs::write(root.join("source.txt"), TRANSACTION)?;
    git(&root, &global, &["add", "source.txt"], Some(&index))?;
    let output = fixture_git::command(std::env::current_exe()?, &global)
        .args([
            "--exact",
            "signed_source_ignores_replaced_objects_and_preserves_the_transaction_index",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &root)
        .env(CHILD_HEAD, head)
        .env(CHILD_TREE, original_tree)
        .env("GIT_INDEX_FILE", &index)
        .output()
        .context("run the isolated production source reader")?;
    ensure!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
