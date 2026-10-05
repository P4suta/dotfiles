use crate::pr_rules::{
    Blocker, Checks, Dependency, Effect, Exclusion, Generation, IssueGate, Lifecycle, MergeState,
    Operation, ReviewRequirement, Step, Stop, api_quota_available, checks_state,
    coderabbit_body_marker_allowed, dependency, exclusion, head_checks, issue_gate,
    keeps_pause_exclusion, merge_step, plan, requests_review, review_request_allowed,
    review_requirement,
};
use crate::refusal::{Refusal, command};
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use pulldown_cmark::{Event, Tag, TagEnd};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

const TITLE_REQUEST: &str = "@coderabbitai";
const SUMMARY_REQUEST: &str = "@coderabbitai summary";
const REVIEW_EXCLUSION: &str = "@coderabbitai ignore";
/// Marks an exclusion that lasts only while the owner pauses CodeRabbit PR reviews.
const PAUSE_EXCLUSION: &str = "<!-- coderabbit-pause -->";

#[derive(Parser)]
#[command(about = "Validate PR documents before invoking gh; local creation always uses draft")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Action,
}

#[derive(Subcommand)]
pub enum Action {
    /// Inspect repository scope and the issue before starting GitHub-bound work.
    Start {
        #[command(flatten)]
        target: Target,
    },
    /// Validate local files or the current GitHub document without mutation.
    Check {
        #[command(flatten)]
        target: Target,
        #[arg(
            long,
            required_unless_present = "pr",
            requires = "body_file",
            conflicts_with = "pr"
        )]
        title: Option<String>,
        #[arg(
            long,
            required_unless_present = "pr",
            requires = "title",
            conflicts_with = "pr"
        )]
        body_file: Option<PathBuf>,
        /// Check the current GitHub document instead of local files.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        pr: Option<u64>,
        /// Reject generation placeholders even in CodeRabbit mode.
        #[arg(long)]
        r#final: bool,
    },
    /// Create a draft for local generation or request CodeRabbit generation.
    Create {
        #[command(flatten)]
        document: DocumentArgs,
        #[arg(long)]
        head: String,
        #[arg(long)]
        base: Option<String>,
        /// Also keep CodeRabbit-created PRs as drafts.
        #[arg(long)]
        draft: bool,
        /// Open the PR although it touches paths another open PR touches; shared commits still refuse.
        #[arg(long)]
        independent: bool,
    },
    /// Replace both title and body with one checked document.
    Edit {
        #[command(flatten)]
        document: DocumentArgs,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        pr: u64,
    },
    /// Validate the current GitHub document and move an open draft to review.
    Ready {
        #[command(flatten)]
        target: Target,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        pr: u64,
    },
    /// Merge independent PRs in order: update a behind head by merge, wait for its checks, mark it ready, and merge the verified head.
    Merge {
        #[command(flatten)]
        target: Target,
        /// A PR to merge, repeated in merge order.
        #[arg(long = "pr", required = true, value_parser = clap::value_parser!(u64).range(1..))]
        prs: Vec<u64>,
        #[command(flatten)]
        polling: Polling,
    },
    /// Create, sync, rebase, and merge stacked PRs through the pinned gh-stack.
    Stack {
        #[command(subcommand)]
        action: StackAction,
    },
}

#[derive(Subcommand)]
pub enum StackAction {
    /// Create a draft PR for the head on top of the open PR of the base branch, and link the chain as one stack.
    Create {
        #[command(flatten)]
        document: DocumentArgs,
        #[arg(long)]
        head: String,
        /// The parent PR's head branch.
        #[arg(long)]
        base: String,
    },
    /// Restack the checked-out stack on its updated parents, push it with explicit leases, and sync its PRs.
    Sync {
        #[command(flatten)]
        target: Target,
    },
    /// Rebase the checked-out stack to resolve a conflict, then continue or abort that rebase.
    Rebase {
        #[command(flatten)]
        target: Target,
        #[arg(long = "continue", conflicts_with = "abort")]
        resume: bool,
        #[arg(long)]
        abort: bool,
    },
    /// Merge the stack up to a PR: restack, wait for every check, mark each ready, and merge them together.
    Merge {
        #[command(flatten)]
        target: Target,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        pr: u64,
        #[command(flatten)]
        polling: Polling,
    },
}

#[derive(Args, Clone, Copy)]
pub struct Polling {
    /// Seconds between reads while checks run.
    #[arg(long, default_value_t = 30)]
    interval: u64,
    /// Minutes to wait for each PR before refusing.
    #[arg(long, default_value_t = 120)]
    timeout: u64,
}

impl Action {
    fn name(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::Check { .. } => "check",
            Self::Create { .. } => "create",
            Self::Edit { .. } => "edit",
            Self::Ready { .. } => "ready",
            Self::Merge { .. } => "merge",
            Self::Stack { .. } => "stack",
        }
    }
}

#[derive(Args, Clone)]
pub struct Target {
    /// Exact GitHub OWNER/REPO; no URL or inferred repository.
    #[arg(long)]
    pub repo: String,
    /// CodeRabbit mode permits its standard generation placeholders.
    #[arg(long, value_enum, default_value = "local")]
    pub generation: Generation,
    /// Repository-scoped JSON exception with repository, source URL, and reason.
    #[arg(long)]
    pub title_policy: Option<PathBuf>,
    /// Existing open issue in the destination repository; required for personal non-forks.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    pub issue: Option<u64>,
}

#[derive(Args)]
pub struct DocumentArgs {
    #[command(flatten)]
    target: Target,
    #[arg(long)]
    title: String,
    #[arg(long)]
    body_file: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TitlePolicy {
    repository: String,
    source: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveDocument {
    title: String,
    body: String,
    state: String,
    is_draft: bool,
    #[serde(default)]
    status_check_rollup: Vec<ReportedCheck>,
}

/// One entry of the head commit's check rollup: a check run or a commit status.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportedCheck {
    status: Option<String>,
    conclusion: Option<String>,
    state: Option<String>,
}

impl ReportedCheck {
    fn finished(&self) -> bool {
        match (&self.status, &self.state) {
            (Some(status), _) => status == "COMPLETED",
            (None, Some(state)) => !matches!(state.as_str(), "PENDING" | "EXPECTED"),
            (None, None) => false,
        }
    }

    fn passed(&self) -> bool {
        match (&self.conclusion, &self.state) {
            (Some(conclusion), _) => matches!(conclusion.as_str(), "SUCCESS" | "NEUTRAL"),
            (None, Some(state)) => state == "SUCCESS",
            (None, None) => false,
        }
    }
}

/// The total, unfinished, and failed checks of a rollup.
fn tally(rollup: &[ReportedCheck]) -> (usize, usize, usize) {
    let unfinished = rollup.iter().filter(|check| !check.finished()).count();
    let failed = rollup
        .iter()
        .filter(|check| check.finished() && !check.passed())
        .count();
    (rollup.len(), unfinished, failed)
}

fn checks(rollup: &[ReportedCheck]) -> Checks {
    let (total, unfinished, failed) = tally(rollup);
    checks_state(total, unfinished, failed)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowPolicy {
    schema: u8,
    minimum_remaining: u64,
    personal_owners: Vec<Owner>,
}

struct Github {
    policy: WorkflowPolicy,
}

#[derive(Deserialize)]
struct Owner {
    login: String,
    id: u64,
}

#[derive(Deserialize)]
struct Repository {
    full_name: String,
    owner: Owner,
    fork: bool,
    #[serde(default)]
    default_branch: Option<String>,
}

/// An open PR as `gh pr list` reports it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPr {
    number: u64,
    head_ref_name: String,
    base_ref_name: String,
    #[serde(default)]
    is_cross_repository: bool,
}

/// One PR of a merge, read in one response so its checks belong to its head.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeView {
    title: String,
    body: String,
    state: String,
    is_draft: bool,
    head_ref_oid: String,
    base_ref_name: String,
    mergeable: String,
    merge_state_status: String,
    #[serde(default)]
    status_check_rollup: Vec<ReportedCheck>,
    #[serde(default)]
    closing_issues_references: Vec<ClosingIssue>,
}

