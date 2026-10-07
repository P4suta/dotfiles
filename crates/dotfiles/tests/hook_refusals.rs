//! The hook dispatcher, run as installed.
//! Each refusal comes last on standard error.

use anyhow::{Result, ensure};
use dotfiles::refusal::Refusal;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "../../dotguard/tests/support/fixture_git.rs"]
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
        let status = platform::process::command("rustc")
            .args(["--edition", "2024"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dotguard.rs"))
            .arg("-o")
            .arg(bin.join(format!("dotguard{}", std::env::consts::EXE_SUFFIX)))
            .status()?;
        ensure!(status.success(), "fixture dotguard compilation failed");
        let status = platform::process::command("rustc")
            .args(["--edition", "2024"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lefthook.rs"))
            .arg("-o")
            .arg(bin.join(format!("lefthook{}", std::env::consts::EXE_SUFFIX)))
            .status()?;
        ensure!(status.success(), "fixture lefthook compilation failed");
        let catalog = scope.path().join("home").join(dotfiles::assure::CATALOG);
        fs::create_dir_all(catalog.parent().unwrap_or(scope.path()))?;
        fs::write(
            catalog,
            include_str!("../../../home/dot_config/assure/shapes.toml"),
        )?;
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
            .command(env!("CARGO_BIN_EXE_dotfiles"))
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
fn a_commit_that_adds_an_unowned_parser_is_refused_with_its_next_action() -> Result<()> {
    let host = Host::new()?;
    fs::create_dir_all(host.repository().join("src"))?;
    fs::write(
        host.repository().join("src/lib.rs"),
        "pub fn parse(text: &str) -> bool {\n    text.is_empty()\n}\n",
    )?;
    host.run("git", &["add", "src/lib.rs"])?;
    let refused = refusal(&host.hook(&["pre-commit"])?);
    assert_eq!(refused.rule, "assure.added");
    assert!(
        refused.evidence[0].starts_with("src/lib.rs:1: parser has no owner")
            && refused.evidence[0].contains("next: add fuzz/fuzz_targets/"),
        "{refused:?}"
    );
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
fn a_repository_with_its_own_lefthook_configuration_has_its_mise_configuration_trusted_first()
-> Result<()> {
    let host = Host::new()?;
    let status = platform::process::command("rustc")
        .args(["--edition", "2024"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mise.rs"))
        .arg("-o")
        .arg(
            host.home()
                .join(".local/bin")
                .join(format!("mise{}", std::env::consts::EXE_SUFFIX)),
        )
        .status()?;
    ensure!(status.success(), "fixture mise compilation failed");
    let log = host.scope.path().join("mise.log");
    let configuration = host.repository().join("mise.toml");
    fs::write(&configuration, "[tools]\n")?;
    let hook = || {
        host.command(env!("CARGO_BIN_EXE_dotfiles"))
            .env("MISE_FIXTURE_LOG", &log)
            .args(["hook", "pre-commit"])
            .output()
    };
    assert_eq!(refusal(&hook()?).rule, "commit.staged-tree");
    assert!(
        !log.exists(),
        "a repository without its own hooks keeps its trust"
    );
    fs::write(host.repository().join("lefthook.yml"), "pre-commit: {}\n")?;
    assert_eq!(refusal(&hook()?).rule, "hook.lefthook");
    let recorded = fs::read_to_string(&log)?;
    let trusted: Vec<_> = recorded
        .lines()
        .map(|line| {
            line.strip_prefix("trust --quiet ")
                .map(|path| dotfiles::canonical(Path::new(path)).ok())
        })
        .collect();
    assert_eq!(
        trusted,
        [Some(Some(dotfiles::canonical(&configuration)?))],
        "{recorded}"
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
