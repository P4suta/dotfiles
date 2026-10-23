use super::{GITHUB, Github, LiveIssue, Refusal, branch, command, gh, references_issue};
use anyhow::{Context, Result, ensure};
use rules::pr_rules::{IssueLinkDecision, issue_link, issue_reference_gap};
use serde::Deserialize;
use std::fmt::Write as _;
use std::num::NonZeroU64;

const COMMIT_HEX_DIGITS: usize = 40;

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct Account {
    id: u64,
    login: String,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct Repository {
    id: u64,
    full_name: String,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct Reference {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
    repo: Option<Repository>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct Pull {
    number: u64,
    html_url: String,
    title: String,
    body: Option<String>,
    state: String,
    draft: bool,
    user: Account,
    head: Reference,
    base: Reference,
    labels: Vec<serde_json::Value>,
    requested_reviewers: Vec<serde_json::Value>,
    requested_teams: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Branch {
    name: String,
    commit: Commit,
}

#[derive(Deserialize)]
struct Commit {
    sha: String,
}

fn read_pull(github: &Github, repo: &str, pr: NonZeroU64) -> Result<Pull> {
    let pull: Pull = github.read(&format!("repos/{repo}/pulls/{pr}"))?;
    ensure!(
        pull.number == pr.get()
            && pull
                .html_url
                .eq_ignore_ascii_case(&format!("{GITHUB}/{repo}/pull/{pr}"))
            && pull.user.id != 0
            && super::single_line(&pull.user.login),
        "pull request identity differs from the selected destination"
    );
    Ok(pull)
}

fn read_issue(github: &Github, repo: &str, issue: NonZeroU64) -> Result<LiveIssue> {
    let selected: LiveIssue = github.read(&format!("repos/{repo}/issues/{issue}"))?;
    ensure!(
        selected.number == issue.get()
            && selected
                .html_url
                .eq_ignore_ascii_case(&format!("{GITHUB}/{repo}/issues/{issue}"))
            && selected.pull_request.is_none(),
        "issue identity differs from the selected destination"
    );
    Ok(selected)
}

fn source(github: &Github, repo: &str, pull: &Pull) -> Result<platform::source_identity::Binding> {
    let head = pull
        .head
        .repo
        .as_ref()
        .context("PR source repository missing")?;
    let base = pull
        .base
        .repo
        .as_ref()
        .context("PR destination repository missing")?;
    let binding =
        platform::source_identity::bind(&head.full_name, repo, |name| github.repository(name))?;
    ensure!(
        head.id == binding.source_id()
            && base.id == binding.target_id()
            && base.full_name.eq_ignore_ascii_case(binding.target()),
        "PR repositories differ from their verified network identities"
    );
    Ok(binding)
}

fn current_heads(
    github: &Github,
    binding: &platform::source_identity::Binding,
    pull: &Pull,
) -> Result<()> {
    for (repo, reference) in [
        (binding.source(), &pull.head),
        (binding.target(), &pull.base),
    ] {
        branch(&reference.name)?;
        ensure!(
            reference.sha.len() == COMMIT_HEX_DIGITS
                && reference.sha.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "PR head or base is not a full commit identity"
        );
        let current: Branch = github.read(&format!(
            "repos/{repo}/branches/{}",
            super::review_policy::path_segment(&reference.name)?
        ))?;
        ensure!(
            current.name == reference.name && current.commit.sha == reference.sha,
            "PR source or base branch changed during issue-link inspection"
        );
    }
    Ok(())
}

fn admission(
    decision: IssueLinkDecision,
    repo: &str,
    pr: NonZeroU64,
    issue: NonZeroU64,
) -> Result<bool> {
    let (rule, cause) = match decision {
        IssueLinkDecision::Append => return Ok(true),
        IssueLinkDecision::Unchanged => return Ok(false),
        IssueLinkDecision::NotOwner => (
            "pr.owner",
            "only the authenticated numeric repository owner may link issues",
        ),
        IssueLinkDecision::ClosedPull => (
            "pr.issue-link",
            "the selected pull request must remain open",
        ),
        IssueLinkDecision::ClosedIssue => ("pr.issue-link", "the selected issue must remain open"),
    };
    Err(Refusal::new(
        rule,
        cause,
        command(&["gh", "pr", "view", &pr.to_string(), "--repo", repo]),
    )
    .evidence(format!("{repo} PR #{pr}, issue #{issue}: {decision:?}"))
    .into())
}

fn appended(body: &str, issue: NonZeroU64) -> Result<String> {
    let mut result = body.to_owned();
    result.push_str(issue_reference_gap(
        body.is_empty(),
        body.ends_with('\n'),
        body.ends_with("\n\n") || body.ends_with("\n\r\n"),
    ));
    writeln!(result, "Closes #{issue}.")?;
    Ok(result)
}

fn write(github: &Github, repo: &str, pr: NonZeroU64, expected: &Pull) -> Result<()> {
    let mut input = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(
        input.as_file_mut(),
        &serde_json::json!({"body":expected.body}),
    )?;
    input.as_file().sync_all()?;
    let args = [
        "api".into(),
        format!("repos/{repo}/pulls/{pr}").into(),
        "--hostname".into(),
        "github.com".into(),
        "--include".into(),
        "--method".into(),
        "PATCH".into(),
        "--input".into(),
        input.path().as_os_str().to_owned(),
    ];
    let response = gh(&args).and_then(|response| github.response::<Pull>(&response, "core"));
    match response {
        Ok(acknowledged) if acknowledged == *expected => Ok(()),
        other => Err(Refusal::new(
            "pr.link-write",
            "the issue-link write has no verified result; inspect the PR before another write",
            command(&["gh", "pr", "view", &pr.to_string(), "--repo", repo]),
        )
        .evidence(match other {
            Ok(_) => {
                "server acknowledgement differs from the expected body, head, or metadata".into()
            }
            Err(error) => format!("{error:#}"),
        })
        .into()),
    }
}

pub(super) fn run(repo: &str, pr: NonZeroU64, issue: NonZeroU64) -> Result<()> {
    ensure!(
        platform::source_identity::github_slug(&format!("{GITHUB}/{repo}")).as_deref()
            == Some(repo),
        "--repo must identify one exact GitHub repository"
    );
    let github = Github::connect()?;
    let repository = github.repository(repo)?;
    let before = read_pull(&github, repo, pr)?;
    let selected = read_issue(&github, repo, issue)?;
    let append = admission(
        issue_link(
            github.user_id,
            repository.owner.id,
            before.state == "open",
            selected.state == "open",
            references_issue(
                before.body.as_deref().unwrap_or_default(),
                repo,
                issue.get(),
            ),
        ),
        repo,
        pr,
        issue,
    )?;
    let binding = source(&github, repo, &before)?;
    current_heads(&github, &binding, &before)?;
    if !append {
        println!("PR #{pr} already closes issue #{issue}; its document is unchanged");
        return Ok(());
    }
    let mut expected = before.clone();
    expected.body = Some(appended(before.body.as_deref().unwrap_or_default(), issue)?);
    let selected = read_issue(&github, repo, issue)?;
    ensure!(
        selected.state == "open",
        "the selected issue changed before the write"
    );
    ensure!(
        read_pull(&github, repo, pr)? == before,
        "the PR changed before the issue-link write"
    );
    current_heads(&github, &binding, &before)?;
    write(&github, repo, pr, &expected)?;
    ensure!(
        read_pull(&github, repo, pr)? == expected,
        "the PR differs after the issue-link write; inspect it before another write"
    );
    println!("Linked issue #{issue} to PR #{pr} in {repo}");
    Ok(())
}
