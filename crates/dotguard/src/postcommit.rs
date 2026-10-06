//! Forces every commit on this machine to end up signed.
//!
//! The hook re-amends each unsigned commit with an explicit signature, even after `commit.gpgsign=false`, `--no-gpg-sign`, or `gpg.format=null`.
//!
//! When signing fails, it rolls the commit back and leaves the changes staged, so no unsigned commit sits on `HEAD`.
//!
//! The hook disables itself on a machine without signing configured, so a fresh Mac without the 1Password SSH agent keeps its commits.

use crate::bypass::{self, Category};
use crate::realgit;
use crate::refusal::Refusal;

/// The lock this hook sets around its own `--amend`, so the amended commit skips the hook.
const LOCK_ENV: &str = "GIT_HOOK_FORCE_SIGN_LOCK";

/// Files in `.git` that mark a rebase, merge, cherry-pick, revert, or bisect in progress, which an amend would corrupt.
const MID_OPERATION: [&str; 6] = [
    "rebase-merge",
    "rebase-apply",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
];

pub fn run() -> i32 {
    let in_hook = hook_is_reentrant();
    let mid_operation = mid_operation();
    if refuses_empty(
        !in_hook && !mid_operation,
        head_is_empty(),
        bypass::waived(Category::Empty),
    ) {
        return refuse_empty();
    }
    if !signing_configured() {
        return 0;
    }
    let situation = Situation {
        in_hook,
        mid_operation,
        head_signed: head_signed(),
    };
    match decide(&situation) {
        Decision::Done => 0,
        Decision::Resign => resign(),
    }
}

struct Situation {
    in_hook: bool,
    mid_operation: bool,
    head_signed: bool,
}

/// The action of the hook, decided from the cheap facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Nothing to do: unconfigured, re-entrant, mid-operation, already signed, or no `HEAD`.
    Done,
    /// `HEAD` lacks a signature, and this hook owns the fix.
    Resign,
}

fn decide(s: &Situation) -> Decision {
    if s.in_hook || s.mid_operation || s.head_signed {
        Decision::Done
    } else {
        Decision::Resign
    }
}

/// Refuses an ordinary commit that records no change unless waived, because its changes most likely vanished from the index.
fn refuses_empty(settled: bool, empty: bool, waived: bool) -> bool {
    settled && empty && !waived
}

/// `HEAD` has one parent and the same tree, so merges and the root commit never count as empty.
fn head_is_empty() -> bool {
    let Some(parents) = realgit::capture(&["rev-list", "--parents", "-n", "1", "HEAD"]) else {
        return false;
    };
    if parents.split_whitespace().count() != 2 {
        return false;
    }
    let tree = |rev: &str| realgit::capture(&["rev-parse", rev]).map(|t| t.trim().to_owned());
    matches!((tree("HEAD^{tree}"), tree("HEAD^^{tree}")), (Some(a), Some(b)) if a == b)
}

fn refuse_empty() -> i32 {
    let short = realgit::capture(&["rev-parse", "--short", "HEAD"])
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
    let rolled_back = realgit::succeeds(&["reset", "--soft", "HEAD^"]);
    empty_refusal(&short, rolled_back).emit();
    1
}

