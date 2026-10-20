use anyhow::{Context, Result, bail, ensure};
use dotfiles::runtime::{Reply, Runner};
use dotfiles::source_sync;
use dotfiles::source_sync_rules::{Decision, SkipReason};
use dotfiles::tool::Tool;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    None,
    DirtyAfterFetch,
    MergeFailed,
    FetchCannotStart,
}

#[derive(Debug)]
struct Fixture {
    _scope: tempfile::TempDir,
    global: PathBuf,
    upstream: PathBuf,
    remote: PathBuf,
    source: PathBuf,
    home: PathBuf,
    config: PathBuf,
    calls: Vec<Vec<OsString>>,
    https_route: Option<String>,
    fault: Fault,
}

impl Fixture {
    fn new() -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let root = dotfiles::canonical(scope.path())?;
        let fixture = Self {
            global: root.join("gitconfig"),
            upstream: root.join("upstream"),
            remote: root.join("remote.git"),
            source: root.join("checkout"),
            home: root.join("home"),
            config: root.join("chezmoi.toml"),
            calls: Vec::new(),
            https_route: None,
            fault: Fault::None,
            _scope: scope,
        };
        fs::write(
            &fixture.global,
            "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
        )?;
        fs::create_dir_all(&fixture.home)?;
        fs::create_dir_all(&fixture.upstream)?;
        fixture.git(
            &fixture.upstream,
            &["init", "--quiet", "--initial-branch=main"],
        )?;
        fs::create_dir(fixture.upstream.join("home"))?;
        fs::write(fixture.upstream.join("home/dot_fixture"), "initial")?;
        fixture.commit(&fixture.upstream)?;
        fixture.git(
            &root,
            &[
                "clone",
                "--quiet",
                "--bare",
                fixture.upstream.to_str().context("upstream path")?,
                fixture.remote.to_str().context("remote path")?,
            ],
        )?;
        fixture.git(
            &root,
            &[
                "clone",
                "--quiet",
                fixture.remote.to_str().context("remote path")?,
                fixture.source.to_str().context("source path")?,
            ],
        )?;
        fs::write(&fixture.config, "machine-local fixture")?;
        Ok(fixture)
    }

    fn git(&self, directory: &Path, arguments: &[&str]) -> Result<String> {
        let output = fixture_git::command("git", &self.global)
            .current_dir(directory)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    fn commit(&self, directory: &Path) -> Result<()> {
        self.git(directory, &["add", "."])?;
        self.git(directory, &["commit", "--quiet", "-m", "fixture"])?;
        Ok(())
    }

    fn advance(&self) -> Result<String> {
        fs::write(self.upstream.join("home/dot_fixture"), "merged main")?;
        self.commit(&self.upstream)?;
        self.git(
            &self.upstream,
            &[
                "push",
                "--quiet",
                self.remote.to_str().context("remote path")?,
                "main",
            ],
        )?;
        self.git(&self.upstream, &["rev-parse", "HEAD"])
    }

    fn synchronize(
        &mut self,
        apply: impl FnOnce(&Path, &Path, &mut Self) -> Result<()>,
    ) -> Result<Decision> {
        source_sync::upgrade(&self.config.clone(), &self.home.clone(), self, apply)
    }

    fn report(&self) -> Result<String> {
        let log = self.home.join(".local/state/dotfiles/hook-failures.log");
        Ok(
            dotfiles::hook_failures::report(&dotfiles::hook_failures::take(&log)?)
                .unwrap_or_default(),
        )
    }
}

impl Runner for Fixture {
    fn available(&self, _tool: Tool) -> bool {
        true
    }

    fn run(&mut self, tool: Tool, arguments: &[OsString]) -> Result<Reply> {
        use Tool::{Chezmoi, Git};

        if tool == Chezmoi {
            let expected: [OsString; 5] = [
                "--config".into(),
                self.config.clone().into(),
                "--destination".into(),
                self.home.clone().into(),
                "source-path".into(),
            ];
            assert_eq!(arguments, expected);
            return Ok(Reply {
                success: true,
                bytes: self
                    .source
                    .join("home")
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec(),
            });
        }
        ensure!(tool == Git, "unexpected tool: {tool:?}");
        self.calls.push(arguments.to_vec());
        let mut invocation = arguments.to_vec();
        let fetch = arguments.iter().position(|argument| argument == "fetch");
        if fetch.is_some() && self.fault == Fault::FetchCannotStart {
            bail!("fixture fetch could not start");
        }
        if arguments.contains(&OsString::from("merge")) && self.fault == Fault::MergeFailed {
            return Ok(Reply {
                success: false,
                bytes: Vec::new(),
            });
        }
        if let (Some(fetch), Some(expected)) = (fetch, &self.https_route) {
            let url = arguments
                .iter()
                .position(|argument| argument == expected.as_str())
                .context("fetch must explicitly use HTTPS")?;
            let resolved = fixture_git::command("git", &self.global)
                .args(&arguments[..fetch])
                .args(["ls-remote", "--get-url", expected])
                .output()?;
            ensure!(
                resolved.status.success(),
                "resolve the unattended transport"
            );
            assert_eq!(String::from_utf8(resolved.stdout)?.trim(), expected);
            invocation[url] = self.remote.clone().into();
        }
        let output = fixture_git::command("git", &self.global)
            .args(invocation)
            .output()?;
        if fetch.is_some() && self.fault == Fault::DirtyAfterFetch {
            fs::write(
                self.source.join("home/dot_fixture"),
                "concurrent local work",
            )?;
        }
        Ok(Reply {
            success: output.status.success(),
            bytes: output.stdout,
        })
    }

