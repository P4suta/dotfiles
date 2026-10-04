//! The force-push gate that cannot be walked around.
//!
//! git hands a pre-push hook one line per ref on stdin:
//!
//! ```text
//! <local ref> <local sha> <remote ref> <remote sha>
//! ```
//!
//! and, crucially, does NOT say whether `--force` was passed.
//! That turns out not to matter: what `--force` actually buys is permission to move a remote ref to a commit that is not a descendant of where it is now.
//! So the question "is this a force push?"
//! is answerable from the shas alone — is `remote_sha` an ancestor of `local_sha`?
//! — and the answer is the same whether the rewrite came from `--force`, `--force-with-lease`, a `+refspec`, an IDE button, or a GUI client that never looked at PATH.
//!
//! That is why this, and not the `~/.local/bin/git` wrapper, is the actual guarantee.
//! The wrapper only sees commands that resolved `git` through PATH.
//! This hook is reached through `core.hooksPath` for every repository on the machine and every invocation of git in it.

use crate::bypass::{self, Category};
use crate::realgit;
use std::io::Read;

enum Verdict {
    Ok,
    Delete,
    Rewrite,
    /// The remote-tracking commit is not in this clone, so ancestry is not decidable.
    /// Treated as a rewrite: an undecidable push is exactly the shape a rewrite has after someone else force-pushed.
    Undecidable,
}

fn classify(local_sha: &str, remote_sha: &str) -> Verdict {
    if local_sha.chars().all(|c| c == '0') {
        return Verdict::Delete;
    }
    if remote_sha.chars().all(|c| c == '0') {
        return Verdict::Ok; // a branch that does not exist on the remote yet
    }
    if !realgit::succeeds(&["cat-file", "-e", &format!("{remote_sha}^{{commit}}")]) {
        return Verdict::Undecidable;
    }
    if realgit::succeeds(&["merge-base", "--is-ancestor", remote_sha, local_sha]) {
        Verdict::Ok
    } else {
        Verdict::Rewrite
    }
}

/// Returns the process exit code: 0 to let the push through, 1 to refuse it.
///
/// Two gates, in the order in which being wrong is cheapest to discover: history first (force/deletion), then signatures.
/// There is no point paying for a `git verify-commit` per commit on a push that is going to be refused anyway.
pub fn run(remote: &str) -> i32 {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return 0; // nothing readable means nothing to judge
    }
    let verdict = judge(remote, &input);
    if verdict != 0 {
        return verdict;
    }
    signatures(remote, &input)
}

/// The shas that could carry new commits: pushes only, not deletions, and not the empty line a here-string over an empty variable still produces.
/// An "Everything up-to-date" push sends nothing at all on stdin, and `read` still succeeds with every field empty — without this filter the signing gate below would reach `git rev-list ""`, which fails, which refuses a push that pushes nothing.
fn pushed_shas(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in input.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [_local_ref, local_sha, _remote_ref, _remote_sha] = f[..] else {
            continue;
        };
        if local_sha.is_empty() || local_sha.chars().all(|c| c == '0') {
            continue;
        }
        out.push((*local_sha).to_owned());
    }
    out
}

/// Refuse a push that would introduce an unsigned commit.
///
/// The backstop for branches carried in from another machine or merged with mixed signing state — `post-commit` re-signs what *this* machine commits, this gate is what catches everything else before it leaves.
/// Best-effort alongside GitHub's "Require signed commits" ruleset, which remains the server-side guarantee.
///
/// Verification is skipped entirely when signing is not configured on this machine (`~/.gitconfig` omits the block while the 1Password SSH agent is unavailable): enforcing here would refuse every push on a fresh Mac, which is the failure mode graceful degradation exists to prevent.
fn signatures(remote: &str, input: &str) -> i32 {
    let configured = realgit::capture(&["config", "--get", "commit.gpgsign"])
        .is_some_and(|v| v.trim() == "true");
    if !configured {
        eprintln!(
            "::warning:: commit signing is not configured; skipping the unsigned-commit gate"
        );
        return 0;
    }

    let mut unsigned: Vec<(String, String)> = Vec::new(); // (sha, summary)
    for sha in pushed_shas(input) {
        // Verify what this push introduces: commits reachable from the new tip that no ref on this remote already carries.
        // Deliberately the same range for a new branch and for a force-push — the narrower-looking `remote..local` is wider after a rebase: it re-verifies the base branch's history, which the remote already accepted under its own ruleset, and that history can be unverifiable here (a forge signs the commits it writes with its own scheme; GitHub uses PGP where this machine signs with SSH).
        let Some(range) =
            realgit::capture(&["rev-list", &sha, "--not", &format!("--remotes={remote}")])
        else {
            continue;
        };
        for commit in range.lines().filter(|l| !l.trim().is_empty()) {
            if !realgit::succeeds(&["verify-commit", commit]) {
                let summary = realgit::capture(&[
                    "log",
                    "-1",
                    "--pretty=format:  %h %s%n  author: %an <%ae>",
                    commit,
                ])
                .map_or_else(|| format!("  {commit}"), |s| s.trim().to_owned());
                unsigned.push((commit.to_owned(), summary));
            }
        }
    }

    if unsigned.is_empty() {
        return 0;
    }

    eprintln!(
        "::error:: {} commit(s) in this push would be unsigned:\n",
        unsigned.len()
    );
    for (_, summary) in &unsigned {
        eprintln!("{summary}");
    }
    eprintln!(
        "\n::notice:: push refused. To sign these commits:\n\n  \
         # last commit only\n  \
         git commit --amend -S --no-edit\n\n  \
         # rewrite a range\n  \
         git rebase --exec 'git commit --amend --no-edit -S' <base>\n\n\
         The local Git wrapper does not allow skipping verification."
    );
    1
}

