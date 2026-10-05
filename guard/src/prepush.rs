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
use crate::gate_rules::{self, Admission, History, Reference, Signing};
use crate::realgit;
use crate::refusal::{Refusal, command};
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

/// The pushed refs that could carry new commits, as `(local ref, sha)`: pushes only, not deletions, and not the empty line a here-string over an empty variable still produces.
/// An "Everything up-to-date" push sends nothing at all on stdin, and `read` still succeeds with every field empty — without this filter the signing gate below would reach `git rev-list ""`, which fails, which refuses a push that pushes nothing.
fn pushed(input: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in input.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [local_ref, local_sha, _remote_ref, _remote_sha] = f[..] else {
            continue;
        };
        if local_sha.is_empty() || local_sha.chars().all(|c| c == '0') {
            continue;
        }
        out.push(((*local_ref).to_owned(), (*local_sha).to_owned()));
    }
    out
}

/// The unsigned commits one pushed ref would publish.
struct Unsigned {
    local_ref: String,
    tip: String,
    /// `(sha, summary)` in topological order, newest first.
    commits: Vec<(String, String)>,
    /// The pushed range contains a merge, which a rebase would flatten.
    merges: bool,
    /// The parent of the oldest unsigned commit, absent for a root commit.
    base: Option<String>,
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

    let not_remote = format!("--remotes={remote}");
    let mut refs: Vec<Unsigned> = Vec::new();
    for (local_ref, sha) in pushed(input) {
        // Verify what this push introduces: commits reachable from the new tip that no ref on this remote already carries.
        // Deliberately the same range for a new branch and for a force-push — the narrower-looking `remote..local` is wider after a rebase: it re-verifies the base branch's history, which the remote already accepted under its own ruleset, and that history can be unverifiable here (a forge signs the commits it writes with its own scheme; GitHub uses PGP where this machine signs with SSH).
        let Some(range) =
            realgit::capture(&["rev-list", "--topo-order", &sha, "--not", &not_remote])
        else {
            continue;
        };
        let commits: Vec<(String, String)> = range
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter(|commit| !realgit::succeeds(&["verify-commit", commit]))
            .map(|commit| {
                let summary = realgit::capture(&[
                    "log",
                    "-1",
                    "--pretty=format:  %h %s%n  author: %an <%ae>",
                    commit,
                ])
                .map_or_else(|| format!("  {commit}"), |s| s.trim().to_owned());
                (commit.to_owned(), summary)
            })
            .collect();
        let Some((oldest, _)) = commits.last() else {
            continue;
        };
        let merges = realgit::capture(&[
            "rev-list",
            "--merges",
            "--count",
            &sha,
            "--not",
            &not_remote,
        ])
        .is_none_or(|count| count.trim() != "0");
        let base = realgit::capture(&["rev-parse", "--verify", "--quiet", &format!("{oldest}^")])
            .map(|parent| parent.trim().to_owned());
        refs.push(Unsigned {
            local_ref,
            tip: sha,
            commits,
            merges,
            base,
        });
    }

    if refs.is_empty() {
        return 0;
    }
    let head = realgit::capture(&["rev-parse", "HEAD"]).map(|head| head.trim().to_owned());
    let head_ref = realgit::capture(&["symbolic-ref", "-q", "HEAD"])
        .map(|reference| reference.trim().to_owned());
    unsigned_refusal(
        remote,
        &refs,
        head.as_deref()
            .map(|sha| (head_ref.as_deref().unwrap_or("HEAD"), sha)),
    )
    .emit();
    1
}

