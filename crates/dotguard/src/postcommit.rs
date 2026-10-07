//! Forces every commit on this machine to end up signed.
//!
//! The hook re-amends each unsigned commit with an explicit signature, even after `commit.gpgsign=false`, `--no-gpg-sign`, or `gpg.format=null`.
//!
//! When signing fails, it rolls the commit back and leaves the changes staged, so no unsigned commit sits on `HEAD`.
//! `postcommit_rules` decides where the rollback leaves `HEAD`: the parent, the commit an amend replaced, or no `HEAD` after a root commit, and an unsigned merge keeps its second parent as `MERGE_HEAD`.
//!
//! The hook disables itself on a machine without signing configured, so a fresh Mac without the 1Password SSH agent keeps its commits.

use crate::bypass::{self, Category};
use crate::postcommit_rules::{
    self, Decision, Gate, Outcome, Shape, Step, Target, decide, refuses_empty,
};
use crate::realgit;
use crate::refusal::Refusal;

/// The lock this hook sets around its own `--amend`, so the amended commit skips the hook.
const LOCK_ENV: &str = "GIT_HOOK_FORCE_SIGN_LOCK";

/// The value the hook's own amend writes into the lock, which separates its re-entry from a caller that placed the lock.
const OWN_AMEND: &str = "dotguard-amend";

/// The hash digits a notice shows when `git rev-parse --short` can't.
const SHORT_HASH: usize = 12;

/// The reflog subject Git records for `git commit --amend`.
const AMEND_SUBJECT: &str = "commit (amend)";

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
    let lock = sign_lock();
    let in_hook = lock.is_some();
    if let Some(value) = lock {
        record_sign_lock(&value);
    }
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
    match decide(in_hook, mid_operation, head_signed()) {
        Decision::Done => 0,
        Decision::Resign => resign(),
    }
}

/// The value of the sign lock, when a caller or this hook's own amend set it.
fn sign_lock() -> Option<String> {
    std::env::var(LOCK_ENV)
        .ok()
        .filter(|value| !value.is_empty())
}

/// The lock skips re-signing and the empty-commit refusal, so every skip lands in the bypass log.
fn record_sign_lock(value: &str) {
    let reason = if value == OWN_AMEND {
        "the hook's own signed amend"
    } else {
        "the caller set GIT_HOOK_FORCE_SIGN_LOCK"
    };
    let head = realgit::capture(&["rev-parse", "HEAD"])
        .map(|head| head.trim().to_owned())
        .unwrap_or_default();
    bypass::record(
        "skipped",
        Category::SignLock,
        reason,
        &[format!("post-commit HEAD={head}")],
    );
}

