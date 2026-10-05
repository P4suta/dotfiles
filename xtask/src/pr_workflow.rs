use crate::pr_rules::{
    Checks, Effect, Exclusion, Generation, IssueGate, Operation, ReviewRequirement,
    api_quota_available, checks_state, coderabbit_body_marker_allowed, exclusion, issue_gate,
    keeps_pause_exclusion, plan, requests_review, review_request_allowed, review_requirement,
};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use pulldown_cmark::{Event, Tag, TagEnd};
use serde::Deserialize;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

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
}

#[derive(Args)]
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
    head_ref_oid: Option<String>,
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

fn checks(rollup: &[ReportedCheck]) -> Checks {
    let unfinished = rollup.iter().filter(|check| !check.finished()).count();
    let failed = rollup
        .iter()
        .filter(|check| check.finished() && !check.passed())
        .count();
    checks_state(rollup.len(), unfinished, failed)
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
        ensure!(
            parts.len() == 2
                && parts.iter().all(|part| !part.is_empty()
                    && *part != "."
                    && *part != ".."
                    && !part.starts_with('-')
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))),
            "repository must be an exact OWNER/REPO"
        );
        let Some(path) = &self.title_policy else {
            return Ok(true);
        };
        let policy: TitlePolicy =
            serde_json::from_slice(&fs::read(path).context("read title policy")?)
                .context("title policy requires repository, source, and reason")?;
        ensure!(
            policy.repository.eq_ignore_ascii_case(&self.repo),
            "title policy belongs to another repository"
        );
        let source = policy
            .source
            .strip_prefix("https://")
            .context("title policy source must be an HTTPS rules URL")?;
        ensure!(
            single_line(source)
                && !source.chars().any(char::is_whitespace)
                && source
                    .split('/')
                    .next()
                    .is_some_and(|host| host.contains('.') && !host.starts_with('.'))
                && single_line(&policy.reason),
            "title policy needs a rules source and reason"
        );
        Ok(false)
    }
}