#[derive(Deserialize)]
struct ClosingIssue {
    number: u64,
}

#[derive(Deserialize)]
struct Comparison {
    behind_by: u64,
}

#[derive(Deserialize)]
struct LiveIssue {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    html_url: String,
    pull_request: Option<serde_json::Value>,
}

pub struct ValidatedDocument {
    title: String,
    body: String,
}

fn single_line(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

fn conventional(title: &str) -> bool {
    let Some((prefix, description)) = title.split_once(": ") else {
        return false;
    };
    if description.trim().is_empty() {
        return false;
    }
    let prefix = prefix.strip_suffix('!').unwrap_or(prefix);
    let kind = if let Some((kind, scope)) = prefix.split_once('(') {
        let Some(scope) = scope.strip_suffix(')') else {
            return false;
        };
        if scope.trim().is_empty() || scope.contains(['(', ')', '!']) {
            return false;
        }
        kind
    } else {
        prefix
    };
    !kind.is_empty() && kind.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn without_comments(value: &str) -> String {
    let mut remaining = value;
    let mut visible = String::new();
    while let Some((before, comment)) = remaining.split_once("<!--") {
        visible.push_str(before);
        let Some((_, after)) = comment.split_once("-->") else {
            return visible;
        };
        remaining = after;
    }
    visible.push_str(remaining);
    visible
}

fn prose(value: &str) -> String {
    let mut ranges = Vec::new();
    let mut block = None;
    for (event, range) in pulldown_cmark::Parser::new(value).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => block = Some(range.start),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = block.take() {
                    ranges.push(start..range.end);
                }
            }
            Event::Code(_) => ranges.push(range),
            _ => {}
        }
    }
    let mut result = value.to_owned();
    for range in ranges.into_iter().rev() {
        result.replace_range(range, "\n");
    }
    result
}

fn unfinished(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.lines().any(|line| {
        let text = line.trim().trim_start_matches(['#', '>', '*', '-', ' ']);
        ["todo", "tbd", "fixme"].iter().any(|marker| {
            text == *marker
                || text.starts_with(&format!("{marker}:"))
                || text
                    .split_once(": ")
                    .is_some_and(|(_, description)| description.trim() == *marker)
        })
    }) || lower.match_indices("{{").any(|(start, _)| {
        !lower[..start].ends_with('$')
            && lower[start + 2..]
                .split_once("}}")
                .is_some_and(|(label, _)| {
                    matches!(
                        label.trim(),
                        "title"
                            | "description"
                            | "summary"
                            | "why"
                            | "changes"
                            | "validation"
                            | "todo"
                            | "tbd"
                            | "fixme"
                    ) || label.trim().starts_with("insert ")
                        || label.trim().starts_with("replace ")
                })
    }) || [
        "[todo]",
        "[tbd]",
        "[fixme]",
        "[insert",
        "<insert",
        "[replace",
        "<replace",
        "<description>",
        "[description]",
        "[summary]",
        "replace me",
        "describe your changes here",
        "add your description here",
        "fill in here",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

impl Target {
    fn conventional_required(&self) -> Result<bool> {
        let parts: Vec<_> = self.repo.split('/').collect();
        require(
            parts.len() == 2
                && parts.iter().all(|part| {
                    !part.is_empty()
                        && *part != "."
                        && *part != ".."
                        && !part.starts_with('-')
                        && part
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
                }),
            || {
                Refusal::new(
                    "pr.repository",
                    "--repo must be an exact OWNER/REPO",
                    "gh repo view --json nameWithOwner --jq .nameWithOwner",
                )
                .evidence(format!("--repo {:?}", self.repo))
            },
        )?;
        let Some(path) = &self.title_policy else {
            return Ok(true);
        };
        let refusal = |cause: &str| {
            Refusal::new(
                "pr.title-policy",
                cause,
                command(&["cat", "--", &path.display().to_string()]),
            )
            .evidence(format!("--title-policy {}", path.display()))
        };
        let policy: TitlePolicy = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| {
                refusal("the title policy must be a readable JSON object with exactly repository, source, and reason")
            })?;
        require(policy.repository.eq_ignore_ascii_case(&self.repo), || {
            refusal("the title policy belongs to another repository").evidence(format!(
                "policy repository {:?}, --repo {:?}",
                policy.repository, self.repo
            ))
        })?;
        let source = policy.source.strip_prefix("https://").unwrap_or_default();
        require(
            single_line(source)
                && !source.chars().any(char::is_whitespace)
                && source
                    .split('/')
                    .next()
                    .is_some_and(|host| host.contains('.') && !host.starts_with('.'))
                && single_line(&policy.reason),
            || {
                refusal("the title policy needs an HTTPS rules source and a one-line reason")
                    .evidence(format!("source {:?}", policy.source))
            },
        )?;
        Ok(false)
    }
}

/// Refuses with `refusal` unless `condition` holds.
fn require(condition: bool, refusal: impl FnOnce() -> Refusal) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(refusal().into())
    }
}

/// The local check to run on a corrected document; `title` is kept when it was not the problem.
fn recheck(target: &Target, title: Option<&str>) -> String {
    let mut words = vec!["pr-workflow", "check", "--repo", &target.repo];
    if target.generation == Generation::Coderabbit {
        words.extend(["--generation", "coderabbit"]);
    }
    let policy = target
        .title_policy
        .as_ref()
        .map(|path| path.display().to_string());
    if let Some(policy) = &policy {
        words.extend(["--title-policy", policy]);
    }
    words.extend([
        "--title",
        title.unwrap_or("<title>"),
        "--body-file",
        "<body-file>",
    ]);
    command(&words)
}

/// Runs `action` and reports every failure as one refusal.
/// `arguments` are the words after `pr-workflow`, whichever entry point received them.
pub fn execute(action: Action, arguments: &[String]) -> Result<()> {
    let name = action.name();
    let rerun = command(
        &std::iter::once("pr-workflow")
            .chain(arguments.iter().map(String::as_str))
            .collect::<Vec<_>>(),
    );
    run(action, &rerun).map_err(|error| match error.downcast::<Refusal>() {
        Ok(refusal) => refusal.into(),
        Err(error) => failure(&error, name, &rerun).into(),
    })
}

/// The record for a failure that no specific rule describes.
fn failure(error: &anyhow::Error, name: &str, rerun: &str) -> Refusal {
    Refusal::new(
        "pr.failed",
        format!("{error:#}"),
        command(&["pr-workflow", name, "--help"]),
    )
    .evidence(format!("command: {rerun}"))
}

pub fn validate(
    target: &Target,
    title: String,
    body: String,
    final_check: bool,
) -> Result<ValidatedDocument> {
    let required = target.conventional_required()?;
    let title_refusal = |cause: &str| {
        Refusal::new("pr.title", cause, recheck(target, None)).evidence(format!("title: {title:?}"))
    };
    require(single_line(&title) && title.trim() == title, || {
        title_refusal("title must be a nonempty single line without surrounding whitespace")
    })?;
    let request = target.generation == Generation::Coderabbit && !final_check;
    require(!unfinished(&title), || {
        title_refusal("title contains an unfinished placeholder")
    })?;
    if title.contains(TITLE_REQUEST) {
        require(request && title == TITLE_REQUEST, || {
            title_refusal(
                "CodeRabbit title generation is pending; use explicit coderabbit mode only for a generation request",
            )
        })?;
    } else {
        require(!required || conventional(&title), || {
            title_refusal(
                "title must use type(scope)!: description; scope and ! are optional (or supply a repository-scoped title policy)",
            )
        })?;
    }
    let body_refusal = |cause: &str, evidence: String| {
        Refusal::new("pr.body", cause, recheck(target, Some(&title))).evidence(evidence)
    };
    let visible = without_comments(&body);
    let visible = visible
        .lines()
        .filter(|line| line.trim() != REVIEW_EXCLUSION)
        .collect::<Vec<_>>()
        .join("\n");
    require(!visible.trim().is_empty(), || {
        body_refusal(
            "body is empty or contains only comments",
            format!("visible body: {:?}", visible.trim()),
        )
    })?;
    require(!unfinished(&prose(&visible)), || {
        let line = visible
            .lines()
            .find(|line| unfinished(&prose(line)))
            .unwrap_or("a placeholder spanning several lines");
        body_refusal(
            "body contains an unfinished placeholder",
            format!("body line: {line:?}"),
        )
    })?;
    if let Some(line) = body.lines().find(|line| {
        line.contains(TITLE_REQUEST)
            && !coderabbit_body_marker_allowed(
                request,
                line.trim() == SUMMARY_REQUEST,
                line.trim() == REVIEW_EXCLUSION,
            )
    }) {
        return Err(body_refusal(
            "CodeRabbit summary generation is pending or the placeholder is not a standalone standard request",
            format!("body line: {line:?}"),
        )
        .into());
    }
    Ok(ValidatedDocument { title, body })
}

