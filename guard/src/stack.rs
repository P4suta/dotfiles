//! The repository facts the published-rewrite rules read, and the lease through which the stack tooling runs them.
//!
//! `pr-workflow stack` writes a random token to `dotguard-stack.lease` in the repository's common Git directory and exports the same token as `DOTGUARD_STACK` to the `gh-stack` process it runs.
//! The Git commands that `gh-stack` runs inherit the token, so the wrapper and the pre-push gate admit its rebases and leased pushes, and nothing else.

use crate::gate_rules;
use crate::gitargv::Repository;
use crate::realgit;
use std::path::Path;

/// The variable through which the stack tooling hands its lease token to the Git commands it runs.
pub const TOKEN: &str = "DOTGUARD_STACK";
/// The lease file the stack tooling writes in the repository's common Git directory while it runs.
pub const LEASE: &str = "dotguard-stack.lease";

/// The global options that choose the repository, so the gate reads the repository the command acts on.
fn locating(globals: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut words = globals.iter();
    while let Some(word) = words.next() {
        if matches!(word.as_str(), "-C" | "--git-dir" | "--work-tree") {
            kept.push(word.clone());
            kept.extend(words.next().cloned());
        } else if word.starts_with("--git-dir=") || word.starts_with("--work-tree=") {
            kept.push(word.clone());
        }
    }
    kept
}

fn git(globals: &[String], arguments: &[&str]) -> Option<String> {
    let located = locating(globals);
    let words: Vec<&str> = located
        .iter()
        .map(String::as_str)
        .chain(arguments.iter().copied())
        .collect();
    realgit::capture(&words).map(|output| output.trim().to_owned())
}

/// Whether the stack tooling holds the lease of the repository `globals` select.
pub fn leased(globals: &[String]) -> bool {
    let token = std::env::var_os(TOKEN).map(std::ffi::OsString::into_encoded_bytes);
    let lease = git(
        globals,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .and_then(|directory| std::fs::read(Path::new(&directory).join(LEASE)).ok());
    gate_rules::stack_tooling(token.as_deref(), lease.as_deref())
}

/// The `OWNER/REPO` a GitHub remote URL names.
pub fn github_slug(url: &str) -> Option<String> {
    let path = [
        "https://github.com/",
        "ssh://git@github.com/",
        "git@github.com:",
    ]
    .iter()
    .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repository) = path.split_once('/')?;
    (!owner.is_empty() && !repository.is_empty() && !repository.contains('/'))
        .then(|| format!("{owner}/{repository}"))
}

/// The checkout the wrapped command runs in, read through the real Git.
pub struct Checkout;

impl Repository for Checkout {
    fn slug(&self, globals: &[String]) -> Option<String> {
        github_slug(&git(globals, &["config", "--get", "remote.origin.url"])?)
    }

    fn current_branch(&self, globals: &[String]) -> Option<String> {
        git(globals, &["symbolic-ref", "--quiet", "--short", "HEAD"])
    }

    fn rewrites_published(
        &self,
        globals: &[String],
        branch: &str,
        upstream: Option<&str>,
    ) -> Option<bool> {
        let tips: Vec<String> = git(globals, &["remote"])?
            .lines()
            .filter_map(|remote| {
                git(
                    globals,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("refs/remotes/{remote}/{branch}^{{commit}}"),
                    ],
                )
            })
            .collect();
        if tips.is_empty() {
            return Some(false);
        }
        let head = git(
            globals,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}^{{commit}}"),
            ],
        )?;
        let base = match upstream {
            Some(upstream) => Some(git(
                globals,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{upstream}^{{commit}}"),
                ],
            )?),
            None => None,
        };
        for tip in tips {
            // Unrelated histories share no commit to rewrite.
            let Some(shared) = git(globals, &["merge-base", "--all", &head, &tip]) else {
                continue;
            };
            let mut arguments = vec!["rev-list", "-n", "1"];
            arguments.extend(shared.lines());
            if let Some(base) = &base {
                arguments.extend(["--not", base]);
            }
            if !git(globals, &arguments)?.is_empty() {
                return Some(true);
            }
        }
        Some(false)
    }
}

#[cfg(test)]
mod tests {
    use super::{github_slug, locating};

    #[test]
    fn a_github_remote_names_its_repository() {
        for url in [
            "https://github.com/P4suta/dotfiles.git",
            "https://github.com/P4suta/dotfiles",
            "git@github.com:P4suta/dotfiles.git",
            "ssh://git@github.com/P4suta/dotfiles",
        ] {
            assert_eq!(
                github_slug(url).as_deref(),
                Some("P4suta/dotfiles"),
                "{url}"
            );
        }
        for url in [
            "https://gitlab.com/P4suta/dotfiles.git",
            "https://github.com/P4suta",
            "https://github.com/P4suta/dotfiles/tree/main",
        ] {
            assert_eq!(github_slug(url), None, "{url}");
        }
    }

    #[test]
    fn only_the_options_that_choose_the_repository_are_kept() {
        let words = |line: &str| {
            line.split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            locating(&words(
                "-c a=b -C repo --no-pager --git-dir=x --work-tree w"
            )),
            words("-C repo --git-dir=x --work-tree w")
        );
    }
}