/// Once rolled back, the waiver records the same empty commit again.
/// While the commit still sits at the checked-out tip, nothing remains to waive.
fn empty_refusal(short: &str, rolled_back: bool) -> Refusal {
    if rolled_back {
        Refusal::new(
            "commit.empty",
            format!(
                "commit {short} records no change from its parent.\n\
                 The staged changes most likely vanished before Git recorded the commit, so it was rolled back.\n\
                 Your working tree is unchanged; stage the changes again and commit."
            ),
            "git status --short",
        )
        .evidence(format!("HEAD {short} has the same tree as HEAD^"))
        .evidence("rolled back with git reset --soft HEAD^")
        .waiver(format!(
            "ALLOW_EMPTY=1 git commit --allow-empty --reuse-message={short}"
        ))
    } else {
        Refusal::new(
            "commit.empty",
            format!("commit {short} records no change and could not be rolled back."),
            "git reset --soft HEAD^",
        )
        .evidence(format!("HEAD {short} has the same tree as HEAD^"))
        .evidence("git reset --soft HEAD^ failed")
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
    // The command's output goes unread because the refusal below already names the reason the agent can't sign.
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

    let target = rollback(&pre_head);
    let head = realgit::capture(&["rev-parse", "--short", "HEAD"]).map(|s| s.trim().to_owned());
    unsigned_refusal(&pre_head, target, head.as_deref()).emit();
    1
}

/// Signing failed and the gate undid the commit.
/// The staged changes and the reflog still hold everything needed to commit again.
fn unsigned_refusal(pre_head: &str, target: Rollback, head: Option<&str>) -> Refusal {
    let rolled_back = match (target, head) {
        (Rollback::SoftToParent, Some(head)) => {
            format!("rolled back to {head}; the changes remain staged")
        }
        (Rollback::SoftToParent, None) => {
            "rolled back to the parent; the changes remain staged".to_owned()
        }
        (Rollback::DropHead, _) => {
            "dropped the root commit; the changes remain staged with no HEAD".to_owned()
        }
    };
    Refusal::new(
        "commit.signature",
        "signing failed, so the unsigned commit was not left on HEAD.\n\
         Likely cause: 1Password is locked or its SSH agent confirmation timed out.\n\
         Other causes: ssh-keygen is missing, the signing key is not loaded, or gpg.ssh.program is misconfigured.\n\
         Unlock 1Password or restart the SSH agent, then commit again with the same message.",
        format!("git commit --reuse-message={pre_head}"),
    )
    .evidence(format!("unsigned commit {pre_head} is kept in the reflog"))
    .evidence(rolled_back)
}

/// Undoes the unsigned commit by stepping back to its parent, or by deleting the ref for a root commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rollback {
    /// `git reset --soft <parent>`, which keeps the changes staged.
    SoftToParent,
    /// `git update-ref -d HEAD`, which leaves the repository without a `HEAD`.
    DropHead,
}

fn rollback_target(parent_exists: bool) -> Rollback {
    if parent_exists {
        Rollback::SoftToParent
    } else {
        Rollback::DropHead
    }
}

fn rollback(pre_head: &str) -> Rollback {
    // Confirm `HEAD` before stepping further, in case a partial amend moved it.
    let _ = realgit::command().and_then(|mut c| {
        c.args(["reset", "--soft", pre_head])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
    });

    let parent_exists = realgit::succeeds(&["rev-parse", "--verify", &format!("{pre_head}^")]);
    let target = rollback_target(parent_exists);
    match target {
        Rollback::SoftToParent => {
            let _ = realgit::succeeds(&["reset", "--soft", &format!("{pre_head}^")]);
        }
        Rollback::DropHead => {
            let _ = realgit::succeeds(&["update-ref", "-d", "HEAD"]);
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::{
        Decision, Rollback, Situation, decide, empty_refusal, refuses_empty, rollback_target,
        unsigned_refusal,
    };

    #[test]
    fn every_refusal_is_a_complete_record() {
        for refusal in [
            empty_refusal("abc1234", true),
            empty_refusal("abc1234", false),
            unsigned_refusal("abc1234", Rollback::SoftToParent, Some("def5678")),
            unsigned_refusal("abc1234", Rollback::SoftToParent, None),
            unsigned_refusal("abc1234", Rollback::DropHead, None),
        ] {
            assert!(refusal.is_complete(), "{refusal:?}");
        }
        assert_eq!(empty_refusal("a", true).rule, "commit.empty");
        assert_eq!(
            empty_refusal("abc1234", true).waiver.as_deref(),
            Some("ALLOW_EMPTY=1 git commit --allow-empty --reuse-message=abc1234")
        );
        let kept = empty_refusal("abc1234", false);
        assert_eq!(kept.next.as_deref(), Some("git reset --soft HEAD^"));
        assert_eq!(kept.waiver, None, "the empty commit is still on HEAD");
        let unsigned = unsigned_refusal("abc1234", Rollback::DropHead, None);
        assert_eq!(
            unsigned.next.as_deref(),
            Some("git commit --reuse-message=abc1234")
        );
        assert_eq!(unsigned.waiver, None);
    }

    #[test]
    fn an_empty_commit_is_refused_unless_waived_or_git_is_mid_operation() {
        assert!(refuses_empty(true, true, false));
        assert!(!refuses_empty(true, false, false));
        assert!(!refuses_empty(true, true, true));
        assert!(
            !refuses_empty(false, true, false),
            "re-entrant or mid-operation: Git is not done"
        );
    }

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