fn read_document(document: DocumentArgs, final_check: bool) -> Result<(Target, ValidatedDocument)> {
    let body = fs::read_to_string(&document.body_file).context("read UTF-8 PR body file")?;
    let validated = validate(&document.target, document.title, body, final_check)?;
    Ok((document.target, validated))
}

fn gh(args: &[OsString]) -> Result<String> {
    let output = Tool::Gh
        .command()
        .env("GH_HOST", "github.com")
        .args(args)
        .output()
        .context("start gh")?;
    ensure!(
        output.status.success(),
        "gh failed with {}: {}; inspect GitHub before retrying an uncertain mutation",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).context("gh returned non-UTF-8 output")
}

impl Github {
    /// Require a live authenticated user without printing or transferring credentials.
    fn connect() -> Result<Self> {
        let policy: WorkflowPolicy = serde_json::from_str(include_str!(
            "../../dot_agents/skills/pull-request/assets/workflow-policy.json"
        ))?;
        require(
            policy.schema == 1
                && policy.minimum_remaining > 0
                && !policy.personal_owners.is_empty(),
            || {
                Refusal::new(
                    "pr.policy",
                    "the installed workflow policy needs schema 1, a nonzero quota reserve, and at least one personal owner",
                    "chezmoi source-path ~/.agents/skills/pull-request/assets/workflow-policy.json",
                )
                .evidence(format!(
                    "schema {}, reserve {}, {} personal owner(s)",
                    policy.schema,
                    policy.minimum_remaining,
                    policy.personal_owners.len()
                ))
            },
        )?;
        let github = Self { policy };
        let user: Owner = github.read("user").map_err(|error| {
            if error.downcast_ref::<Refusal>().is_some() {
                return error;
            }
            Refusal::new(
                "pr.auth",
                "authenticated GitHub access is required",
                "gh auth login --hostname github.com",
            )
            .evidence(format!("gh api user: {error:#}"))
            .into()
        })?;
        require(user.id != 0 && single_line(&user.login), || {
            Refusal::new(
                "pr.auth",
                "GitHub reported no valid authenticated user",
                "gh auth status --hostname github.com",
            )
            .evidence(format!(
                "gh api user: login {:?}, id {}",
                user.login, user.id
            ))
        })?;
        Ok(github)
    }

    /// Inspect live REST quota headers on each serialized prerequisite request.
    fn read<T: serde::de::DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        let args = ["api", endpoint, "--hostname", "github.com", "--include"].map(OsString::from);
        let response = gh(&args)?.replace("\r\n", "\n");
        let (headers, body) = response
            .split_once("\n\n")
            .context("missing GitHub response headers")?;
        ensure!(
            headers
                .lines()
                .next()
                .is_some_and(|line| line.starts_with("HTTP/")
                    && line.split_whitespace().nth(1) == Some("200")),
            "unexpected GitHub response status"
        );
        let header = |name: &str| -> Result<&str> {
            let values: Vec<_> = headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.trim())
                .collect();
            ensure!(
                values.len() == 1,
                "missing or duplicate GitHub {name} header"
            );
            Ok(values[0])
        };
        let remaining: u64 = header("x-ratelimit-remaining")?
            .parse()
            .context("invalid GitHub remaining quota")?;
        let reset: u64 = header("x-ratelimit-reset")?
            .parse()
            .context("invalid GitHub quota reset")?;
        ensure!(
            header("x-ratelimit-resource")? == "core" && reset > 0,
            "invalid GitHub REST quota resource"
        );
        require(
            api_quota_available(remaining, self.policy.minimum_remaining),
            || {
                Refusal::new(
                    "pr.quota",
                    format!(
                        "GitHub REST quota is low ({remaining} remaining); stop until reset at UTC epoch {reset}; no automatic retry"
                    ),
                    "gh api rate_limit --jq .resources.core",
                )
                .evidence(format!(
                    "remaining {remaining}, reserve {}, reset at UTC epoch {reset}",
                    self.policy.minimum_remaining
                ))
            },
        )?;
        serde_json::from_str(body).context("invalid GitHub prerequisite response")
    }
}

/// Recognize a closing reference in visible Markdown, excluding examples and comments.
fn references_issue(body: &str, repo: &str, issue: u64) -> bool {
    let visible = prose(&without_comments(body));
    let mut text = String::new();
    let mut quoted = 0;
    for event in pulldown_cmark::Parser::new(&visible) {
        match event {
            Event::Start(Tag::BlockQuote(_)) => quoted += 1,
            Event::End(TagEnd::BlockQuote(_)) => quoted -= 1,
            Event::Text(value) if quoted == 0 => text.push_str(&value),
            Event::Start(Tag::Link { dest_url, .. }) if quoted == 0 => {
                text.push(' ');
                text.push_str(&dest_url);
                text.push(' ');
            }
            Event::SoftBreak | Event::HardBreak | Event::End(_) => text.push('\n'),
            _ => {}
        }
    }
    let short = format!("#{issue}");
    let url = format!("https://github.com/{repo}/issues/{issue}");
    let words: Vec<_> = text
        .split_whitespace()
        .map(|word| word.trim_matches([',', '.', ':', ';', '!', '(', ')', '[', ']']))
        .collect();
    words.windows(2).any(|pair| {
        matches!(
            pair[0].to_ascii_lowercase().as_str(),
            "close"
                | "closes"
                | "closed"
                | "fix"
                | "fixes"
                | "fixed"
                | "resolve"
                | "resolves"
                | "resolved"
        ) && (pair[1] == short || pair[1].eq_ignore_ascii_case(&url))
    })
}

/// Where a checked PR body lives, so a refusal names the edit that changes it.
#[derive(Clone, Copy)]
enum BodySource<'a> {
    File(&'a Path),
    Pr(u64),
}