/// Signs exactly the unsigned commits when one command can, and lists them otherwise.
/// `head` is the ref `HEAD` points at, or `HEAD` itself when detached, and the commit it names.
fn unsigned_refusal(remote: &str, refs: &[Unsigned], head: Option<(&str, &str)>) -> Refusal {
    let first = &refs[0];
    let branch = first.local_ref.strip_prefix("refs/heads/");
    let reference = if first.local_ref == "HEAD" {
        Reference::Head
    } else if head.is_some_and(|(reference, _)| first.local_ref == reference) {
        Reference::CheckedOutBranch
    } else if branch.is_some() {
        Reference::Branch
    } else {
        Reference::Other
    };
    let only_tip =
        matches!(&first.commits[..], [(only, _)] if head.is_some_and(|(_, sha)| sha == only));
    let sign = "git commit --amend --no-edit -S";
    let next = match gate_rules::signing(refs.len(), reference, first.merges, only_tip) {
        Signing::Amend => "git commit --amend -S --no-edit".to_owned(),
        Signing::Rebase => command(&[
            "git",
            "rebase",
            "--exec",
            sign,
            first.base.as_deref().unwrap_or("--root"),
            branch.unwrap_or_default(),
        ]),
        Signing::Inspect => command(&[
            "git",
            "log",
            "--format=%h %G? %s",
            &first.tip,
            "--not",
            &format!("--remotes={remote}"),
        ]),
    };
    let count: usize = refs.iter().map(|unsigned| unsigned.commits.len()).sum();
    let mut refusal = Refusal::new(
        "push.signature",
        format!(
            "{count} commit(s) in this push would be unsigned.\n\
             Sign them and push again; the local Git wrapper does not allow skipping verification."
        ),
        next,
    );
    for unsigned in refs {
        for (_, summary) in &unsigned.commits {
            refusal = refusal.evidence(format!("{}: {summary}", unsigned.local_ref));
        }
    }
    refusal
}

/// One ref update the history gate refuses.
struct Problem {
    verdict: Verdict,
    local_ref: String,
    remote_ref: String,
    detail: String,
}

/// The branch name a ref update names, for commands that take a branch.
fn branch(reference: &str) -> &str {
    reference.strip_prefix("refs/heads/").unwrap_or(reference)
}

/// Deletions go through the forge, a stale clone fetches first, and a rewrite integrates the remote history instead.
/// The waiver repeats exactly the refused updates.
fn history_refusal(remote: &str, problems: &[Problem]) -> Refusal {
    let rewrite = |p: &&Problem| matches!(p.verdict, Verdict::Rewrite | Verdict::Undecidable);
    let next = match gate_rules::history(
        problems
            .iter()
            .any(|p| matches!(p.verdict, Verdict::Undecidable)),
        problems
            .iter()
            .any(|p| matches!(p.verdict, Verdict::Rewrite)),
    ) {
        History::Fetch => command(&["git", "fetch", remote]),
        History::Integrate => {
            let problem = problems.iter().find(rewrite).unwrap_or(&problems[0]);
            command(&[
                "git",
                "pull",
                "--rebase",
                remote,
                branch(&problem.remote_ref),
            ])
        }
        History::ForgeDelete => problems
            .iter()
            .map(|p| {
                command(&[
                    "gh",
                    "api",
                    "-X",
                    "DELETE",
                    &format!("repos/{{owner}}/{{repo}}/git/{}", p.remote_ref),
                ])
            })
            .collect::<Vec<_>>()
            .join(" && "),
    };
    let mut waiver = vec!["git".to_owned(), "push".to_owned()];
    if problems.iter().any(|p| rewrite(&p)) {
        waiver.push("--force-with-lease".to_owned());
    }
    waiver.push(remote.to_owned());
    waiver.extend(problems.iter().map(|p| match p.verdict {
        Verdict::Delete => format!(":{}", p.remote_ref),
        _ => format!("{}:{}", p.local_ref, p.remote_ref),
    }));
    let mut refusal = Refusal::new(
        "push.history",
        "refusing to push a change that destroys published history.\n\
         Force-pushing a shared branch is how other people's work disappears: their clone\n\
         still points at commits the remote no longer has, and the next `git pull` quietly\n\
         merges the two histories back together.\n\
         --force-with-lease refuses if someone else pushed since your last fetch, which is\n\
         the only part of this that plain --force gives up. The waiver is recorded in\n\
         ~/.local/state/git-bypass.log.",
        next,
    )
    .waiver(format!("ALLOW_FORCE=1 {}", command(&waiver)));
    for problem in problems {
        refusal = refusal.evidence(problem.detail.clone());
    }
    refusal
}