    fn run_installed(&mut self, _program: &Path, _arguments: &[OsString]) -> Result<Reply> {
        bail!("no installed program belongs in source synchronization")
    }
}

#[test]
fn a_clean_applied_worktree_fast_forwards_and_applies_with_a_fresh_backup() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.git(&fixture.source, &["switch", "--quiet", "-c", "development"])?;
    let applied = fixture.home.join("applied-source");
    fixture.git(
        &fixture.source,
        &[
            "worktree",
            "add",
            "--quiet",
            applied.to_str().context("worktree path")?,
            "main",
        ],
    )?;
    fixture.source = applied;
    let merged = fixture.advance()?;
    let mut backups = Vec::new();
    for _ in 0..2 {
        assert_eq!(
            fixture.synchronize(|source, backup, fixture| {
                assert_eq!(source, fixture.source);
                assert_eq!(fixture.git(source, &["rev-parse", "HEAD"])?, merged);
                assert!(backup.starts_with(fixture.home.join(".local/state/dotfiles/backups")));
                assert!(!backup.exists());
                fs::create_dir(backup)?;
                fs::copy(
                    source.join("home/dot_fixture"),
                    fixture.home.join(".fixture"),
                )?;
                backups.push(backup.to_path_buf());
                Ok(())
            })?,
            Decision::Move
        );
    }
    assert_ne!(backups[0], backups[1]);
    assert!(backups.iter().all(|backup| backup.is_dir()));
    assert_eq!(
        fs::read_to_string(fixture.home.join(".fixture"))?,
        "merged main"
    );
    assert_eq!(
        fixture.git(&fixture.source, &["status", "--porcelain"])?,
        ""
    );
    assert_eq!(fixture.report()?, "");
    Ok(())
}

#[test]
fn worktree_index_and_untracked_changes_skip_without_fetch_or_apply() -> Result<()> {
    for kind in ["worktree", "index", "untracked"] {
        let mut fixture = Fixture::new()?;
        fixture.advance()?;
        let path = if kind == "untracked" {
            "untracked"
        } else {
            "home/dot_fixture"
        };
        fs::write(fixture.source.join(path), "local work")?;
        if kind == "index" {
            fixture.git(&fixture.source, &["add", path])?;
        }
        let head = fixture.git(&fixture.source, &["rev-parse", "HEAD"])?;
        let status = fixture.git(&fixture.source, &["status", "--porcelain"])?;
        assert_eq!(
            fixture.synchronize(|_, _, _| bail!("a dirty checkout must not apply"))?,
            Decision::Skip(SkipReason::Dirty)
        );
        assert_eq!(fixture.git(&fixture.source, &["rev-parse", "HEAD"])?, head);
        assert_eq!(
            fixture.git(&fixture.source, &["status", "--porcelain"])?,
            status
        );
        assert!(
            !fixture
                .calls
                .iter()
                .any(|args| args.contains(&"fetch".into()))
        );
        assert!(fixture.report()?.contains("dirty"));
    }
    Ok(())
}

#[test]
fn another_branch_and_a_detached_head_skip_without_fetch_or_apply() -> Result<()> {
    for arguments in [
        vec!["switch", "-c", "feature"],
        vec!["checkout", "--detach"],
    ] {
        let mut fixture = Fixture::new()?;
        fixture.advance()?;
        fixture.git(&fixture.source, &arguments)?;
        let head = fixture.git(&fixture.source, &["rev-parse", "HEAD"])?;
        assert_eq!(
            fixture.synchronize(|_, _, _| bail!("off-main must not apply"))?,
            Decision::Skip(SkipReason::OffMain)
        );
        assert_eq!(fixture.git(&fixture.source, &["rev-parse", "HEAD"])?, head);
        assert!(
            !fixture
                .calls
                .iter()
                .any(|args| args.contains(&"fetch".into()))
        );
        assert!(fixture.report()?.contains("off main"));
    }
    Ok(())
}

#[test]
fn a_local_commit_that_is_ahead_or_diverged_is_preserved() -> Result<()> {
    for diverged in [false, true] {
        let mut fixture = Fixture::new()?;
        if diverged {
            fixture.advance()?;
        }
        fs::write(fixture.source.join("local"), "local commit")?;
        fixture.commit(&fixture.source)?;
        let head = fixture.git(&fixture.source, &["rev-parse", "HEAD"])?;
        assert_eq!(
            fixture.synchronize(|_, _, _| bail!("divergence must not apply"))?,
            Decision::Skip(SkipReason::Diverged)
        );
        assert_eq!(fixture.git(&fixture.source, &["rev-parse", "HEAD"])?, head);
        assert_eq!(
            fs::read_to_string(fixture.source.join("local"))?,
            "local commit"
        );
        assert!(fixture.report()?.contains("diverged"));
    }
    Ok(())
}

