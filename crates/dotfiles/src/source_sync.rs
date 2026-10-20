//! The applied chezmoi source, fast-forwarded before a verified profile apply.

use crate::runtime::{Reply, Runner, args, checked};
use crate::source_sync_rules::{Checkout, Decision, Remote, SkipReason, decide, preflight};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

const MAIN: &str = "refs/heads/main";
const ORIGIN_MAIN: &str = "refs/remotes/origin/main";
const MAIN_REFSPEC: &str = "refs/heads/main:refs/remotes/origin/main";
const GITHUB_HTTPS: &str = "https://github.com/";
const SYNC_LOCK: &str = ".local/state/dotfiles/source-sync.lock";
const BACKUPS: &str = ".local/state/dotfiles/backups";
const HOOK: &str = "source-sync";

#[derive(Debug)]
struct SyncLock(File);

impl SyncLock {
    fn acquire(home: &Path) -> Result<Self> {
        let path = home.join(SYNC_LOCK);
        fs::create_dir_all(
            path.parent()
                .context("source synchronization state directory")?,
        )?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        file.try_lock()
            .context("another source upgrade holds the synchronization lock")?;
        Ok(Self(file))
    }
}

impl Drop for SyncLock {
    fn drop(&mut self) {
        platform::cleanup::report(self.0.unlock(), "unlock source synchronization");
    }
}

/// Updates the source chezmoi actually uses, then runs the verified profile apply.
/// Skips and failures reach the next session-start hook report.
///
/// # Errors
/// Returns an error when inspecting, locking, fast-forwarding, or applying the source fails.
pub fn upgrade<R: Runner>(
    config: &Path,
    home: &Path,
    runner: &mut R,
    apply: impl FnOnce(&Path, &Path, &mut R) -> Result<()>,
) -> Result<Decision> {
    let result = synchronize(config, home, runner, apply);
    let cause = match &result {
        Ok(Decision::Move) => None,
        Ok(Decision::Skip(reason)) => {
            Some(format!("source update skipped: {}", skip_reason(*reason)))
        }
        Err(error) => Some(format!("source update failed: {error:#}")),
    };
    if let Some(cause) = cause {
        eprintln!("dotfiles: {cause}");
        crate::hook_failures::record_in(home, HOOK, &cause);
    }
    result
}

fn synchronize<R: Runner>(
    config: &Path,
    home: &Path,
    runner: &mut R,
    apply: impl FnOnce(&Path, &Path, &mut R) -> Result<()>,
) -> Result<Decision> {
    let _lock = SyncLock::acquire(home)?;
    let source = applied_source(config, home, runner)?;
    if let Some(reason) = preflight(checkout(&source, runner)?) {
        return Ok(Decision::Skip(reason));
    }
    let fetched = match fetch(&source, runner) {
        Ok(fetched) => fetched,
        Err(error) => {
            eprintln!("dotfiles: source fetch failed: {error:#}");
            false
        }
    };
    if !fetched {
        return Ok(decide(checkout(&source, runner)?, Remote::FetchFailed));
    }
    if let Some(reason) = preflight(checkout(&source, runner)?) {
        return Ok(Decision::Skip(reason));
    }
    let revision = read_git(&source, runner, &["rev-parse", "--verify", ORIGIN_MAIN])?;
    let ancestor = git(
        &source,
        runner,
        &args(&["merge-base", "--is-ancestor", "HEAD", &revision]),
    )?;
    let remote = if ancestor.success {
        Remote::FastForward
    } else {
        Remote::Diverged
    };
    let decision = decide(checkout(&source, runner)?, remote);
    if decision == Decision::Move {
        let backups = home.join(BACKUPS);
        fs::create_dir_all(&backups)?;
        let backup = tempfile::Builder::new()
            .prefix("source-sync-")
            .tempdir_in(backups)?
            .keep()
            .join("profile");
        let merged = git(
            &source,
            runner,
            &args(&["merge", "--ff-only", "--no-edit", &revision]),
        )?;
        ensure!(merged.success, "source fast-forward failed");
        apply(&source, &backup, runner).context("verified source apply failed")?;
    }
    Ok(decision)
}

fn skip_reason(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Dirty => "dirty working tree or index",
        SkipReason::OffMain => "off main (another branch or detached HEAD)",
        SkipReason::Diverged => "diverged from origin/main",
        SkipReason::FetchFailed => "fetch failed",
    }
}

fn applied_source(config: &Path, home: &Path, runner: &mut impl Runner) -> Result<PathBuf> {
    let source = checked(
        runner,
        Tool::Chezmoi,
        &[
            "--config".into(),
            config.into(),
            "--destination".into(),
            home.into(),
            "source-path".into(),
        ],
    )
    .context("locate the applied chezmoi source")?;
    let source = PathBuf::from(String::from_utf8(source)?.trim());
    ensure!(source.is_absolute(), "chezmoi source-path must be absolute");
    let root = read_git(&source, runner, &["rev-parse", "--show-toplevel"])?;
    Ok(crate::canonical(Path::new(&root))?)
}

fn checkout(source: &Path, runner: &mut impl Runner) -> Result<Checkout> {
    let branch = git(source, runner, &args(&["symbolic-ref", "--quiet", "HEAD"]))?;
    let clean = read_git(
        source,
        runner,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty();
    Ok(Checkout {
        on_main: branch.success && String::from_utf8(branch.bytes)?.trim() == MAIN,
        clean,
    })
}

fn fetch(source: &Path, runner: &mut impl Runner) -> Result<bool> {
    let origin = read_git(source, runner, &["config", "--get", "remote.origin.url"])?;
    let url = if let Some(slug) = platform::source_identity::github_slug(&origin) {
        format!("{GITHUB_HTTPS}{slug}.git")
    } else {
        ensure!(
            Path::new(&origin).is_absolute(),
            "source origin must name GitHub or an absolute local repository"
        );
        origin
    };
    // This exact per-command mapping outranks the earlier installed owner-wide SSH rewrite before the first apply replaces it.
    let route = format!("url.{url}.insteadOf={url}");
    Ok(git(
        source,
        runner,
        &args(&[
            "-c",
            &route,
            "-c",
            "credential.interactive=false",
            "fetch",
            "--no-tags",
            "--no-recurse-submodules",
            &url,
            MAIN_REFSPEC,
        ]),
    )?
    .success)
}

fn git(source: &Path, runner: &mut impl Runner, arguments: &[OsString]) -> Result<Reply> {
    let mut invocation = vec!["-C".into(), source.into()];
    invocation.extend_from_slice(arguments);
    runner.run(Tool::Git, &invocation)
}

fn read_git(source: &Path, runner: &mut impl Runner, arguments: &[&str]) -> Result<String> {
    let reply = git(source, runner, &args(arguments))?;
    ensure!(reply.success, "source git {} failed", arguments.join(" "));
    Ok(String::from_utf8(reply.bytes)?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_refuses_overlap_and_releases_even_with_a_duplicated_descriptor() -> Result<()> {
        let home = tempfile::tempdir()?;
        let lock = SyncLock::acquire(home.path())?;
        assert!(SyncLock::acquire(home.path()).is_err());
        let duplicate = lock.0.try_clone()?;
        drop(lock);
        let next = SyncLock::acquire(home.path())?;
        drop(duplicate);
        drop(next);
        Ok(())
    }
}
