//! Force every commit on this machine to end up signed.
//!
//! Even if the operator ran `git -c commit.gpgsign=false commit`, or `git commit --no-gpg-sign`, or `git -c gpg.format=null commit`, this hook re-amends the resulting commit with an explicit signature.
//!
//! Failure mode (1Password locked / SSH agent timeout / signer missing): roll the unsigned commit back so the working state lands as "staged but not committed" — never as "unsigned commit sitting on HEAD".
//! The user's diff is preserved while a poisoned state is refused to an overnight agent push.
//! Paired with a GitHub repository ruleset enforcing `Require signed commits` server-side; that is the actual guarantee and this is best-effort defense-in-depth.
//!
//! Reached through `core.hooksPath` for every repository, which is also why it self-disables when signing is not configured on this machine: a fresh Mac whose 1Password agent is not yet enabled would otherwise roll back every commit it makes.

use crate::realgit;

/// The lock this hook sets around its own `--amend`, so the resulting commit does not re-enter it.
const LOCK_ENV: &str = "GIT_HOOK_FORCE_SIGN_LOCK";

/// Files in `.git` whose presence means git itself is mid-operation; amending now would corrupt the in-progress rebase / merge / cherry-pick / revert / bisect, so signing is left to git.
const MID_OPERATION: [&str; 6] = [
    "rebase-merge",
    "rebase-apply",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
];

/// Returns the process exit code.
pub fn run() -> i32 {
    if !signing_configured() {
        return 0;
    }
    let situation = Situation {
        in_hook: hook_is_reentrant(),
        mid_operation: mid_operation(),
        head_signed: head_signed(),
    };
    match decide(&situation) {
        Decision::Done => 0,
        Decision::Resign => resign(),
    }
}

/// The cheap facts the decision rests on.
struct Situation {
    in_hook: bool,
    mid_operation: bool,
    head_signed: bool,
}

/// What the hook should do once the cheap facts are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to do: unconfigured, re-entrant, mid-operation, already signed, or no HEAD at all.
    Done,
    /// HEAD is unsigned and this hook is the one that should fix that.
    Resign,
}

fn decide(s: &Situation) -> Decision {
    if s.in_hook || s.mid_operation || s.head_signed {
        Decision::Done
    } else {
        Decision::Resign
    }
}

fn signing_configured() -> bool {
    realgit::capture(&["config", "--get", "commit.gpgsign"]).is_some_and(|v| v.trim() == "true")
}

fn hook_is_reentrant() -> bool {
    std::env::var_os(LOCK_ENV).is_some_and(|v| !v.is_empty())
}

fn mid_operation() -> bool {
    let Some(git_dir) = realgit::capture(&["rev-parse", "--git-dir"]) else {
        return false; // not a repository: nothing to guard
    };
    let git_dir = git_dir.trim();
    MID_OPERATION
        .iter()
        .any(|f| std::path::Path::new(git_dir).join(f).exists())
}

fn head_signed() -> bool {
    realgit::succeeds(&["verify-commit", "HEAD"])
}

fn resign() -> i32 {
    let Some(pre_head) = realgit::capture(&["rev-parse", "HEAD"]) else {
        return 0; // nothing to amend
    };
    let pre_head = pre_head.trim().to_owned();

    // The re-sign itself, under the lock.
    // Output is swallowed: on failure every word of it is about *why* the agent is unreachable, which the rollback message below already names.
    let Some(mut amend) = realgit::command() else {
        return 0;
    };
    let amended = amend
        .env(LOCK_ENV, "1")
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
        .is_ok_and(|s| s.success());

    if amended && realgit::succeeds(&["verify-commit", "HEAD"]) {
        let short = realgit::capture(&["rev-parse", "--short", "HEAD"]).map_or_else(
            || pre_head[..12.min(pre_head.len())].to_owned(),
            |s| s.trim().to_owned(),
        );
        eprintln!("::notice:: HEAD was unsigned; re-signed automatically: {short}");
        return 0;
    }

    eprintln!(
        "::error:: signing failed — refusing to leave an unsigned commit on HEAD.\n\n\
         Likely cause: 1Password is locked / its SSH agent confirm dialog\n\
         timed out. Other causes: ssh-keygen missing, signing key not loaded,\n\
         or gpg.ssh.program misconfigured."
    );
    rollback(&pre_head)
}

