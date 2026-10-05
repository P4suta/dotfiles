use anyhow::{Result, ensure};
use dotfiles_xtask::change::{Source, gates, paths};
use std::fs;
use std::path::Path;

#[path = "../../guard/tests/support/fixture_git.rs"]
mod fixture_git;

fn git(directory: &Path, arguments: &[&str]) -> Result<()> {
    let output = fixture_git::command("git", &directory.with_extension("gitconfig"))
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// A repository whose `main` holds one commit and whose `topic` branch starts there.
fn repository() -> Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    git(root, &["init", "--quiet", "--initial-branch=main"])?;
    for (key, value) in [
        ("user.name", "fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(root, &["config", key, value])?;
    }
    fs::write(root.join("README.md"), "start\n")?;
    git(root, &["add", "."])?;
    git(root, &["commit", "--quiet", "-m", "start"])?;
    git(root, &["switch", "--quiet", "-c", "topic"])?;
    Ok(directory)
}

fn commit(root: &Path, files: &[(&str, &str)]) -> Result<()> {
    for (path, text) in files {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("fixture parent"))?;
        fs::write(file, text)?;
    }
    git(root, &["add", "."])?;
    git(root, &["commit", "--quiet", "-m", "change"])
}

#[test]
fn a_markdown_only_branch_selects_the_prose_gate_alone() -> Result<()> {
    let repository = repository()?;
    commit(repository.path(), &[("docs/guide.md", "words\n")])?;
    let base = Source::Base("main".into());
    assert_eq!(paths(repository.path(), &base)?, ["docs/guide.md"]);
    assert_eq!(gates(repository.path(), &base)?.names(), "prose");
    Ok(())
}

#[test]
fn a_rust_change_selects_the_rust_gate() -> Result<()> {
    let repository = repository()?;
    commit(repository.path(), &[("xtask/src/new.rs", "fn main() {}\n")])?;
    let selected = gates(repository.path(), &Source::Base("main".into()))?;
    assert_eq!(selected.names(), "prose,skills,rust,profiles");
    Ok(())
}

#[test]
fn an_unknown_path_selects_every_gate() -> Result<()> {
    let repository = repository()?;
    commit(
        repository.path(),
        &[("docs/guide.md", "words\n"), ("unlisted/file", "x\n")],
    )?;
    let selected = gates(repository.path(), &Source::Base("main".into()))?;
    assert_eq!(selected.names(), "prose,skills,rust,profiles");
    Ok(())
}

#[test]
fn a_rename_classifies_both_the_old_and_the_new_path() -> Result<()> {
    let repository = repository()?;
    let root = repository.path();
    git(root, &["switch", "--quiet", "main"])?;
    commit(root, &[("xtask/src/old.rs", "fn main() {}\n")])?;
    git(root, &["switch", "--quiet", "-c", "rename"])?;
    fs::create_dir_all(root.join("docs"))?;
    git(root, &["mv", "xtask/src/old.rs", "docs/old.md"])?;
    git(root, &["commit", "--quiet", "-m", "move"])?;
    let selected = gates(root, &Source::Base("main".into()))?;
    assert_eq!(selected.names(), "prose,skills,rust,profiles");
    Ok(())
}

#[test]
fn staged_paths_are_classified_and_unstaged_ones_are_not() -> Result<()> {
    let repository = repository()?;
    let root = repository.path();
    fs::write(root.join("dot_zshrc.tmpl"), "x\n")?;
    git(root, &["add", "dot_zshrc.tmpl"])?;
    fs::write(root.join("xtask.rs"), "x\n")?;
    assert_eq!(gates(root, &Source::Staged)?.names(), "prose,profiles");
    Ok(())
}

#[test]
fn an_unknown_base_is_an_error_rather_than_a_quiet_selection() -> Result<()> {
    let repository = repository()?;
    assert!(paths(repository.path(), &Source::Base("missing".into())).is_err());
    Ok(())
}