/// Establish the global personal scope and validate an explicitly selected issue.
/// `rerun` is the refused command, which a refusal for a body file repeats after the edit.
fn inspect_issue(
    github: &Github,
    target: &Target,
    body: Option<(&str, BodySource)>,
    rerun: &str,
) -> Result<IssueGate> {
    target.conventional_required()?;
    let repository: Repository = github.read(&format!("repos/{}", target.repo))?;
    require(
        repository.full_name.eq_ignore_ascii_case(&target.repo)
            && repository.owner.id != 0
            && repository
                .owner
                .login
                .eq_ignore_ascii_case(target.repo.split('/').next().unwrap()),
        || {
            Refusal::new(
                "pr.repository",
                "repository identity differs from the requested destination",
                command(&[
                    "gh",
                    "repo",
                    "view",
                    &target.repo,
                    "--json",
                    "nameWithOwner,owner",
                ]),
            )
            .evidence(format!(
                "requested {}, GitHub reports {} owned by {} ({})",
                target.repo, repository.full_name, repository.owner.login, repository.owner.id
            ))
        },
    )?;
    let mut personal = false;
    for owner in &github.policy.personal_owners {
        let name_matches = owner.login.eq_ignore_ascii_case(&repository.owner.login);
        let id_matches = owner.id == repository.owner.id;
        require(
            owner.id != 0 && single_line(&owner.login) && name_matches == id_matches,
            || {
                Refusal::new(
                    "pr.policy",
                    "a personal owner in the workflow policy no longer matches GitHub's login and id; the owner must review the policy",
                    command(&[
                        "gh",
                        "api",
                        &format!("users/{}", repository.owner.login),
                        "--jq",
                        "{login, id}",
                    ]),
                )
                .evidence(format!("policy owner {} ({})", owner.login, owner.id))
                .evidence(format!(
                    "repository owner {} ({})",
                    repository.owner.login, repository.owner.id
                ))
            },
        )?;
        personal |= id_matches;
    }
    let Some(number) = target.issue else {
        return Ok(issue_gate(personal, repository.fork, false));
    };
    let issue: LiveIssue = github.read(&format!("repos/{}/issues/{number}", target.repo))?;
    require(
        issue.number == number
            && issue.html_url.eq_ignore_ascii_case(&format!(
                "https://github.com/{}/issues/{number}",
                target.repo
            ))
            && issue.state == "open"
            && issue.pull_request.is_none(),
        || {
            Refusal::new(
                "pr.issue",
                "--issue must identify an open issue in the exact destination, not a PR",
                command(&[
                    "gh",
                    "issue",
                    "list",
                    "--repo",
                    &target.repo,
                    "--state",
                    "open",
                ]),
            )
            .evidence(format!(
                "#{number}: state {}, url {}, pull request {}",
                issue.state,
                issue.html_url,
                issue.pull_request.is_some()
            ))
        },
    )?;
    let issue_body = issue.body.unwrap_or_default();
    let visible = without_comments(&issue_body);
    require(
        single_line(&issue.title)
            && !unfinished(&issue.title)
            && !visible.trim().is_empty()
            && !unfinished(&prose(&visible))
            && !issue_body.contains(TITLE_REQUEST),
        || {
            Refusal::new(
                "pr.issue",
                "the issue must describe the intended problem and scope; its content is empty or unfinished",
                command(&[
                    "gh",
                    "issue",
                    "edit",
                    &number.to_string(),
                    "--repo",
                    &target.repo,
                    "--body-file",
                    "<body-file>",
                ]),
            )
            .evidence(format!(
                "#{number}: title {:?}, {} visible body characters",
                issue.title,
                visible.trim().chars().count()
            ))
        },
    )?;
    if let Some((body, source)) = body {
        require(references_issue(body, &target.repo, number), || {
            let next = match source {
                BodySource::File(path) => format!(
                    "{} >> {} && {rerun}",
                    command(&["printf", &format!("\\n\\nCloses #{number}\\n")]),
                    command(&[&path.display().to_string()]),
                ),
                BodySource::Pr(pr) => command(&[
                    "gh",
                    "pr",
                    "edit",
                    &pr.to_string(),
                    "--repo",
                    &target.repo,
                    "--body-file",
                    "<body-file>",
                ]),
            };
            Refusal::new(
                "pr.body",
                format!(
                    "PR body must visibly close the selected issue, for example: Closes #{number}."
                ),
                next,
            )
            .evidence(format!(
                "no closing reference to #{number} in the visible body"
            ))
        })?;
    }
    Ok(issue_gate(personal, repository.fork, true))
}

fn require_issue(gate: IssueGate, target: &Target) -> Result<()> {
    require(gate != IssueGate::Missing, || missing_issue(target))
}

fn missing_issue(target: &Target) -> Refusal {
    Refusal::new(
        "pr.issue",
        "personal non-fork repositories require --issue NUMBER; record the problem, scope, and acceptance criteria before starting",
        command(&[
            "gh",
            "issue",
            "create",
            "--repo",
            &target.repo,
            "--title",
            "<title>",
            "--body-file",
            "<body-file>",
        ]),
    )
    .evidence(format!("{} is a personal non-fork repository and no --issue was given", target.repo))
}

/// Plans `operation` on a validated document and names the unmet condition when it is refused.
/// `pr` is the existing PR a ready transition acts on.
fn planned(
    operation: Operation,
    target: &Target,
    pr: Option<u64>,
    draft: bool,
    issue: IssueGate,
    checks: Checks,
) -> Result<Effect> {
    let blocker = match plan(operation, target.generation, true, draft, issue, checks) {
        Ok(effect) => return Ok(effect),
        Err(blocker) => blocker,
    };
    let pr = pr.map_or_else(|| "<number>".to_owned(), |pr| pr.to_string());
    let view = |fields: &str| {
        command(&[
            "gh",
            "pr",
            "view",
            &pr,
            "--repo",
            &target.repo,
            "--json",
            fields,
        ])
    };
    Err(match blocker {
        Blocker::MissingIssue => missing_issue(target),
        Blocker::Unvalidated => Refusal::new(
            "pr.body",
            "the document was not validated",
            recheck(target, None),
        )
        .evidence(format!("{operation:?} without a validated document")),
        Blocker::NotDraft => Refusal::new(
            "pr.state",
            "only a draft can become ready",
            view("state,isDraft"),
        )
        .evidence(format!("#{pr}: not a draft")),
        Blocker::UnfinishedChecks => Refusal::new(
            "pr.checks",
            "a draft becomes ready only when every reported check on its head has passed and no work remains",
            command(&["gh", "pr", "checks", &pr, "--repo", &target.repo]),
        )
        .evidence(format!("#{pr}: head checks {checks:?}")),
    }
    .into())
}

/// Read the owner's CodeRabbit PR review pause from the guard's state.
fn pr_reviews_paused() -> Result<bool> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).context(
        "the user directory is unavailable; set HOME or USERPROFILE to read the CodeRabbit pause",
    )?;
    Ok(crate::review_guard::pr_reviews_paused(
        crate::review_guard::pause(std::path::Path::new(&home))?,
    ))
}

fn excludes_review(body: &str) -> bool {
    without_comments(body)
        .lines()
        .any(|line| line.trim() == REVIEW_EXCLUSION)
}

fn review_exclusion(body: &str) -> Exclusion {
    exclusion(
        excludes_review(body),
        body.lines().any(|line| line.trim() == PAUSE_EXCLUSION),
    )
}

/// Refuse an operation that would request a CodeRabbit PR review while the owner pauses them.
/// Also refuse one that would leave the review unowed after the owner resumes them.
/// `live` yields the PR's current draft state and exclusion; it is read only during a pause.
fn admit_review_request(
    operation: Operation,
    generation: Generation,
    body: &str,
    live: impl FnOnce() -> Result<(bool, Exclusion)>,
) -> Result<()> {
    if !pr_reviews_paused()? {
        return Ok(());
    }
    let (draft, was) = if generation == Generation::Coderabbit {
        (true, Exclusion::None)
    } else {
        live()?
    };
    ensure!(
        review_request_allowed(
            true,
            requests_review(
                operation,
                generation,
                draft,
                was != Exclusion::None,
                excludes_review(body)
            )
        ),
        "CodeRabbit PR reviews are paused by the owner (`coderabbit --guard-status --json` reports reviews.pr paused), and this operation would request one; {}",
        match (generation, operation) {
            (Generation::Coderabbit, _) =>
                "rerun with --generation local and a completed title and body".to_owned(),
            (_, Operation::Edit) => format!(
                "keep the standalone `{REVIEW_EXCLUSION}` line of this ready PR until the owner resumes PR reviews"
            ),
            _ => format!(
                "keep the draft until the owner resumes PR reviews, or add the standalone lines `{REVIEW_EXCLUSION}` and `{PAUSE_EXCLUSION}` with pr-workflow edit so the review is owed again after resumption"
            ),
        }
    );
    ensure!(
        keeps_pause_exclusion(operation, draft, was, review_exclusion(body)),
        "CodeRabbit PR reviews are paused by the owner (`coderabbit --guard-status --json` reports reviews.pr paused), and without the standalone `{PAUSE_EXCLUSION}` line the review would not be owed after resumption; {}",
        if operation == Operation::Ready {
            format!(
                "add the standalone `{PAUSE_EXCLUSION}` line beside `{REVIEW_EXCLUSION}` with pr-workflow edit, then rerun ready"
            )
        } else {
            format!(
                "keep the standalone `{PAUSE_EXCLUSION}` line of this ready PR until the owner resumes PR reviews"
            )
        }
    );
    Ok(())
}