/// `HEAD` has one parent and the same tree, so merges and the root commit never count as empty.
fn head_is_empty() -> bool {
    let Some(parents) = realgit::capture(&["rev-list", "--parents", "-n", "1", "HEAD"]) else {
        return false;
    };
    // `rev-list --parents` prints the commit and then its parents, so one parent gives two words.
    let [_, _] = parents.split_whitespace().collect::<Vec<_>>()[..] else {
        return false;
    };
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

/// The commit Git just recorded and the facts its rollback needs, read before the hook's own amend moves `HEAD`.
struct Recorded {
    head: String,
    shape: Shape,
    /// The commit an amend replaced, which the reflog names as `HEAD@{1}`.
    original: Option<String>,
}

fn recorded() -> Option<Recorded> {
    let head = realgit::capture(&["rev-parse", "HEAD"])?.trim().to_owned();
    let parents = realgit::capture(&["rev-list", "--parents", "-n", "1", &head])
        .map_or(0, |line| line.split_whitespace().count().saturating_sub(1));
    let amended = realgit::capture(&["log", "-g", "-1", "--format=%gs", "HEAD"])
        .is_some_and(|subject| subject.starts_with(AMEND_SUBJECT));
    let original = amended
        .then(|| realgit::capture(&["rev-parse", "--verify", "HEAD@{1}"]))
        .flatten()
        .map(|original| original.trim().to_owned());
    Some(Recorded {
        shape: postcommit_rules::shape(parents, original.is_some()),
        head,
        original,
    })
}

fn resign() -> i32 {
    let Some(recorded) = recorded() else {
        return 0; // nothing to amend
    };

    // The re-sign itself, under the lock.
    // The command's output goes unread because the refusal below already names the reason the agent can't sign.
    let Some(mut amend) = realgit::command() else {
        return 0;
    };
    let amended = amend
        .env(LOCK_ENV, OWN_AMEND)
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
    let outcome = match (amended, amended && head_signed()) {
        (_, true) => Outcome::Signed,
        (true, false) => Outcome::Unverified,
        (false, _) => Outcome::Failed,
    };
    let rollback = postcommit_rules::rollback_target(Gate::Fixed, recorded.shape, outcome);
    if rollback.target == Target::Keep {
        let short = realgit::capture(&["rev-parse", "--short", "HEAD"]).map_or_else(
            || recorded.head.chars().take(SHORT_HASH).collect(),
            |s| s.trim().to_owned(),
        );
        eprintln!("::notice:: HEAD was unsigned; re-signed automatically: {short}");
        return 0;
    }

    for step in rollback.steps() {
        if !roll_back(*step, &recorded) {
            eprintln!(
                "warning: the rollback step {step:?} failed for {}; inspect `git log -1` before retrying",
                recorded.head
            );
        }
    }
    let head = realgit::capture(&["rev-parse", "--short", "HEAD"]).map(|s| s.trim().to_owned());
    unsigned_refusal(
        &recorded.head,
        rollback.target,
        recorded.shape,
        head.as_deref(),
    )
    .emit();
    1
}

/// Runs one rollback step, each naming an absolute commit, so a rerun after a crash ends in the same place.
fn roll_back(step: Step, recorded: &Recorded) -> bool {
    let head = recorded.head.as_str();
    match step {
        Step::ResetToCommit => realgit::succeeds(&["reset", "--soft", head]),
        Step::ResetToFirstParent => realgit::succeeds(&["reset", "--soft", &format!("{head}^")]),
        Step::ResetToOriginal => recorded
            .original
            .as_deref()
            .is_some_and(|original| realgit::succeeds(&["reset", "--soft", original])),
        Step::DropHead => realgit::succeeds(&["update-ref", "-d", "HEAD"]),
        Step::RestoreMergeParent => restore_merge_parent(head),
    }
}

/// Git removes `MERGE_HEAD` before the hook runs, so the hook writes the second parent back and the next commit counts as a merge again.
fn restore_merge_parent(head: &str) -> bool {
    let (Some(second), Some(path)) = (
        realgit::capture(&["rev-parse", "--verify", &format!("{head}^2")]),
        realgit::capture(&["rev-parse", "--git-path", "MERGE_HEAD"]),
    ) else {
        return false;
    };
    std::fs::write(path.trim(), format!("{}\n", second.trim())).is_ok()
}

/// Signing failed and the gate undid the commit.
/// The staged changes and the reflog still hold everything needed to commit again.
fn unsigned_refusal(pre_head: &str, target: Target, shape: Shape, head: Option<&str>) -> Refusal {
    let head = head.unwrap_or("the commit before it");
    let rolled_back = match target {
        Target::Unborn => {
            "dropped the root commit; the changes remain staged with no HEAD".to_owned()
        }
        Target::Original => {
            format!("restored {head}, the commit the amend replaced; the changes remain staged")
        }
        Target::FirstParent | Target::Keep if shape == Shape::Merge => format!(
            "rolled back to {head} with the second parent kept as MERGE_HEAD; the changes remain staged"
        ),
        Target::FirstParent | Target::Keep => {
            format!("rolled back to {head}; the changes remain staged")
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

#[cfg(test)]
mod tests {
    use super::{Shape, Target, empty_refusal, unsigned_refusal};

    #[test]
    fn every_refusal_is_a_complete_record() {
        for refusal in [
            empty_refusal("abc1234", true),
            empty_refusal("abc1234", false),
            unsigned_refusal(
                "abc1234",
                Target::FirstParent,
                Shape::Single,
                Some("def5678"),
            ),
            unsigned_refusal("abc1234", Target::FirstParent, Shape::Merge, None),
            unsigned_refusal("abc1234", Target::Original, Shape::Amend, Some("def5678")),
            unsigned_refusal("abc1234", Target::Unborn, Shape::Root, None),
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
        let unsigned = unsigned_refusal("abc1234", Target::Unborn, Shape::Root, None);
        assert_eq!(
            unsigned.next.as_deref(),
            Some("git commit --reuse-message=abc1234")
        );
        assert_eq!(unsigned.waiver, None);
    }
}