fn judge(remote: &str, input: &str) -> i32 {
    let mut refused: Vec<String> = Vec::new();

    for line in input.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [local_ref, local_sha, remote_ref, remote_sha] = f[..] else {
            continue;
        };

        match classify(local_sha, remote_sha) {
            Verdict::Ok => {}
            Verdict::Delete => refused.push(format!(
                "  {remote_ref} on '{remote}' would be DELETED\n    (nothing on this machine can restore a ref the remote no longer has)"
            )),
            Verdict::Rewrite => {
                let dropped = realgit::capture(&["rev-list", "--count", &format!("{local_sha}..{remote_sha}")])
                    .map_or_else(|| "some".to_owned(), |s| s.trim().to_owned());
                refused.push(format!(
                    "  {remote_ref} on '{remote}' would be REWRITTEN\n    \
                     {remote_sha:.12} is not an ancestor of {local_sha:.12} ({local_ref})\n    \
                     {dropped} commit(s) currently on the remote would stop being reachable"
                ));
            }
            Verdict::Undecidable => refused.push(format!(
                "  {remote_ref} on '{remote}' cannot be checked\n    \
                 {remote_sha:.12} is not an object in this clone; run `git fetch {remote}` and try again"
            )),
        }
    }

    if refused.is_empty() {
        return 0;
    }

    let summary = refused.join("\n");
    let argv: Vec<String> = std::env::args().collect();

    if bypass::waived(Category::Force) {
        bypass::record(
            "BYPASS",
            Category::Force,
            "pre-push non-fast-forward",
            &argv,
        );
        eprintln!("::warning:: ALLOW_FORCE=1 — pushing a rewrite:\n{summary}");
        return 0;
    }

    bypass::record(
        "REJECT",
        Category::Force,
        "pre-push non-fast-forward",
        &argv,
    );
    eprintln!(
        "::error:: refusing to push a change that destroys published history.\n\n{summary}\n\n\
         Force-pushing a shared branch is how other people's work disappears: their clone\n\
         still points at commits the remote no longer has, and the next `git pull` quietly\n\
         merges the two histories back together.\n\n\
         If the branch is yours alone and the rewrite is deliberate:\n\n\
           ALLOW_FORCE=1 git push --force-with-lease\n\n\
         --force-with-lease refuses if someone else pushed since your last fetch, which is\n\
         the only part of this that plain --force gives up. The bypass is recorded in\n\
         ~/.local/state/git-bypass.log."
    );
    1
}

#[cfg(test)]
mod tests {
    use super::{Verdict, classify, pushed_shas};

    const ZERO: &str = "0000000000000000000000000000000000000000";

    #[test]
    fn deleting_a_ref_is_refused() {
        assert!(matches!(classify(ZERO, "abc123"), Verdict::Delete));
    }

    #[test]
    fn a_brand_new_branch_is_fine() {
        assert!(matches!(classify("abc123", ZERO), Verdict::Ok));
    }

    #[test]
    fn only_real_pushes_carry_commits() {
        // A ref deletion, a nothing-to-push line, and one real push.
        let input = format!(
            "refs/heads/main {ZERO} refs/heads/main abc123\n\
             refs/heads/x {ZERO} refs/heads/x {ZERO}\n\
             refs/heads/y deadbee refs/heads/y abc123\n"
        );
        assert_eq!(pushed_shas(&input), vec!["deadbee"]);
    }
}
