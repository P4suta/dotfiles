//! The force-push gate.
//!
//! git hands a pre-push hook one line per ref on stdin:
//!
//! ```text
//! <local ref> <local sha> <remote ref> <remote sha>
//! ```
//!
//! The line omits whether `--force` applies, and the gate needs no such flag.
//! A force push moves a remote ref to a commit outside its descendants, so the gate checks whether `local_sha` descends from `remote_sha`.
//! The answer holds for `--force`, `--force-with-lease`, a `+refspec`, and any graphical client.
//!
//! This hook, and not the `~/.local/bin/git` wrapper, enforces the rule, because the wrapper only handles commands that resolve `git` through `PATH`.
//! `core.hooksPath` runs this hook for every repository and every git invocation on the machine.

use crate::bypass::{self, Category};
use crate::realgit;
use std::io::Read;

enum Verdict {
    Ok,
    Delete,
    Rewrite,
    /// This clone lacks the remote-tracking commit, so ancestry stays unknown.
    /// The gate treats it as a rewrite, the shape a push takes after someone else force-pushed.
    Undecidable,
}

fn classify(local_sha: &str, remote_sha: &str) -> Verdict {
    if local_sha.chars().all(|c| c == '0') {
        return Verdict::Delete;
    }
    if remote_sha.chars().all(|c| c == '0') {
        return Verdict::Ok; // A branch new to the remote.
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
/// The history gate for force pushes and deletions runs before the signature gate, so a refused push skips the per-commit `git verify-commit`.
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

/// The shas that could carry new commits: pushes only, without deletions or the empty line from a here-string over an empty variable.
/// An up-to-date push sends an empty stdin, and without this filter the signing gate would run `git rev-list ""` and refuse the push.
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
/// The backstop for branches from another machine or with mixed signing state, because `post-commit` re-signs only the commits of this machine.
/// The GitHub ruleset `Require signed commits` enforces the same rule on the server.
///
/// Without signing configured on this machine, the gate skips verification, so a fresh Mac without the 1Password key service can still push.
fn signatures(remote: &str, input: &str) -> i32 {
    let configured = realgit::capture(&["config", "--get", "commit.gpgsign"])
        .is_some_and(|v| v.trim() == "true");
    if !configured {
        eprintln!(
            "::warning:: commit signing is not configured; skipping the unsigned-commit gate"
        );
        return 0;
    }

    let mut unsigned: Vec<(String, String)> = Vec::new(); // sha and summary
    for sha in pushed_shas(input) {
        // Verify what this push introduces: commits reachable from the new tip that no ref on this remote already carries.
        // A new branch and a force push use the same range.
        // After a rebase, `remote..local` would re-verify base history that a forge signed with its own key.
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
