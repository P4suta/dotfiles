//! The git hooks seeded into every repo through `init.templateDir`.
//!
//! Defense in depth around commit signing, paired with a GitHub ruleset that requires signed commits server-side.
//! That ruleset is the actual guarantee;
//! these are best-effort and local.
//!
//! post-commit re-signs an unsigned HEAD and rolls back if it cannot.
//! pre-push refuses a range that still holds an unsigned commit, which catches what post-commit could not see: history merged in from another machine.

use std::io::BufRead;
use std::process::Command;

use anyhow::Result;

use crate::{env, proc};

/// Set while this hook is the one running `--amend`, so the amend's own post-commit run returns immediately instead of looping.
const LOCK: &str = "GIT_HOOK_FORCE_SIGN_LOCK";

/// Files whose presence means git is mid-operation.
/// Amending under any of them corrupts the operation in progress, so the hook stands down.
const IN_PROGRESS: &[&str] = &[
    "rebase-merge",
    "rebase-apply",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
];

fn git() -> Command {
    env::command("git")
}

/// True when `git <args>` exits 0; output is discarded.
fn git_ok(args: &[&str]) -> bool {
    git()
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn git_line(args: &[&str]) -> Option<String> {
    let out = proc::capture(git().args(args)).ok()?;
    if !out.ok() {
        return None;
    }
    let line = out.stdout_text().trim().to_owned();
    (!line.is_empty()).then_some(line)
}

/// Re-signs an unsigned HEAD, or rolls the commit back to a staged state.
///
/// Even `git -c commit.gpgsign=false commit` or `--no-gpg-sign` ends up signed, because this runs after the fact.
/// git ignores a post-commit exit code, so the rollback, not the exit status, is what enforces the rule.
pub fn post_commit() -> Result<i32> {
    if std::env::var_os(LOCK).is_some() {
        return Ok(0);
    }

    let Some(git_dir) = git_line(&["rev-parse", "--git-dir"]) else {
        return Ok(0);
    };
    let git_dir = std::path::Path::new(&git_dir);
    if IN_PROGRESS.iter().any(|name| git_dir.join(name).exists()) {
        return Ok(0);
    }

    if git_ok(&["verify-commit", "HEAD"]) {
        return Ok(0);
    }

    // Captured before the amend so a failure can roll back to its parent.
    let Some(pre_head) = git_line(&["rev-parse", "HEAD"]) else {
        return Ok(0);
    };

    if resign() {
        let short = git_line(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
        eprintln!("::notice:: HEAD was unsigned; re-signed automatically: {short}");
        return Ok(0);
    }

    eprintln!();
    eprintln!("::error:: signing failed - refusing to leave an unsigned commit on HEAD.");
    eprintln!();
    eprintln!("Likely cause: 1Password is locked, or its SSH agent confirm dialog");
    eprintln!("timed out. Other causes: ssh-keygen missing, signing key not loaded,");
    eprintln!("or gpg.ssh.program misconfigured.");
    eprintln!();

    // A partial amend is unlikely to leave HEAD elsewhere, but be defensive.
    let _ = git_ok(&["reset", "--soft", &pre_head]);

    let parent = format!("{pre_head}^");
    if git_ok(&["rev-parse", "--verify", &parent]) {
        // Step back one commit; the staged tree keeps the user's changes.
        let _ = git_ok(&["reset", "--soft", &parent]);
        let short = git_line(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
        eprintln!("::notice:: rolled back to {short}; your changes remain staged.");
    } else {
        // The root commit: drop the ref so the repo behaves as if the commit never happened.
        // The index is untouched.
        let _ = git_ok(&["update-ref", "-d", "HEAD"]);
        eprintln!(
            "::notice:: rolled back the root commit; your changes remain staged with no HEAD."
        );
    }

    eprintln!();
    eprintln!("To recover:");
    eprintln!("  1. Unlock 1Password (or restart the SSH agent)");
    eprintln!("  2. Re-run your original commit; the staged diff is unchanged");
    eprintln!("  3. The reflog still holds the unsigned commit: git reflog");
    Ok(1)
}

fn resign() -> bool {
    let signed = git()
        .env(LOCK, "1")
        .args([
            "-c",
            "commit.gpgsign=true",
            "-c",
            "tag.gpgsign=true",
            "commit",
            "--amend",
            "--no-edit",
            "--gpg-sign",
            "--no-verify",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    signed && git_ok(&["verify-commit", "HEAD"])
}

/// Refuses a push whose range still contains an unsigned commit.
///
/// git feeds the hook one `<local ref> <local sha> <remote ref> <remote sha>` line per ref on stdin.
pub fn pre_push(remote: &str) -> Result<i32> {
    let mut code = 0;

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [_local_ref, local_sha, _remote_ref, remote_sha] = fields[..] else {
            continue;
        };

        // A deletion has nothing to verify on the local side.
        if is_zero(local_sha) {
            continue;
        }

        let remote_known = !is_zero(remote_sha)
            && git_ok(&["cat-file", "-e", &format!("{remote_sha}^{{commit}}")]);
        let range = if !remote_known {
            // New branch, or a remote tip this repository has never fetched: only commits that no remote ref already holds, so history already pushed is not re-verified.
            git_lines(&[
                "rev-list",
                local_sha,
                "--not",
                &format!("--remotes={remote}"),
            ])
        } else {
            git_lines(&["rev-list", &format!("{remote_sha}..{local_sha}")])
        };
        let Some(range) = range else {
            eprintln!(
                "::error:: could not determine which commits this push sends for {local_sha}"
            );
            code = 1;
            continue;
        };

        for commit in range {
            if git_ok(&["verify-commit", &commit]) {
                continue;
            }
            eprintln!("::error:: unsigned commit cannot be pushed: {commit}");
            if let Some(details) = git_line(&[
                "log",
                "-1",
                "--pretty=format:  %h %s%n  author: %an <%ae>",
                &commit,
            ]) {
                eprintln!("{details}");
            }
            code = 1;
        }
    }

    if code != 0 {
        eprintln!();
        eprintln!("::notice:: push refused. To sign these commits:");
        eprintln!();
        eprintln!("  # last commit only");
        eprintln!("  git commit --amend -S --no-edit");
        eprintln!();
        eprintln!("  # rewrite a range");
        eprintln!("  git rebase --exec 'git commit --amend --no-edit -S' <base>");
        eprintln!();
        eprintln!(
            "Signing and hook verification cannot be waived; configure the signing key instead."
        );
    }
    Ok(code)
}

/// True for git's all-zero sha, at either sha1 or sha256 length.
fn is_zero(sha: &str) -> bool {
    !sha.is_empty() && sha.bytes().all(|b| b == b'0')
}

/// `None` when git could not answer, which a gate must not read as "nothing to check".
fn git_lines(args: &[&str]) -> Option<Vec<String>> {
    let out = proc::capture(git().args(args)).ok()?;
    if !out.ok() {
        return None;
    }
    Some(
        out.stdout_text()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
    )
}
