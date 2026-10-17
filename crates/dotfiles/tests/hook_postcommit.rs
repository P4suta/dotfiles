//! The installed post-commit hook keeps signing and rollback independent of the policy companion.

use anyhow::{Result, ensure};
use dotfiles::refusal::Refusal;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

struct Repository {
    scope: tempfile::TempDir,
}

impl Repository {
    fn new() -> Result<Self> {
        let scope = tempfile::tempdir()?;
        fs::create_dir_all(scope.path().join("home"))?;
        fs::create_dir_all(scope.path().join("repository"))?;
        fs::write(
            scope.path().join("gitconfig"),
            "[user]\nname = Fixture\nemail = fixture@example.invalid\n",
        )?;
        let repository = Self { scope };
        repository.git(&["init", "-q", "-b", "main"])?;
        Ok(repository)
    }

    fn path(&self) -> PathBuf {
        self.scope.path().join("repository")
    }

    fn command(&self, program: &str) -> Command {
        let mut command = fixture_git::command(program, &self.scope.path().join("gitconfig"));
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("ALLOW_") {
                command.env_remove(name);
            }
        }
        command
            .current_dir(self.path())
            .env("HOME", self.scope.path().join("home"))
            .env("USERPROFILE", self.scope.path().join("home"));
        command
    }

    fn git(&self, arguments: &[&str]) -> Result<String> {
        let output = self.command("git").args(arguments).output()?;
        ensure!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    fn commit(&self, content: &str, parent: Option<&str>) -> Result<String> {
        fs::write(self.path().join("file"), content)?;
        self.git(&["add", "file"])?;
        let tree = self.git(&["write-tree"])?;
        let mut arguments = vec!["commit-tree", &tree, "-m", "fixture"];
        if let Some(parent) = parent {
            arguments.extend(["-p", parent]);
        }
        let commit = self.git(&arguments)?;
        self.git(&["update-ref", "HEAD", &commit])?;
        Ok(commit)
    }

    fn hook(&self) -> Result<Output> {
        Ok(self
            .command(env!("CARGO_BIN_EXE_dotfiles"))
            .args(["hook", "post-commit"])
            .output()?)
    }
}

fn refusal(output: &Output, rule: &str) {
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let record = Refusal::parse(stderr.lines().last().unwrap_or_default())
        .unwrap_or_else(|| panic!("the final line must be a refusal: {stderr}"));
    assert_eq!(record.rule, rule);
    assert!(record.is_complete());
    assert_eq!(stderr.matches("dotfiles-refusal/2").count(), 1, "{stderr}");
}

#[test]
fn the_managed_hook_rolls_back_empty_commits_without_an_installed_guard() -> Result<()> {
    let repository = Repository::new()?;
    let base = repository.commit("base", None)?;
    assert!(repository.hook()?.status.success());
    repository.commit("base", Some(&base))?;
    refusal(&repository.hook()?, "commit.empty");
    assert_eq!(repository.git(&["rev-parse", "HEAD"])?, base);
    assert_eq!(fs::read_to_string(repository.path().join("file"))?, "base");
    assert!(!repository.scope.path().join("home/.local/bin").exists());
    Ok(())
}

#[test]
fn the_managed_hook_rolls_back_failed_signatures_without_an_installed_guard() -> Result<()> {
    let repository = Repository::new()?;
    let base = repository.commit("base", None)?;
    let unsigned = repository.commit("changed", Some(&base))?;
    let missing = repository.path().join("missing-signer");
    let missing = missing.to_str().unwrap();
    repository.git(&["config", "commit.gpgsign", "true"])?;
    repository.git(&["config", "gpg.format", "ssh"])?;
    repository.git(&["config", "user.signingkey", missing])?;
    repository.git(&["config", "gpg.ssh.program", missing])?;
    refusal(&repository.hook()?, "commit.signature");
    assert_eq!(repository.git(&["rev-parse", "HEAD"])?, base);
    assert_eq!(
        repository.git(&["write-tree"])?,
        repository.git(&["rev-parse", &format!("{unsigned}^{{tree}}")])?
    );
    assert!(!repository.scope.path().join("home/.local/bin").exists());
    Ok(())
}

#[test]
fn stale_signing_workers_refuse_before_mutating_head() -> Result<()> {
    let repository = Repository::new()?;
    let base = repository.commit("base", None)?;
    let empty = repository.commit("base", Some(&base))?;
    let output = repository
        .command(env!("CARGO_BIN_EXE_dotfiles"))
        .args(["post-commit-worker", "different-source"])
        .output()?;
    refusal(&output, "tool.stale");
    assert_eq!(repository.git(&["rev-parse", "HEAD"])?, empty);
    Ok(())
}
