#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

//! The hook dispatcher and both pr-workflow entry points, run as installed: each refusal is the last line on standard error.

use anyhow::{Result, ensure};
use dotfiles_xtask::refusal::Refusal;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[expect(
    clippy::disallowed_methods,
    reason = "the shared constructor for fixture Git processes"
)]
#[path = "../../guard/tests/support/fixture_git.rs"]
mod fixture_git;

/// An isolated home with stand-ins for dotguard and lefthook, and a repository holding one staged file.
struct Host {
    scope: tempfile::TempDir,
}

impl Host {
    fn new() -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let bin = scope.path().join("home/.local/bin");
        fs::create_dir_all(&bin)?;
        fs::create_dir_all(scope.path().join("repository"))?;
        fs::write(
            scope.path().join("gitconfig"),
            "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
        )?;
        let status = Command::new("rustc")
            .args(["--edition", "2024"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dotguard.rs"))
            .arg("-o")
            .arg(bin.join(format!("dotguard{}", std::env::consts::EXE_SUFFIX)))
            .status()?;
        ensure!(status.success(), "fixture dotguard compilation failed");
        let status = Command::new("rustc")
            .args(["--edition", "2024"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lefthook.rs"))
            .arg("-o")
            .arg(bin.join(format!("lefthook{}", std::env::consts::EXE_SUFFIX)))
            .status()?;
        ensure!(status.success(), "fixture lefthook compilation failed");
        let host = Self { scope };
        host.run("git", &["init", "-q"])?;
        fs::write(host.repository().join("a.txt"), "a\n")?;
        host.run("git", &["add", "a.txt"])?;
        Ok(host)
    }

    fn home(&self) -> PathBuf {
        self.scope.path().join("home")
    }

    fn repository(&self) -> PathBuf {
        self.scope.path().join("repository")
    }

    fn command(&self, program: &str) -> Command {
        let mut command = fixture_git::command(program, &self.scope.path().join("gitconfig"));
        command
            .current_dir(self.repository())
            .env("HOME", self.home())
            .env("USERPROFILE", self.home());
        command
    }

    fn run(&self, program: &str, arguments: &[&str]) -> Result<Output> {
        let output = self.command(program).args(arguments).output()?;
        ensure!(
            output.status.success(),
            "{program} {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }

    fn hook(&self, arguments: &[&str]) -> Result<Output> {
        Ok(self
            .command(env!("CARGO_BIN_EXE_dotfiles-xtask"))
            .arg("hook")
            .args(arguments)
            .output()?)
    }
}

/// The record a refused command printed as its last line.
fn refusal(output: &Output) -> Refusal {
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refusal = Refusal::parse(stderr.lines().last().unwrap_or_default())
        .unwrap_or_else(|| panic!("the last line is not a refusal: {stderr}"));
    assert!(refusal.is_complete(), "{refusal:?}");
    refusal
}

#[test]
fn a_paused_push_is_refused_without_a_waiver() -> Result<()> {
    let host = Host::new()?;
    let marker = host.home().join(".config/git/push-paused");
    fs::create_dir_all(marker.parent().unwrap())?;
    fs::write(&marker, "")?;
    let refused =
        refusal(&host.hook(&["pre-push", "--", "origin", "https://example.invalid/r"])?);
    assert_eq!(refused.rule, "push.paused");
    assert_eq!(refused.waiver, None);
    assert!(refused.evidence[0].contains("push-paused"), "{refused:?}");
    Ok(())
}

#[test]
fn a_gate_that_changes_the_staged_tree_refuses_the_commit() -> Result<()> {
    let host = Host::new()?;
    let refused = refusal(&host.hook(&["pre-commit"])?);
    assert_eq!(refused.rule, "commit.staged-tree");
    assert_eq!(refused.waiver, None);
    Ok(())
}

#[test]
fn a_child_refusal_stays_last_and_a_bare_failure_gets_its_own_record() -> Result<()> {
    let host = Host::new()?;
    let message = host.scope.path().join("message");
    fs::write(&message, "fixture\n")?;
    let relayed = host.hook(&["commit-msg", "--", message.to_str().unwrap()])?;
    assert_eq!(refusal(&relayed).rule, "commit.language");
    assert!(
        !String::from_utf8_lossy(&relayed.stderr).contains("Error:"),
        "nothing follows a relayed refusal"
    );
    let failed = refusal(&host.hook(&["post-commit"])?);
    assert_eq!(failed.rule, "hook.gate");
    assert_eq!(
        failed.next.as_deref().unwrap_or_default(),
        "dotguard post-commit"
    );
    Ok(())
}

#[test]
fn a_lefthook_configuration_that_cannot_be_loaded_refuses_the_hook() -> Result<()> {
    let host = Host::new()?;
    fs::write(host.repository().join("lefthook.yml"), "pre-push: {}\n")?;
    let refused =
        refusal(&host.hook(&["pre-push", "--", "origin", "https://example.invalid/r"])?);
    assert_eq!(refused.rule, "hook.lefthook");
    assert_eq!(refused.next.as_deref().unwrap_or_default(), "lefthook dump");
    assert_eq!(refused.waiver, None);
    assert!(
        refused
            .evidence
            .iter()
            .any(|item| item.contains("invalid lefthook configuration")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn both_pr_workflow_entry_points_report_an_unclassified_failure_the_same_way() -> Result<()> {
    let host = Host::new()?;
    let arguments = [
        "check",
        "--repo",
        "owner/repository",
        "--title",
        "feat: fixture",
        "--body-file",
        "missing.md",
    ];
    let standalone = refusal(
        &host
            .command(env!("CARGO_BIN_EXE_pr-workflow"))
            .args(arguments)
            .output()?,
    );
    let through_xtask = refusal(
        &host
            .command(env!("CARGO_BIN_EXE_dotfiles-xtask"))
            .arg("pr-workflow")
            .args(arguments)
            .output()?,
    );
    assert_eq!(standalone, through_xtask);
    assert_eq!(standalone.rule, "pr.failed");
    assert_eq!(
        standalone.next.as_deref().unwrap_or_default(),
        "pr-workflow check --help"
    );
    assert_eq!(
        standalone.evidence,
        [
            "command: pr-workflow check --repo owner/repository --title 'feat: fixture' --body-file missing.md"
        ]
    );
    Ok(())
}