fn report_review_requirement(body: &str) -> Result<()> {
    println!(
        "{}",
        match review_requirement(pr_reviews_paused()?, review_exclusion(body)) {
            ReviewRequirement::Paused =>
                "CodeRabbit PR review: not required while the owner pauses PR reviews; do not wait for or request one".to_owned(),
            ReviewRequirement::Excluded => format!(
                "CodeRabbit PR review: excluded by the body's `{REVIEW_EXCLUSION}` line; none is owed"
            ),
            ReviewRequirement::Resumed => format!(
                "CodeRabbit PR review: required again now that PR reviews have resumed; remove the `{REVIEW_EXCLUSION}` and `{PAUSE_EXCLUSION}` lines with pr-workflow edit, then request it with an authorized `@coderabbitai review` comment"
            ),
            ReviewRequirement::Required =>
                "CodeRabbit PR review: required for the current head where the destination enables CodeRabbit; complete it with coderabbit-review".to_owned(),
        }
    );
    Ok(())
}

fn arguments(operation: &str, repo: &str) -> Vec<OsString> {
    ["pr", operation, "--repo", repo]
        .into_iter()
        .map(OsString::from)
        .collect()
}

fn write_document(
    args: &mut Vec<OsString>,
    document: &ValidatedDocument,
) -> Result<tempfile::NamedTempFile> {
    let mut body = tempfile::NamedTempFile::new().context("reserve private PR body file")?;
    body.write_all(document.body.as_bytes())?;
    body.flush()?;
    args.extend([
        OsString::from("--title"),
        OsString::from(&document.title),
        OsString::from("--body-file"),
        body.path().as_os_str().to_owned(),
    ]);
    Ok(body)
}

fn branch(value: &str) -> Result<()> {
    require(
        single_line(value) && !value.starts_with('-') && !value.chars().any(char::is_whitespace),
        || {
            Refusal::new(
                "pr.branch",
                "head and base must be nonempty branch names",
                "git branch --show-current",
            )
            .evidence(format!("branch: {value:?}"))
        },
    )
}

fn live_document(_github: &Github, target: &Target, pr: u64) -> Result<LiveDocument> {
    target.conventional_required()?;
    let mut args = arguments("view", &target.repo);
    args.extend([
        pr.to_string().into(),
        "--json".into(),
        "title,body,state,isDraft,statusCheckRollup".into(),
    ]);
    serde_json::from_str(&gh(&args)?).context("invalid gh PR document")
}