/// How to undo the unsigned commit: step back to its parent when there is one, or drop the ref entirely when it was the root — the index keeps the user's changes either way, and the reflog keeps the commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rollback {
    /// `git reset --soft <parent>` — staged tree retains the changes.
    SoftToParent,
    /// `git update-ref -d HEAD` — the repo behaves as if the commit was never made, with no HEAD to hang anything from.
    DropHead,
}

fn rollback_target(parent_exists: bool) -> Rollback {
    if parent_exists {
        Rollback::SoftToParent
    } else {
        Rollback::DropHead
    }
}

fn rollback(pre_head: &str) -> i32 {
    // Defensive: a partial amend is unlikely to leave HEAD elsewhere, but be sure before stepping further.
    let _ = realgit::command().and_then(|mut c| {
        c.args(["reset", "--soft", pre_head])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
    });

    let parent_exists = realgit::succeeds(&["rev-parse", "--verify", &format!("{pre_head}^")]);
    match rollback_target(parent_exists) {
        Rollback::SoftToParent => {
            let _ = realgit::succeeds(&["reset", "--soft", &format!("{pre_head}^")]);
            let short = realgit::capture(&["rev-parse", "--short", "HEAD"])
                .map(|s| s.trim().to_owned())
                .unwrap_or_default();
            eprintln!(
                "::notice:: rolled back to {short}; your changes remain staged.\n\n\
                 To recover:\n  \
                 1. Unlock 1Password (or restart the SSH agent)\n  \
                 2. Re-run your original commit; the staged diff is unchanged\n  \
                 3. The reflog still contains the unsigned commit if you need to inspect it:\n  \
                 git reflog"
            );
        }
        Rollback::DropHead => {
            let _ = realgit::succeeds(&["update-ref", "-d", "HEAD"]);
            eprintln!(
                "::notice:: rolled back the root commit; your changes remain staged with no HEAD.\n\n\
                 To recover:\n  \
                 1. Unlock 1Password (or restart the SSH agent)\n  \
                 2. Re-run your original commit; the staged diff is unchanged\n  \
                 3. The reflog still contains the unsigned commit if you need to inspect it:\n  \
                 git reflog"
            );
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::{Decision, Rollback, Situation, decide, rollback_target};

    #[test]
    fn nothing_happens_without_a_reason_to_act() {
        let done = |f: fn(&mut Situation)| {
            let mut s = Situation {
                in_hook: false,
                mid_operation: false,
                head_signed: false,
            };
            f(&mut s);
            decide(&s)
        };
        assert_eq!(
            done(|s| s.in_hook = true),
            Decision::Done,
            "re-entrant: the amend this hook makes must not re-enter it"
        );
        assert_eq!(
            done(|s| s.mid_operation = true),
            Decision::Done,
            "mid-operation: amending would corrupt the in-progress rebase"
        );
        assert_eq!(
            done(|s| s.head_signed = true),
            Decision::Done,
            "already signed: nothing to do"
        );
        assert_eq!(
            decide(&Situation {
                in_hook: false,
                mid_operation: false,
                head_signed: false
            }),
            Decision::Resign,
            "otherwise: fix it"
        );
    }

    #[test]
    fn the_root_commit_drops_the_ref_instead_of_stepping_back() {
        assert_eq!(rollback_target(true), Rollback::SoftToParent);
        assert_eq!(rollback_target(false), Rollback::DropHead);
    }
}