pub fn validate(
    target: &Target,
    title: String,
    body: String,
    final_check: bool,
) -> Result<ValidatedDocument> {
    let required = target.conventional_required()?;
    ensure!(
        single_line(&title) && title.trim() == title,
        "title must be a nonempty single line without surrounding whitespace"
    );
    let request = target.generation == Generation::Coderabbit && !final_check;
    ensure!(
        !unfinished(&title),
        "title contains an unfinished placeholder"
    );
    if title.contains(TITLE_REQUEST) {
        ensure!(
            request && title == TITLE_REQUEST,
            "CodeRabbit title generation is pending; use explicit coderabbit mode only for a generation request"
        );
    } else {
        ensure!(
            !required || conventional(&title),
            "title must use type(scope)!: description; scope and ! are optional (or supply a repository-scoped title policy)"
        );
    }
    let visible = without_comments(&body);
    let visible = visible
        .lines()
        .filter(|line| line.trim() != REVIEW_EXCLUSION)
        .collect::<Vec<_>>()
        .join("\n");
    ensure!(
        !visible.trim().is_empty(),
        "body is empty or contains only comments"
    );
    ensure!(
        !unfinished(&prose(&visible)),
        "body contains an unfinished placeholder"
    );
    if body.contains(TITLE_REQUEST) {
        ensure!(
            body.lines()
                .filter(|line| line.contains(TITLE_REQUEST))
                .all(|line| {
                    coderabbit_body_marker_allowed(
                        request,
                        line.trim() == SUMMARY_REQUEST,
                        line.trim() == REVIEW_EXCLUSION,
                    )
                }),
            "CodeRabbit summary generation is pending or the placeholder is not a standalone standard request"
        );
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
        ensure!(
            policy.schema == 1
                && policy.minimum_remaining > 0
                && !policy.personal_owners.is_empty(),
            "invalid global workflow policy"
        );
        let github = Self { policy };
        let user: Owner = github.read("user").context(
            "authenticated GitHub access is required; use gh auth login --hostname github.com",
        )?;
        ensure!(
            user.id != 0 && single_line(&user.login),
            "invalid authenticated GitHub user"
        );
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
        ensure!(
            api_quota_available(remaining, self.policy.minimum_remaining),
            "GitHub REST quota is low ({remaining} remaining); stop until reset at UTC epoch {reset}; no automatic retry"
        );
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

/// Establish the global personal scope and validate an explicitly selected issue.
fn inspect_issue(github: &Github, target: &Target, body: Option<&str>) -> Result<IssueGate> {
    target.conventional_required()?;
    let repository: Repository = github.read(&format!("repos/{}", target.repo))?;
    ensure!(
        repository.full_name.eq_ignore_ascii_case(&target.repo)
            && repository.owner.id != 0
            && repository
                .owner
                .login
                .eq_ignore_ascii_case(target.repo.split('/').next().unwrap()),
        "repository identity differs from the requested destination"
    );
    let mut personal = false;
    for owner in &github.policy.personal_owners {
        ensure!(
            owner.id != 0 && single_line(&owner.login),
            "invalid personal owner identity"
        );
        let name_matches = owner.login.eq_ignore_ascii_case(&repository.owner.login);
        let id_matches = owner.id == repository.owner.id;
        ensure!(
            name_matches == id_matches,
            "personal owner identity has changed; review the global policy"
        );
        personal |= id_matches;
    }
    let Some(number) = target.issue else {
        return Ok(issue_gate(personal, repository.fork, false));
    };
    let issue: LiveIssue = github.read(&format!("repos/{}/issues/{number}", target.repo))?;
    ensure!(
        issue.number == number
            && issue.html_url.eq_ignore_ascii_case(&format!(
                "https://github.com/{}/issues/{number}",
                target.repo
            ))
            && issue.state == "open"
            && issue.pull_request.is_none(),
        "--issue must identify an open issue in the exact destination, not a PR"
    );
    let issue_body = issue
        .body
        .context("the issue must describe the intended problem and scope")?;
    let visible = without_comments(&issue_body);
    ensure!(
        single_line(&issue.title)
            && !unfinished(&issue.title)
            && !visible.trim().is_empty()
            && !unfinished(&prose(&visible))
            && !issue_body.contains(TITLE_REQUEST),
        "issue content is empty or unfinished"
    );
    if let Some(body) = body {
        ensure!(
            references_issue(body, &target.repo, number),
            "PR body must visibly close the selected issue, for example: Closes #{number}."
        );
    }
    Ok(issue_gate(personal, repository.fork, true))
}

fn require_issue(gate: IssueGate) -> Result<()> {
    ensure!(
        gate != IssueGate::Missing,
        "personal non-fork repositories require --issue NUMBER; record the problem, scope, and acceptance criteria before starting"
    );
    Ok(())
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
    ensure!(
        single_line(value) && !value.starts_with('-') && !value.chars().any(char::is_whitespace),
        "head and base must be nonempty branch names"
    );
    Ok(())
}

fn live_document(_github: &Github, target: &Target, pr: u64) -> Result<LiveDocument> {
    target.conventional_required()?;
    let mut args = arguments("view", &target.repo);
    args.extend([
        pr.to_string().into(),
        "--json".into(),
        "title,body,state,isDraft,statusCheckRollup,headRefOid".into(),
    ]);
    serde_json::from_str(&gh(&args)?).context("invalid gh PR document")
}

pub fn run(action: Action) -> Result<()> {
    match action {
        Action::Start { target } => {
            target.conventional_required()?;
            let github = Github::connect()?;
            let gate = inspect_issue(&github, &target, None)?;
            require_issue(gate)?;
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
                require_issue(inspect_issue(&github, &target, Some(&document.body))?)?;
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
        } => {
            let (target, document) = read_document(document, false)?;
            branch(&head)?;
            if let Some(base) = &base {
                branch(base)?;
            }
            admit_review_request(Operation::Create, target.generation, &document.body, || {
                Ok((true, Exclusion::None))
            })?;
            let github = Github::connect()?;
            let issue = inspect_issue(&github, &target, Some(&document.body))?;
            require_issue(issue)?;
            let effect = plan(
                Operation::Create,
                target.generation,
                true,
                draft,
                issue,
                Checks::Incomplete,
            )
            .context("PR creation refused")?;
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
            let issue = inspect_issue(&github, &target, Some(&document.body))?;
            require_issue(issue)?;
            ensure!(
                plan(
                    Operation::Edit,
                    target.generation,
                    true,
                    false,
                    issue,
                    Checks::Incomplete
                ) == Some(Effect::Edit),
                "PR edit refused"
            );
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
            ensure!(live.state == "OPEN", "only an open PR can become ready");
            let document = validate(&target, live.title, live.body, false)?;
            let issue = inspect_issue(&github, &target, Some(&document.body))?;
            require_issue(issue)?;
            ensure!(live.is_draft, "only a validated draft can become ready");
            ensure!(
                plan(
                    Operation::Ready,
                    target.generation,
                    true,
                    live.is_draft,
                    issue,
                    checks(&live.status_check_rollup),
                ) == Some(Effect::Ready),
                "a draft becomes ready only when every reported check on its head has passed and no work remains"
            );
            crate::hosts::require_admission(
                std::path::Path::new("."),
                live.head_ref_oid
                    .as_deref()
                    .context("GitHub reported no head commit")?,
            )?;
            admit_review_request(Operation::Ready, target.generation, &document.body, || {
                Ok((live.is_draft, review_exclusion(&document.body)))
            })?;
            let mut args = arguments("ready", &target.repo);
            args.push(pr.to_string().into());
            print!("{}", gh(&args)?);
            report_review_requirement(&document.body)?;
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