fn run(action: Action, rerun: &str) -> Result<()> {
    match action {
        Action::Start { target } => {
            target.conventional_required()?;
            let github = Github::connect()?;
            let gate = inspect_issue(&github, &target, None, rerun)?;
            require_issue(gate, &target)?;
            println!(
                "Repository prerequisites accepted ({gate:?}); inspect the destination rules and issue scope before implementation"
            );
        }
        Action::Check {
            target,
            title,
            body_file,
            pr,
            r#final,
        } => {
            if let Some(pr) = pr {
                target.conventional_required()?;
                let github = Github::connect()?;
                let live = live_document(&github, &target, pr)?;
                let document = validate(&target, live.title, live.body, r#final)?;
                require_issue(
                    inspect_issue(
                        &github,
                        &target,
                        Some((&document.body, BodySource::Pr(pr))),
                        rerun,
                    )?,
                    &target,
                )?;
                report_review_requirement(&document.body)?;
            } else {
                read_document(
                    DocumentArgs {
                        target,
                        title: title.context("title required")?,
                        body_file: body_file.context("body file required")?,
                    },
                    r#final,
                )?;
            }
            println!(
                "PR document accepted; local-file checks do not inspect issues and validation claims still require execution evidence"
            );
        }
        Action::Create {
            document,
            head,
            base,
            draft,
            independent,
        } => {
            let body_file = document.body_file.clone();
            let title = document.title.clone();
            let (target, document) = read_document(document, false)?;
            branch(&head)?;
            if let Some(base) = &base {
                branch(base)?;
            }
            admit_review_request(Operation::Create, target.generation, &document.body, || {
                Ok((true, Exclusion::None))
            })?;
            let github = Github::connect()?;
            let issue = inspect_issue(
                &github,
                &target,
                Some((&document.body, BodySource::File(&body_file))),
                rerun,
            )?;
            let effect = planned(
                Operation::Create,
                &target,
                None,
                draft,
                issue,
                Checks::Incomplete,
            )?;
            refuse_dependent(
                &github,
                &target,
                &head,
                base.as_deref(),
                independent,
                &title,
                &body_file,
            )?;
            let mut args = arguments("create", &target.repo);
            args.extend([OsString::from("--head"), head.into()]);
            if let Some(base) = base {
                args.extend([OsString::from("--base"), base.into()]);
            }
            if effect == Effect::CreateDraft {
                args.push("--draft".into());
            }
            let _body = write_document(&mut args, &document)?;
            print!("{}", gh(&args)?);
        }
        Action::Edit { document, pr } => {
            let body_file = document.body_file.clone();
            let (target, document) = read_document(document, false)?;
            let mut connected = None;
            admit_review_request(Operation::Edit, target.generation, &document.body, || {
                let live = live_document(connected.insert(Github::connect()?), &target, pr)?;
                Ok((live.is_draft, review_exclusion(&live.body)))
            })?;
            let github = match connected {
                Some(github) => github,
                None => Github::connect()?,
            };
            let issue = inspect_issue(
                &github,
                &target,
                Some((&document.body, BodySource::File(&body_file))),
                rerun,
            )?;
            planned(
                Operation::Edit,
                &target,
                Some(pr),
                false,
                issue,
                Checks::Incomplete,
            )?;
            let mut args = vec![
                OsString::from("pr"),
                "edit".into(),
                pr.to_string().into(),
                "--repo".into(),
                target.repo.into(),
            ];
            let _body = write_document(&mut args, &document)?;
            print!("{}", gh(&args)?);
        }
        Action::Ready { target, pr } => {
            target.conventional_required()?;
            let github = Github::connect()?;
            let live = live_document(&github, &target, pr)?;
            let inspect = || {
                command(&[
                    "gh",
                    "pr",
                    "view",
                    &pr.to_string(),
                    "--repo",
                    &target.repo,
                    "--json",
                    "state,isDraft",
                ])
            };
            require(live.state == "OPEN", || {
                Refusal::new("pr.state", "only an open PR can become ready", inspect())
                    .evidence(format!("#{pr}: state {}", live.state))
            })?;
            let body = make_ready(
                &github,
                &target,
                pr,
                live.title,
                live.body,
                live.is_draft,
                checks(&live.status_check_rollup),
                rerun,
            )?;
            report_review_requirement(&body)?;
        }
        Action::Merge {
            target,
            prs,
            polling,
        } => {
            target.conventional_required()?;
            let github = Github::connect()?;
            for pr in prs {
                merge_independent(&github, &target, pr, polling, rerun)?;
            }
        }
        Action::Stack { action } => stack(action, rerun)?,
    }
    Ok(())
}

/// Validates a live document and moves the open draft to review, returning the checked body.
#[allow(clippy::too_many_arguments)]
fn make_ready(
    github: &Github,
    target: &Target,
    pr: u64,
    title: String,
    body: String,
    draft: bool,
    checks: Checks,
    rerun: &str,
) -> Result<String> {
    let document = validate(target, title, body, false)?;
    let issue = inspect_issue(
        github,
        target,
        Some((&document.body, BodySource::Pr(pr))),
        rerun,
    )?;
    planned(Operation::Ready, target, Some(pr), draft, issue, checks)?;
    admit_review_request(Operation::Ready, target.generation, &document.body, || {
        Ok((draft, review_exclusion(&document.body)))
    })?;
    let mut args = arguments("ready", &target.repo);
    args.push(pr.to_string().into());
    print!("{}", gh(&args)?);
    Ok(document.body)
}

fn open_prs(target: &Target) -> Result<Vec<OpenPr>> {
    let mut args = arguments("list", &target.repo);
    args.extend(
        [
            "--state",
            "open",
            "--limit",
            "500",
            "--json",
            "number,headRefName,baseRefName,isCrossRepository",
        ]
        .map(OsString::from),
    );
    let listed: Vec<OpenPr> = serde_json::from_str(&gh(&args)?).context("invalid gh PR list")?;
    Ok(listed
        .into_iter()
        .filter(|pr| !pr.is_cross_repository)
        .collect())
}

/// The open PRs from the trunk up to the one whose head is `head`, bottom first, and the trunk below them.
fn chain(open: &[OpenPr], head: &str) -> (Vec<u64>, String) {
    let mut numbers = Vec::new();
    let mut branch = head.to_owned();
    // Each PR is taken at most once, so a cycle of bases ends the walk.
    while let Some(pr) = open
        .iter()
        .find(|pr| pr.head_ref_name == branch && !numbers.contains(&pr.number))
    {
        numbers.push(pr.number);
        branch.clone_from(&pr.base_ref_name);
    }
    numbers.reverse();
    (numbers, branch)
}

fn merge_view(target: &Target, pr: u64) -> Result<MergeView> {
    let mut args = arguments("view", &target.repo);
    args.extend([
        pr.to_string().into(),
        "--json".into(),
        "title,body,state,isDraft,headRefOid,baseRefName,mergeable,mergeStateStatus,statusCheckRollup,closingIssuesReferences".into(),
    ]);
    serde_json::from_str(&gh(&args)?).context("invalid gh PR merge state")
}

/// The merge state of `view`; a stacked PR's merge requirements are GitHub's to evaluate when the stack merges.
fn merge_state(
    github: &Github,
    target: &Target,
    view: &MergeView,
    stacked: bool,
) -> Result<MergeState> {
    let lifecycle = match (view.state.as_str(), view.is_draft) {
        ("MERGED", _) => Lifecycle::Merged,
        ("CLOSED", _) => Lifecycle::Closed,
        (_, true) => Lifecycle::Draft,
        _ => Lifecycle::Ready,
    };
    let behind = if matches!(lifecycle, Lifecycle::Draft | Lifecycle::Ready) {
        let comparison: Comparison = github.read(&format!(
            "repos/{}/compare/{}...{}",
            target.repo, view.base_ref_name, view.head_ref_oid
        ))?;
        comparison.behind_by > 0
    } else {
        false
    };
    let (total, unfinished, failed) = tally(&view.status_check_rollup);
    Ok(MergeState {
        lifecycle,
        conflict: view.mergeable == "CONFLICTING",
        behind,
        checks: head_checks(total, unfinished, failed),
        clean: stacked || matches!(view.merge_state_status.as_str(), "CLEAN" | "HAS_HOOKS"),
        exclusion: review_exclusion(&view.body),
    })
}

/// The structured refusal for a merge that stops at `pr`.
fn stop_refusal(target: &Target, pr: u64, stop: Stop, stacked: bool, view: &MergeView) -> Refusal {
    let number = pr.to_string();
    let evidence = format!(
        "#{pr}: head {}, mergeable {}, merge state {}",
        view.head_ref_oid, view.mergeable, view.merge_state_status
    );
    let (rule, cause, next) = match stop {
        Stop::Closed => (
            "pr.state",
            "a closed PR cannot merge; reopen it or leave it out of the merge".to_owned(),
            command(&["gh", "pr", "view", &number, "--repo", &target.repo]),
        ),
        Stop::Conflict if stacked => (
            "pr.conflict",
            "the stack conflicts with its parent; resolve the conflict in the stack rebase, then merge again".to_owned(),
            command(&["pr-workflow", "stack", "rebase", "--repo", &target.repo]),
        ),
        Stop::Conflict => (
            "pr.conflict",
            format!(
                "the head conflicts with {}; merge {} into it, resolve the conflict, push, and merge again",
                view.base_ref_name, view.base_ref_name
            ),
            command(&["gh", "pr", "checkout", &number, "--repo", &target.repo]),
        ),
        Stop::FailedChecks => (
            "pr.checks",
            "a check on the head failed; fix it, push, and merge again".to_owned(),
            command(&["gh", "pr", "checks", &number, "--repo", &target.repo]),
        ),
        Stop::Review => (
            "pr.review",
            "a CodeRabbit PR review of the head is owed; complete it and its supported findings with coderabbit-review, which this command cannot verify, then merge the PR yourself".to_owned(),
            command(&["gh", "pr", "view", &number, "--repo", &target.repo, "--comments"]),
        ),
    };
    Refusal::new(rule, cause, next).evidence(evidence)
}

/// Waits one interval, or refuses once the PR has waited longer than the timeout.
fn wait(target: &Target, pr: u64, polling: Polling, started: Instant) -> Result<()> {
    require(
        started.elapsed() < Duration::from_secs(polling.timeout.saturating_mul(60)),
        || {
            Refusal::new(
                "pr.timeout",
                format!(
                    "#{pr} did not reach a mergeable state within {} minutes; inspect its checks and merge requirements, then merge again",
                    polling.timeout
                ),
                command(&["gh", "pr", "checks", &pr.to_string(), "--repo", &target.repo]),
            )
            .evidence(format!("waited {} seconds", started.elapsed().as_secs()))
        },
    )?;
    std::thread::sleep(Duration::from_secs(polling.interval));
    Ok(())
}

/// Adds the CodeRabbit pause lines, so the draft becomes ready during the owner's pause and the review is owed again after resumption.
fn add_pause_lines(target: &Target, pr: u64, view: &MergeView) -> Result<()> {
    let mut body = view.body.trim_end().to_owned();
    body.push('\n');
    if !excludes_review(&view.body) {
        body.push('\n');
        body.push_str(REVIEW_EXCLUSION);
    }
    body.push('\n');
    body.push_str(PAUSE_EXCLUSION);
    body.push('\n');
    let document = validate(target, view.title.clone(), body, false)?;
    let mut args = vec![
        OsString::from("pr"),
        "edit".into(),
        pr.to_string().into(),
        "--repo".into(),
        target.repo.clone().into(),
    ];
    let _body = write_document(&mut args, &document)?;
    print!("{}", gh(&args)?);
    Ok(())
}

/// The target of one PR's ready transition: its own closing issue unless the command named one.
fn ready_target(target: &Target, view: &MergeView) -> Target {
    let mut target = target.clone();
    target.issue = target.issue.or_else(|| {
        view.closing_issues_references
            .first()
            .map(|issue| issue.number)
    });
    target
}

/// Merges one independent PR, updating it, waiting for its checks, and marking it ready on the way.
fn merge_independent(
    github: &Github,
    target: &Target,
    pr: u64,
    polling: Polling,
    rerun: &str,
) -> Result<()> {
    let paused = pr_reviews_paused()?;
    let started = Instant::now();
    loop {
        let view = merge_view(target, pr)?;
        match merge_step(merge_state(github, target, &view, false)?, false, paused) {
            Step::Done => {
                println!("#{pr} merged at {}", view.head_ref_oid);
                return Ok(());
            }
            Step::Update => {
                let mut args = arguments("update-branch", &target.repo);
                args.push(pr.to_string().into());
                print!("{}", gh(&args)?);
                wait(target, pr, polling, started)?;
            }
            Step::Restack => bail!("an independent PR is never restacked"),
            Step::Wait => wait(target, pr, polling, started)?,
            Step::Exclude => add_pause_lines(target, pr, &view)?,
            Step::Ready => {
                make_ready(
                    github,
                    &ready_target(target, &view),
                    pr,
                    view.title,
                    view.body,
                    true,
                    checks(&view.status_check_rollup),
                    rerun,
                )?;
            }
            Step::Merge => {
                let mut args = arguments("merge", &target.repo);
                args.extend(
                    [
                        &pr.to_string(),
                        "--squash",
                        "--match-head-commit",
                        &view.head_ref_oid,
                    ]
                    .map(OsString::from),
                );
                print!("{}", gh(&args)?);
                wait(target, pr, polling, started)?;
            }
            Step::Stop(stop) => return Err(stop_refusal(target, pr, stop, false, &view).into()),
        }
    }
}

/// The variable and file through which dotguard admits the stack tooling's rewrites; dotguard's `stack` module reads the same names.
const STACK_TOKEN: &str = "DOTGUARD_STACK";
const STACK_LEASE: &str = "dotguard-stack.lease";

/// The repository's stack lease, held while gh-stack rewrites and pushes branches and removed when dropped.
struct Lease {
    path: PathBuf,
    token: String,
}

impl Lease {
    fn acquire() -> Result<Self> {
        let directory = git_output(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let path = Path::new(directory.trim()).join(STACK_LEASE);
        let probe = tempfile::NamedTempFile::new()?;
        let mut seed = Sha256::new();
        seed.update(std::process::id().to_le_bytes());
        seed.update(format!("{:?}", std::time::SystemTime::now()));
        seed.update(probe.path().as_os_str().as_encoded_bytes());
        let token: String = seed
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                Refusal::new(
                    "pr.stack",
                    "another stack command holds the lease, or one was interrupted; wait for it, or remove the lease once no stack command runs",
                    command(&["rm", "--", &path.display().to_string()]),
                )
                .evidence(format!("{}: {error}", path.display()))
            })?;
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
        Ok(Self { path, token })
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn git_output(args: &[&str]) -> Result<String> {
    let output = Tool::Git
        .command()
        .args(args)
        .output()
        .context("start git")?;
    ensure!(
        output.status.success(),
        "git {} failed with {}: {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).context("git returned non-UTF-8 output")
}

fn git_set(args: &[&str]) -> Result<BTreeSet<String>> {
    Ok(git_output(args)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect())
}

/// The `OWNER/REPO` a GitHub remote URL names.
fn github_slug(url: &str) -> Option<String> {
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

/// Requires the current directory to be a checkout whose `origin` is the target repository.
fn require_checkout(target: &Target) -> Result<()> {
    let url = git_output(&["remote", "get-url", "origin"]).unwrap_or_default();
    let slug = github_slug(url.trim());
    require(
        slug.as_deref()
            .is_some_and(|slug| slug.eq_ignore_ascii_case(&target.repo)),
        || {
            Refusal::new(
                "pr.stack",
                "stack commands run in a checkout whose origin is the target repository",
                command(&["gh", "repo", "clone", &target.repo]),
            )
            .evidence(format!("origin: {:?}", url.trim()))
        },
    )
}

/// Runs the pinned gh-stack against the target repository, with the lease when it rewrites or pushes branches.
fn gh_stack(target: &Target, lease: Option<&Lease>, args: &[&str]) -> Result<()> {
    let mut process = Tool::GhStack.command();
    process
        .args(args)
        .env("GH_HOST", "github.com")
        .env("GH_REPO", &target.repo)
        .env("GH_STACK_NO_UPDATE_NOTIFIER", "1")
        .env_remove(STACK_TOKEN)
        .stdin(Stdio::null());
    if let Some(lease) = lease {
        process.env(STACK_TOKEN, &lease.token);
    }
    let status = process
        .status()
        .context("start the pinned gh-stack; run `mise install` in the repository")?;
    let rebase = || command(&["pr-workflow", "stack", "rebase", "--repo", &target.repo]);
    match status.code() {
        Some(0) => Ok(()),
        // gh-stack reports a rebase conflict with exit status 3.
        Some(3) if args.first() == Some(&"rebase") => Err(Refusal::new(
            "pr.conflict",
            "the stack rebase stopped at a conflict; resolve the listed files, stage them, and continue",
            format!("{} --continue", rebase()),
        )
        .evidence(format!("gh-stack {} exited with {status}", args.join(" ")))
        .into()),
        Some(3) => Err(Refusal::new(
            "pr.conflict",
            "the stack conflicts with its updated parent; gh-stack restored every branch, so rebase the stack to resolve it",
            rebase(),
        )
        .evidence(format!("gh-stack {} exited with {status}", args.join(" ")))
        .into()),
        _ => bail!("gh-stack {} failed with {status}", args.join(" ")),
    }
}

/// Restacks the checked-out stack under the lease, pushing it with explicit leases.
fn sync_stack(target: &Target) -> Result<()> {
    require_checkout(target)?;
    let branch = git_output(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let lease = Lease::acquire()?;
    // Checking out the current branch adopts the stack GitHub records for it, when this clone does not track it yet.
    gh_stack(target, Some(&lease), &["checkout", branch.trim()])?;
    gh_stack(target, Some(&lease), &["sync"])
}

/// Merges the stack up to `top` once every PR in it can merge.
fn merge_stack(
    github: &Github,
    target: &Target,
    top: u64,
    polling: Polling,
    rerun: &str,
) -> Result<()> {
    let paused = pr_reviews_paused()?;
    let started = Instant::now();
    'restart: loop {
        let open = open_prs(target)?;
        let Some(top_head) = open
            .iter()
            .find(|pr| pr.number == top)
            .map(|pr| pr.head_ref_name.clone())
        else {
            let view = merge_view(target, top)?;
            if view.state == "MERGED" {
                println!("#{top} and the PRs below it are merged");
                return Ok(());
            }
            return Err(stop_refusal(target, top, Stop::Closed, true, &view).into());
        };
        let (numbers, _) = chain(&open, &top_head);
        let mut verified = Vec::new();
        for &pr in &numbers {
            let view = merge_view(target, pr)?;
            match merge_step(merge_state(github, target, &view, true)?, true, paused) {
                Step::Merge => verified.push((pr, view.head_ref_oid)),
                Step::Done => {}
                Step::Restack => {
                    sync_stack(target)?;
                    continue 'restart;
                }
                Step::Wait => {
                    wait(target, pr, polling, started)?;
                    continue 'restart;
                }
                Step::Exclude => {
                    add_pause_lines(target, pr, &view)?;
                    continue 'restart;
                }
                Step::Ready => {
                    make_ready(
                        github,
                        &ready_target(target, &view),
                        pr,
                        view.title,
                        view.body,
                        true,
                        checks(&view.status_check_rollup),
                        rerun,
                    )?;
                    continue 'restart;
                }
                Step::Update => bail!("a stacked PR is restacked, never updated by merge"),
                Step::Stop(stop) => return Err(stop_refusal(target, pr, stop, true, &view).into()),
            }
        }
        if verified.is_empty() {
            println!("#{top} and the PRs below it are merged");
            return Ok(());
        }
        gh_stack(
            target,
            None,
            &["merge", &top.to_string(), "--yes", "--squash"],
        )?;
        for (pr, oid) in verified {
            let view = merge_view(target, pr)?;
            require(view.state == "MERGED" && view.head_ref_oid == oid, || {
                Refusal::new(
                    "pr.merge",
                    "a PR of the stack did not merge at the head whose checks were verified",
                    command(&["gh", "pr", "view", &pr.to_string(), "--repo", &target.repo]),
                )
                .evidence(format!(
                    "#{pr}: state {}, head {}, verified {oid}",
                    view.state, view.head_ref_oid
                ))
            })?;
            println!("#{pr} merged at {oid}");
        }
        return Ok(());
    }
}

/// Refuses an independent PR whose head shares commits, or unless declared independent touched paths, with another open PR.
fn refuse_dependent(
    github: &Github,
    target: &Target,
    head: &str,
    base: Option<&str>,
    independent: bool,
    title: &str,
    body_file: &Path,
) -> Result<()> {
    let open = open_prs(target)?;
    let others: Vec<&OpenPr> = open.iter().filter(|pr| pr.head_ref_name != head).collect();
    if others.is_empty() {
        return Ok(());
    }
    let trunk = match base {
        Some(base) => base.to_owned(),
        None => {
            let repository: Repository = github.read(&format!("repos/{}", target.repo))?;
            repository
                .default_branch
                .context("GitHub reported no default branch")?
        }
    };
    git_output(&["fetch", "--quiet", "origin"])?;
    let trunk = format!("refs/remotes/origin/{trunk}");
    let remote_head = format!("refs/remotes/origin/{head}");
    let tip = if git_output(&["rev-parse", "--verify", "--quiet", &remote_head]).is_ok() {
        remote_head
    } else {
        format!("refs/heads/{head}")
    };
    let commits = |tip: &str| git_set(&["rev-list", &format!("{trunk}..{tip}")]);
    let paths = |tip: &str| git_set(&["diff", "--name-only", &format!("{trunk}...{tip}")]);
    let (own_commits, own_paths) = (commits(&tip)?, paths(&tip)?);
    for other in others {
        let theirs = format!("refs/remotes/origin/{}", other.head_ref_name);
        let shared_commits = !own_commits.is_disjoint(&commits(&theirs)?);
        let shared_paths = !own_paths.is_disjoint(&paths(&theirs)?);
        let Some(found) = dependency(shared_commits, shared_paths, independent) else {
            continue;
        };
        let mut next = vec!["pr-workflow", "stack", "create", "--repo", &target.repo];
        let issue = target.issue.map(|issue| issue.to_string());
        if let Some(issue) = &issue {
            next.extend(["--issue", issue]);
        }
        let body_file = body_file.display().to_string();
        next.extend([
            "--title",
            title,
            "--body-file",
            &body_file,
            "--head",
            head,
            "--base",
            &other.head_ref_name,
        ]);
        return Err(Refusal::new(
            "pr.dependency",
            match found {
                Dependency::Commits => format!(
                    "{head} contains commits of #{}, so it depends on that PR; open it as a stack on top of it",
                    other.number
                ),
                Dependency::Paths => format!(
                    "{head} touches paths #{} touches; open it as a stack on top of that PR, or pass --independent when the changes do not depend on each other",
                    other.number
                ),
            },
            command(&next),
        )
        .evidence(format!(
            "#{} ({}) shares {}",
            other.number,
            other.head_ref_name,
            match found {
                Dependency::Commits => "commits",
                Dependency::Paths => "touched paths",
            }
        ))
        .into());
    }
    Ok(())
}

/// The PR number at the end of the URL that `gh pr create` printed.
fn created_number(output: &str) -> Result<u64> {
    output
        .trim()
        .rsplit('/')
        .next()
        .and_then(|number| number.parse().ok())
        .context("gh pr create printed no PR URL")
}

fn stack(action: StackAction, rerun: &str) -> Result<()> {
    match action {
        StackAction::Create {
            document,
            head,
            base,
        } => {
            let body_file = document.body_file.clone();
            let (target, document) = read_document(document, false)?;
            branch(&head)?;
            branch(&base)?;
            admit_review_request(Operation::Create, target.generation, &document.body, || {
                Ok((true, Exclusion::None))
            })?;
            let github = Github::connect()?;
            let issue = inspect_issue(
                &github,
                &target,
                Some((&document.body, BodySource::File(&body_file))),
                rerun,
            )?;
            planned(
                Operation::Create,
                &target,
                None,
                true,
                issue,
                Checks::Incomplete,
            )?;
            let (below, trunk) = chain(&open_prs(&target)?, &base);
            require(!below.is_empty(), || {
                Refusal::new(
                    "pr.stack",
                    format!("{base} has no open PR to stack on; open the parent PR first"),
                    command(&[
                        "pr-workflow",
                        "create",
                        "--repo",
                        &target.repo,
                        "--head",
                        &base,
                        "--title",
                        "<title>",
                        "--body-file",
                        "<body-file>",
                    ]),
                )
                .evidence(format!("no open PR has the head {base}"))
            })?;
            let mut args = arguments("create", &target.repo);
            args.extend(["--head", &head, "--base", &base, "--draft"].map(OsString::from));
            let _body = write_document(&mut args, &document)?;
            let created = gh(&args)?;
            print!("{created}");
            let mut link = vec!["link".to_owned(), "--base".to_owned(), trunk];
            link.extend(below.iter().map(u64::to_string));
            link.push(created_number(&created)?.to_string());
            gh_stack(
                &target,
                None,
                &link.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
        }
        StackAction::Sync { target } => {
            target.conventional_required()?;
            sync_stack(&target)?;
        }
        StackAction::Rebase {
            target,
            resume,
            abort,
        } => {
            target.conventional_required()?;
            require_checkout(&target)?;
            let lease = Lease::acquire()?;
            let mut args = vec!["rebase"];
            if resume {
                args.push("--continue");
            }
            if abort {
                args.push("--abort");
            }
            gh_stack(&target, Some(&lease), &args)?;
            if !abort {
                drop(lease);
                sync_stack(&target)?;
            }
        }
        StackAction::Merge {
            target,
            pr,
            polling,
        } => {
            target.conventional_required()?;
            let github = Github::connect()?;
            merge_stack(&github, &target, pr, polling, rerun)?;
        }
    }
    Ok(())
}

pub fn install(binary: &std::path::Path, user_directory: &std::path::Path) -> Result<()> {
    let directory = user_directory.join(".local/bin");
    fs::create_dir_all(&directory)?;
    let file = tempfile::NamedTempFile::new_in(&directory)?;
    fs::copy(binary, file.path())?;
    file.as_file().sync_all()?;
    file.persist(directory.join(format!("pr-workflow{}", std::env::consts::EXE_SUFFIX)))?;
    println!("Installed pr-workflow in {}", directory.display());
    Ok(())
}

pub fn deploy(
    source: &std::path::Path,
    binary: &std::path::Path,
    user_directory: &std::path::Path,
    mut apply: impl FnMut(&[PathBuf]) -> Result<()>,
) -> Result<()> {
    use crate::skill_ops as ops;
    ops::check_catalog(source)?;
    let manifest = user_directory.join(".config/skill-ops/approved.json");
    let mut approved = ops::read_approved(&manifest)?;
    ensure!(
        ops::hash(&approved.catalog)? == approved.catalog_hash,
        "installed managed catalog is inconsistent"
    );
    let mut retained = approved.catalog.clone();
    retained.skills.remove("pull-request");
    ensure!(
        ops::catalog_owned(&user_directory.join(".agents/skills"), &retained)? == retained,
        "unrelated installed managed skills have changed; preserve and review them before installation"
    );
    let policy: ops::Policy =
        ops::read_json(&user_directory.join(".config/skill-ops/policy.json"))?;
    ensure!(
        ops::hash(&policy)? == approved.policy_hash,
        "installed policy differs from its managed identity"
    );
    let expected = ops::catalog(&source.join("dot_agents/skills"))?;
    approved.catalog.skills.insert(
        "pull-request".into(),
        expected
            .skills
            .get("pull-request")
            .context("source PR skill is missing")?
            .clone(),
    );
    let reviewed = ops::adoptable_revisions(
        &expected,
        &ops::tracked_decisions(&source.join(ops::DECISIONS))?,
    );
    match reviewed.get("pull-request") {
        Some(skill) => approved
            .reviewed
            .insert("pull-request".into(), skill.clone()),
        None => approved.reviewed.remove("pull-request"),
    };
    let tree = source.join("dot_agents/skills/pull-request");
    let alias = source.join("dot_claude/skills/symlink_pull-request");
    let paths: Vec<_> = crate::skill_source_paths(source)?
        .into_iter()
        .filter(|path| path.starts_with(&tree) || *path == alias)
        .collect();
    ensure!(paths.len() >= 2, "scoped PR skill installation is empty");
    apply(&paths)?;
    ensure!(
        ops::catalog_owned(&user_directory.join(".agents/skills"), &approved.catalog)?
            == approved.catalog,
        "installed PR skill differs from the reviewed source"
    );
    install(binary, user_directory)?;
    approved.catalog_hash = ops::hash(&approved.catalog)?;
    ops::write_json(&manifest, &approved, true)?;
    Ok(())
}