fn judge(remote: &str, input: &str) -> i32 {
    let mut problems: Vec<Problem> = Vec::new();

    for line in input.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [local_ref, local_sha, remote_ref, remote_sha] = f[..] else {
            continue;
        };

        let verdict = classify(local_sha, remote_sha);
        let detail = match verdict {
            Verdict::Ok => continue,
            Verdict::Delete => format!(
                "{remote_ref} on '{remote}' would be DELETED\n  (nothing on this machine can restore a ref the remote no longer has)"
            ),
            Verdict::Rewrite => {
                let dropped = realgit::capture(&[
                    "rev-list",
                    "--count",
                    &format!("{local_sha}..{remote_sha}"),
                ])
                .map_or_else(|| "some".to_owned(), |s| s.trim().to_owned());
                format!(
                    "{remote_ref} on '{remote}' would be REWRITTEN\n  \
                     {remote_sha:.12} is not an ancestor of {local_sha:.12} ({local_ref})\n  \
                     {dropped} commit(s) currently on the remote would stop being reachable"
                )
            }
            Verdict::Undecidable => format!(
                "{remote_ref} on '{remote}' cannot be checked\n  \
                 {remote_sha:.12} is not an object in this clone"
            ),
        };
        problems.push(Problem {
            verdict,
            local_ref: local_ref.to_owned(),
            remote_ref: remote_ref.to_owned(),
            detail,
        });
    }

    if problems.is_empty() {
        return 0;
    }

    let argv: Vec<String> = std::env::args().collect();
    // The stack tooling rewrites branches and pushes them with explicit leases; it never deletes a ref.
    let rewrites = problems
        .iter()
        .all(|p| !matches!(p.verdict, Verdict::Delete));
    match gate_rules::admission(
        rewrites,
        rewrites && crate::stack::leased(&[]),
        bypass::waived(Category::Force),
    ) {
        Admission::Tooling => {
            bypass::record("STACK", Category::Force, "pre-push non-fast-forward", &argv);
            return 0;
        }
        Admission::Waived => {
            bypass::record(
                "BYPASS",
                Category::Force,
                "pre-push non-fast-forward",
                &argv,
            );
            let summary: Vec<&str> = problems.iter().map(|p| p.detail.as_str()).collect();
            eprintln!(
                "::warning:: ALLOW_FORCE=1 — pushing a rewrite:\n{}",
                summary.join("\n")
            );
            return 0;
        }
        Admission::Refused => {}
    }

    bypass::record(
        "REJECT",
        Category::Force,
        "pre-push non-fast-forward",
        &argv,
    );
    history_refusal(remote, &problems).emit();
    1
}

#[cfg(test)]
mod tests {
    use super::{Problem, Unsigned, Verdict, classify, history_refusal, pushed, unsigned_refusal};

    fn problem(verdict: Verdict, name: &str) -> Problem {
        Problem {
            verdict,
            local_ref: format!("refs/heads/{name}"),
            remote_ref: format!("refs/heads/{name}"),
            detail: format!("refs/heads/{name} on 'origin'"),
        }
    }

    #[test]
    fn a_history_refusal_names_the_command_for_its_worst_case_and_waives_exactly_the_refused_updates()
     {
        let refused = |problems: &[Problem]| {
            let refusal = history_refusal("origin", problems);
            assert!(refusal.is_complete(), "{refusal:?}");
            assert_eq!(refusal.rule, "push.history");
            (refusal.next.unwrap(), refusal.waiver.unwrap())
        };
        assert_eq!(
            refused(&[problem(Verdict::Delete, "a"), problem(Verdict::Delete, "b")]),
            (
                "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/a' && gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/b'".to_owned(),
                "ALLOW_FORCE=1 git push origin :refs/heads/a :refs/heads/b".to_owned()
            )
        );
        assert_eq!(
            refused(&[problem(Verdict::Delete, "a"), problem(Verdict::Rewrite, "b")]),
            (
                "git pull --rebase origin b".to_owned(),
                "ALLOW_FORCE=1 git push --force-with-lease origin :refs/heads/a refs/heads/b:refs/heads/b".to_owned()
            )
        );
        assert_eq!(
            refused(&[
                problem(Verdict::Rewrite, "a"),
                problem(Verdict::Undecidable, "b")
            ])
            .0,
            "git fetch origin"
        );
    }

