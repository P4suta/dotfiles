//! The hook dispatcher, run as installed.
//! Each refusal comes last on standard error.

use anyhow::{Result, ensure};
use dotfiles::refusal::Refusal;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[path = "support/catalog.rs"]
mod catalog;

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
        catalog::install(&scope.path().join("home"))?;
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
    assert_eq!(refused.bypass, None);
    assert!(refused.evidence[0].contains("push-paused"), "{refused:?}");
    Ok(())
}

#[test]
fn a_gate_that_changes_the_staged_tree_refuses_the_commit() -> Result<()> {
    let host = Host::new()?;
    let refused = refusal(&host.hook(&["pre-commit"])?);
    assert_eq!(refused.rule, "commit.staged-tree");
    assert_eq!(refused.bypass, None);
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
    eprintln!("{}", failed.line());
    assert_eq!(failed.rule, "hook.gate");
    assert_eq!(
        failed.next.as_deref().unwrap_or_default(),
        "dotguard post-commit"
    );
    Ok(())
}

#[test]
fn post_commit_refreshes_changed_tools_once_and_keeps_failed_installations_stale() -> Result<()> {
    use platform::build_inputs::{CRATES, content_hash};
    use platform::freshness::{Built, Embedded, lagging, record_installation, verify};

    let host = Host::new()?;
    for krate in CRATES {
        fs::create_dir_all(host.repository().join(krate).join("src"))?;
    }
    let library = host.repository().join("crates/platform/src/lib.rs");
    fs::write(
        host.repository().join("crates/platform/Cargo.toml"),
        "[package]\nname = \"platform\"\n",
    )?;
    fs::write(&library, "pub fn current() {}\n")?;
    for built in Built::ALL {
        record_installation(&host.home(), &host.repository(), built)?;
    }
    let stamp = Embedded {
        revision: "fixture".into(),
        root: host.repository(),
        hash: content_hash(&host.repository())?,
    };
    let just = host
        .home()
        .join(".local/bin")
        .join(format!("just{}", std::env::consts::EXE_SUFFIX));
    let status = platform::process::command("rustc")
        .args(["--edition", "2024"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/just.rs"))
        .args(["-o"])
        .arg(&just)
        .status()?;
    ensure!(status.success(), "fixture just compilation failed");
    let events = host.scope.path().join("hook-events");
    let run = |failure: Option<&str>| -> Result<Output> {
        let mut command = host.command(env!("CARGO_BIN_EXE_dotfiles"));
        let mut path = vec![just.parent().unwrap().to_path_buf()];
        path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command
            .args(["hook", "post-commit"])
            .env("PATH", std::env::join_paths(path)?)
            .env("FIXTURE_HOOK_LOG", &events)
            .env("FIXTURE_BUILD_HASH", content_hash(&host.repository())?);
        if let Some(failure) = failure {
            command.env("FIXTURE_INSTALL_FAILURE", failure);
        } else {
            command.env_remove("FIXTURE_INSTALL_FAILURE");
        }
        Ok(command.output()?)
    };
    fs::write(&library, "pub fn changed() {}\n")?;
    let stale = verify(Built::SkillOps, &stamp, None, Some(&host.repository())).unwrap_err();
    let stale = stale.downcast_ref::<platform::refusal::Refusal>().unwrap();
    assert_eq!(stale.rule, "tool.stale");
    println!("\n{}", stale.line());
    let refreshed = run(None)?;
    assert!(
        refreshed.status.success(),
        "{}",
        String::from_utf8_lossy(&refreshed.stderr)
    );
    assert_eq!(
        lagging(&host.home(), &host.repository())?,
        Vec::<Built>::new()
    );
    let installed = "dotguard\ninstall-review-guard\ninstall-pr-workflow\ninstall-skill-ops\n";
    assert_eq!(fs::read_to_string(&events)?, installed);
    assert!(run(None)?.status.success());
    assert_eq!(
        fs::read_to_string(&events)?,
        format!("{installed}dotguard\n")
    );
    fs::write(&library, "pub fn changed_again() {}\n")?;
    let failed = run(Some("install-skill-ops"))?;
    assert!(failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr)
            .contains("reinstalling tools built from changed sources failed")
    );
    assert_eq!(
        lagging(&host.home(), &host.repository())?,
        [Built::SkillOps]
    );
    assert!(run(None)?.status.success());
    assert_eq!(
        lagging(&host.home(), &host.repository())?,
        Vec::<Built>::new()
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
    assert_eq!(refused.bypass, None);
    assert!(
        refused
            .evidence
            .iter()
            .any(|item| item.contains("invalid lefthook configuration")),
        "{refused:?}"
    );
    Ok(())
}
