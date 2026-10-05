//! The pre-push gate that keeps a ready PR finished: its branch takes no push until the PR returns to draft.

use crate::hosts_rules::gated;
use crate::ready_push_rules::{Pr, Verdict, push};
use crate::refusal::{Refusal, command};
use crate::tool::Tool;
use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The `owner/name` of a GitHub remote URL, or `None` for any other remote.
pub fn repository(url: &str) -> Option<String> {
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    (!owner.is_empty() && !name.is_empty() && !name.contains('/'))
        .then(|| format!("{owner}/{name}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    number: u64,
    is_draft: bool,
}

/// The open PR whose head is `branch` in `repository`, as `gh` reports it; a failed query is `Unknown`.
pub fn lookup(repository: &str, branch: &str) -> (Pr, Option<u64>) {
    let output = Tool::Gh
        .command()
        .env("GH_HOST", "github.com")
        .args([
            "pr",
            "list",
            "--repo",
            repository,
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "number,isDraft",
        ])
        .output();
    let listed: Option<Vec<Listed>> = output
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| serde_json::from_slice(&output.stdout).ok());
    match listed.as_deref() {
        None => (Pr::Unknown, None),
        Some([]) => (Pr::Absent, None),
        Some(prs) => match prs.iter().find(|pr| !pr.is_draft) {
            Some(ready) => (Pr::Ready, Some(ready.number)),
            None => (Pr::Draft, Some(prs[0].number)),
        },
    }
}

/// Judges pre-push input, `<local ref> <local sha> <remote ref> <remote sha>` per line, with `lookup` answering for each pushed branch.
pub fn gate_with(
    url: &str,
    input: &[u8],
    lookup: impl Fn(&str, &str) -> (Pr, Option<u64>),
) -> Result<()> {
    let repository = repository(url);
    for line in std::str::from_utf8(input)?.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let [_, local, remote_ref, _] = fields[..] else {
            bail!("malformed pre-push input: {line}");
        };
        let branch = remote_ref.strip_prefix("refs/heads/");
        let deletion = local.bytes().all(|byte| byte == b'0');
        let (Some(repository), Some(branch)) = (repository.as_deref(), branch) else {
            continue;
        };
        if !gated(true, true, deletion) {
            continue;
        }
        let (pr, number) = lookup(repository, branch);
        match push(true, pr) {
            Verdict::Admit => {}
            Verdict::ReturnToDraft => {
                let number = number.context("a ready PR has a number")?.to_string();
                return Err(Refusal::new(
                    "push.ready",
                    format!(
                        "#{number} is ready for review, so its branch takes no push until it returns to draft; after the push, pr-workflow ready returns it once every check passes"
                    ),
                    command(&["gh", "pr", "ready", &number, "--repo", repository, "--undo"]),
                )
                .evidence(format!(
                    "{repository} #{number} has the head {branch} and is not a draft"
                ))
                .into());
            }
            Verdict::Inspect => {
                return Err(Refusal::new(
                    "push.ready",
                    "GitHub could not report whether the pushed branch has a ready PR; establish its state before pushing",
                    command(&[
                        "gh", "pr", "list", "--repo", repository, "--head", branch, "--state",
                        "open",
                    ]),
                )
                .evidence(format!("gh pr list --head {branch} failed or returned no list"))
                .into());
            }
        }
    }
    Ok(())
}

/// The gate as the global pre-push hook runs it, asking GitHub for each pushed branch.
pub fn gate(url: &str, input: &[u8]) -> Result<()> {
    gate_with(url, input, lookup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_remote_urls_name_their_repository() {
        for url in [
            "git@github.com:P4suta/dotfiles.git",
            "https://github.com/P4suta/dotfiles",
            "https://github.com/P4suta/dotfiles.git",
            "ssh://git@github.com/P4suta/dotfiles.git",
        ] {
            assert_eq!(repository(url).as_deref(), Some("P4suta/dotfiles"), "{url}");
        }
        for url in [
            "ssh://hub/~/git/dotfiles.git",
            "/srv/git/dotfiles.git",
            "https://github.com/P4suta",
        ] {
            assert_eq!(repository(url), None, "{url}");
        }
    }
}