#[test]
fn a_failed_fetch_keeps_the_source_and_is_reported_at_session_start() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let head = fixture.git(&fixture.source, &["rev-parse", "HEAD"])?;
    let missing = fixture.home.join("missing.git");
    fixture.git(
        &fixture.source,
        &[
            "remote",
            "set-url",
            "origin",
            missing.to_str().context("missing path")?,
        ],
    )?;
    assert_eq!(
        fixture.synchronize(|_, _, _| bail!("failed fetch must not apply"))?,
        Decision::Skip(SkipReason::FetchFailed)
    );
    assert_eq!(fixture.git(&fixture.source, &["rev-parse", "HEAD"])?, head);
    assert!(fixture.report()?.contains("fetch failed"));
    assert_eq!(fixture.report()?, "");
    Ok(())
}

#[test]
fn a_failed_apply_is_reported_and_retried_with_a_new_backup() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let merged = fixture.advance()?;
    let mut failed_backup = PathBuf::new();
    let error = fixture
        .synchronize(|_, backup, _| {
            failed_backup = backup.to_path_buf();
            fs::create_dir(backup)?;
            bail!("fixture apply refused")
        })
        .unwrap_err();
    assert!(error.to_string().contains("apply"));
    assert!(failed_backup.is_dir());
    assert_eq!(
        fixture.git(&fixture.source, &["rev-parse", "HEAD"])?,
        merged
    );
    assert!(fixture.report()?.contains("fixture apply refused"));
    assert_eq!(
        fixture.synchronize(|_, backup, _| {
            assert_ne!(backup, failed_backup);
            assert!(!backup.exists());
            Ok(())
        })?,
        Decision::Move
    );
    Ok(())
}

#[test]
fn https_fetch_overrides_the_old_owner_rewrite_and_existing_ssh_origins() -> Result<()> {
    for origin in [
        "https://github.com/P4suta/dotfiles.git",
        "git@github.com:P4suta/dotfiles.git",
        "ssh://git@github.com/P4suta/dotfiles.git",
    ] {
        let mut fixture = Fixture::new()?;
        let merged = fixture.advance()?;
        fixture.git(&fixture.source, &["remote", "set-url", "origin", origin])?;
        let previous = fs::read_to_string(&fixture.global)?;
        fs::write(
            &fixture.global,
            format!(
                "{previous}[url \"git@github.com:P4suta/\"]\ninsteadOf = https://github.com/P4suta/\n"
            ),
        )?;
        fixture.https_route = Some("https://github.com/P4suta/dotfiles.git".into());
        assert_eq!(fixture.synchronize(|_, _, _| Ok(()))?, Decision::Move);
        assert_eq!(
            fixture.git(&fixture.source, &["rev-parse", "HEAD"])?,
            merged
        );
        assert_eq!(
            fixture.git(&fixture.source, &["config", "--get", "remote.origin.url"])?,
            origin
        );
    }
    Ok(())
}

#[test]
fn a_checkout_dirtied_during_fetch_is_not_moved_or_applied() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.advance()?;
    let head = fixture.git(&fixture.source, &["rev-parse", "HEAD"])?;
    fixture.fault = Fault::DirtyAfterFetch;
    assert_eq!(
        fixture.synchronize(|_, _, _| bail!("concurrent work must not apply"))?,
        Decision::Skip(SkipReason::Dirty)
    );
    assert_eq!(fixture.git(&fixture.source, &["rev-parse", "HEAD"])?, head);
    assert_eq!(
        fs::read_to_string(fixture.source.join("home/dot_fixture"))?,
        "concurrent local work"
    );
    assert!(fixture.report()?.contains("dirty"));
    Ok(())
}

#[test]
fn a_failed_fast_forward_is_reported_without_applying() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.advance()?;
    let head = fixture.git(&fixture.source, &["rev-parse", "HEAD"])?;
    fixture.fault = Fault::MergeFailed;
    assert!(
        fixture
            .synchronize(|_, _, _| bail!("failed merge must not apply"))
            .is_err()
    );
    assert_eq!(fixture.git(&fixture.source, &["rev-parse", "HEAD"])?, head);
    assert!(fixture.report()?.contains("fast-forward failed"));
    Ok(())
}

#[test]
fn a_fetch_that_cannot_start_is_a_reported_skip() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.fault = Fault::FetchCannotStart;
    assert_eq!(
        fixture.synchronize(|_, _, _| bail!("failed fetch must not apply"))?,
        Decision::Skip(SkipReason::FetchFailed)
    );
    assert!(fixture.report()?.contains("fetch failed"));
    Ok(())
}