    fn unsigned(name: &str, commits: &[&str], merges: bool, base: Option<&str>) -> Unsigned {
        Unsigned {
            local_ref: name.to_owned(),
            tip: commits[0].to_owned(),
            commits: commits
                .iter()
                .map(|sha| ((*sha).to_owned(), format!("  {sha} summary")))
                .collect(),
            merges,
            base: base.map(str::to_owned),
        }
    }

    #[test]
    fn an_unsigned_push_refusal_signs_exactly_the_unsigned_range_or_lists_it() {
        let topic = "refs/heads/topic";
        let inspect = "git log '--format=%h %G? %s' bbb --not --remotes=origin";
        let on = |reference: &'static str, sha: &'static str| Some((reference, sha));
        for (refs, head, next) in [
            (
                vec![unsigned(topic, &["aaa"], false, Some("base"))],
                on(topic, "aaa"),
                "git commit --amend -S --no-edit",
            ),
            (
                vec![unsigned("HEAD", &["aaa"], false, Some("base"))],
                on("refs/heads/main", "aaa"),
                "git commit --amend -S --no-edit",
            ),
            // An amend moves only the checked-out branch, so another ref at the same commit is signed where it points.
            (
                vec![unsigned(topic, &["aaa"], false, Some("base"))],
                on("refs/heads/main", "aaa"),
                "git rebase --exec 'git commit --amend --no-edit -S' base topic",
            ),
            (
                vec![unsigned("refs/tags/v1", &["bbb"], false, Some("base"))],
                on("refs/heads/main", "bbb"),
                inspect,
            ),
            (
                vec![unsigned(topic, &["aaa"], false, Some("base"))],
                on(topic, "ccc"),
                "git rebase --exec 'git commit --amend --no-edit -S' base topic",
            ),
            (
                vec![unsigned(topic, &["bbb", "aaa"], false, Some("base"))],
                on(topic, "bbb"),
                "git rebase --exec 'git commit --amend --no-edit -S' base topic",
            ),
            (
                vec![unsigned(topic, &["bbb", "aaa"], false, None)],
                on(topic, "bbb"),
                "git rebase --exec 'git commit --amend --no-edit -S' --root topic",
            ),
            (
                vec![unsigned(topic, &["bbb", "aaa"], true, Some("base"))],
                on(topic, "bbb"),
                inspect,
            ),
            (
                vec![
                    unsigned(topic, &["bbb"], false, Some("base")),
                    unsigned("refs/heads/other", &["ccc"], false, Some("base")),
                ],
                on(topic, "bbb"),
                inspect,
            ),
            (
                vec![unsigned(
                    "refs/tags/v1",
                    &["bbb", "aaa"],
                    false,
                    Some("base"),
                )],
                on(topic, "ccc"),
                inspect,
            ),
        ] {
            let refusal = unsigned_refusal("origin", &refs, head);
            assert!(refusal.is_complete(), "{refusal:?}");
            assert_eq!(refusal.next.as_deref(), Some(next));
            assert_eq!(refusal.waiver, None);
            assert_eq!(
                refusal.evidence.len(),
                refs.iter().map(|r| r.commits.len()).sum::<usize>()
            );
        }
    }

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
        assert_eq!(
            pushed(&input),
            vec![("refs/heads/y".to_owned(), "deadbee".to_owned())]
        );
    }
}
