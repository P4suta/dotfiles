//! The pull request workflow: issue-scoped preparation, creation, document edits, ready transitions, and stacked branches.

mod context;
mod current_checks;
mod diagram;
mod evidence;
mod figure;
mod figure_migration;
mod link_issue;
mod local;
mod merge;
mod review_policy;
mod screenshot;
mod work;
pub use evidence::store_assessment;
pub use local::Action as LocalAction;
use merge::{await_result as await_merge, submit as submit_merge};

/// Validate the owner-local issue contexts selected by the actual local change.
///
/// # Errors
///
/// Returns a refusal when the source, issue, destination, signature, or path scope differs.
pub fn check_issue_context(root: &Path, base: Option<&str>, staged: bool) -> Result<()> {
    context::check(root, base, staged).map_err(|error| {
        Refusal::new(
            "pr.issue-context",
            "the implementation change lacks a matching checked issue context",
            "pr-workflow start --repo OWNER/REPO --issue NUMBER --path PATH",
        )
        .evidence(format!("{error:#}"))
        .into()
    })
}

/// Read the authenticated review route of the actual PR head without requesting a review.
///
/// # Errors
/// Returns a refusal for an unavailable, unsigned, malformed, stale, or mismatched assessment.
pub fn current_review_route(
    root: &Path,
    repository: &str,
    pr: u64,
) -> Result<rules::review_routing_rules::ReviewRoute> {
    let target = Target {
        repo: repository.into(),
        title_policy: None,
        issue: None,
        cli_receipt: None,
        prior_cli_receipt: None,
    };
    target.conventional_required()?;
    let github = Github::connect()?;
    let live = live_document(&github, &target, pr)?;
    match live.state.as_str() {
        "CLOSED" | "MERGED" => return Ok(rules::review_routing_rules::ReviewRoute::None),
        "OPEN" => {}
        _ => bail!("unknown PR lifecycle while reading its review route"),
    }
    Ok(review_policy::live_binding_at(root, &github, &target, &live.body, &live)?.route)
}

pub use platform::{aside, build_inputs, freshness, refusal, report, tool};
pub use rules::pr_merge_rules;
pub use rules::{pr_ready_rules, pr_rules, wip_rules};

use crate::pr_ready_rules::{Action as Move, Report, Settlement, State, Workflow};
use crate::pr_rules::{
    Blocker, Checks, Dependency, DependencySkip, Effect, Exclusion, IssueGate, Lifecycle,
    MergeState, Operation, PathMerge, Step, Stop, api_quota_available, dependency, dependency_skip,
    head_checks, issue_gate, merge_step, plan,
};
use crate::refusal::{Refusal, command};
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use dotfiles::body_rules::{Verdict, next_action as body_next_action, verdict as body_verdict};
use dotguard::provenance_rules;
use prose::Publication::{Issue as ISSUE, PullRequest as PULL_REQUEST};
use prose::prose_rules::standard_applies;
use pulldown_cmark::{Event, Tag, TagEnd};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

/// The characters of a refused line that a refusal quotes.
const EVIDENCE_PREVIEW: usize = 80;

/// The delimiters of a generation placeholder.
const PLACEHOLDER_OPEN: &str = "{{";
const PLACEHOLDER_CLOSE: &str = "}}";
/// A repository's parts: `OWNER/REPO`.
const REPOSITORY_PARTS: usize = 2;
const MAX_IDENTITY_REPOSITORIES: usize = 2;
const GITHUB: &str = "https://github.com";
/// A closing reference: a keyword such as `closes`, then the issue.
const CLOSING_REFERENCE_WORDS: usize = 2;
const SECONDS_PER_MINUTE: u64 = 60;
/// gh-stack reports a rebase conflict with this exit status.
const STACK_CONFLICT: i32 = 3;
/// A scoped skill installation holds the skill's tree and its Claude link at least.
const SKILL_WITH_ALIAS: usize = 2;

#[derive(Debug, Parser)]
#[command(about = "Validate PR documents before invoking gh; local creation always uses draft")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Plan or apply historical figure attachment migration and verified branch retirement.
    MigrateFigures(figure_migration::Action),
    /// Report the verified route of the actual current PR assessment without requesting review.
    ReviewRoute {
        #[command(flatten)]
        target: Target,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        pr: u64,
    },
    /// Sign an AI importance assessment for the exact current committed PR diff.
    Assess {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        base: String,
        #[arg(long)]
        assessment_file: PathBuf,
        /// Carry the prior actual PR assessment when changing its review route.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        pr: Option<u64>,
    },
    /// Inspect repository scope and the issue before starting GitHub-bound work.
    Start {
        #[command(flatten)]
        target: Target,
        /// Repository-relative files or directories this issue permits implementation to change.
        #[arg(long = "path")]
        paths: Vec<PathBuf>,
        /// Adopt the exact pre-existing baseline approved once in owner-local state.
        #[arg(long)]
        adopt_existing: bool,
        /// Link actual local issue history through this new checked remote authorization.
        #[arg(long, conflicts_with = "adopt_existing")]
        promote_local: bool,
        /// Declare unrelated work and keep its reason in owner-local state.
        #[arg(long, value_name = "REASON", value_parser = work::reason)]
        independent: Option<String>,
    },
    /// Match signed issue contexts to the actual local or CI change.
    ChangeCheck {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, conflicts_with = "staged")]
        base: Option<String>,
        #[arg(long)]
        staged: bool,
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
        /// Also require completed review of the live PR's current head.
        #[arg(long)]
        r#final: bool,
    },
    /// Create a draft with a checked title and description.
    Create {
        #[command(flatten)]
        document: DocumentArgs,
        #[arg(long)]
        head: String,
        #[arg(long)]
        base: Option<String>,
        /// Open an independent work unit and keep its reason in owner-local state.
        #[arg(long, value_name = "REASON", value_parser = work::reason)]
        independent: Option<String>,
    },
    /// Replace both title and body with one checked document.
    Edit {
        #[command(flatten)]
        document: DocumentArgs,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        pr: u64,
    },
    /// Add an issue closing reference as the destination repository owner.
    LinkIssue {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        pr: std::num::NonZeroU64,
        #[arg(long)]
        issue: std::num::NonZeroU64,
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
    /// Delete local branches whose PR merged at their exact head, or whose changes the default branch already holds.
    PruneMerged {
        #[command(flatten)]
        target: Target,
        /// List the branches without deleting them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Create, sync, rebase, and merge stacked PRs through the pinned gh-stack.
    Stack {
        #[command(subcommand)]
        action: StackAction,
    },
    /// Render D2 locally and print its attachment reference with editable source.
    Diagram {
        /// The exact destination repository.
        #[arg(long)]
        repo: String,
        /// The D2 source, which leaves configuration and colors to this command.
        #[arg(long)]
        source: PathBuf,
        /// The image description a reader gets in place of the diagram.
        #[arg(long)]
        alt: String,
    },
    /// Prepare a local screenshot and print its Markdown attachment reference.
    Screenshot {
        /// The exact destination repository.
        #[arg(long)]
        repo: String,
        /// The captured image.
        #[arg(long)]
        file: PathBuf,
        /// What the screenshot shows, for a reader who can't see it.
        #[arg(long)]
        alt: String,
    },
    /// Check, create, or edit an issue through the same document and writing checks.
    Issue {
        #[command(subcommand)]
        action: IssueAction,
    },
    /// Prepare and check owner-local issues without a remote repository or GitHub access.
    Local {
        #[command(subcommand)]
        action: LocalAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum StackAction {
    /// Link an ordered set of existing owned PRs and retire their independent declarations after readback.
    Link {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        base: String,
        #[arg(num_args = 2.., value_parser = clap::value_parser!(u64).range(1..))]
        prs: Vec<u64>,
    },
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
    /// Rebase the stack in the current checkout to resolve a conflict, then continue or stop that rebase.
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

#[derive(Args, Clone, Copy, Debug)]
pub struct Polling {
    /// Seconds between reads while checks run.
    #[arg(long, default_value_t = 30)]
    interval: u64,
    /// Minutes to wait for each PR before refusing.
    #[arg(long, default_value_t = 120)]
    timeout: u64,
}

#[derive(Debug, Subcommand)]
pub enum IssueAction {
    /// Check a local issue title and body without mutation.
    Check {
        #[command(flatten)]
        document: IssueDocument,
    },
    /// Open an issue with the checked title and body.
    Create {
        #[command(flatten)]
        document: IssueDocument,
    },
    /// Replace the title and body of an existing issue.
    Edit {
        #[command(flatten)]
        document: IssueDocument,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        number: u64,
    },
}

#[derive(Args, Debug)]
pub struct IssueDocument {
    /// The repository in exact owner/name form.
    #[arg(long)]
    repo: String,
    #[arg(long)]
    title: String,
    #[arg(long)]
    body_file: PathBuf,
}

impl Commands {
    fn name(&self) -> &'static str {
        match self {
            Self::MigrateFigures(_) => "migrate-figures",
            Self::ReviewRoute { .. } => "review-route",
            Self::Assess { .. } => "assess",
            Self::Start { .. } => "start",
            Self::ChangeCheck { .. } => "change-check",
            Self::Check { .. } => "check",
            Self::Create { .. } => "create",
            Self::Edit { .. } => "edit",
            Self::LinkIssue { .. } => "link-issue",
            Self::Ready { .. } => "ready",
            Self::Merge { .. } => "merge",
            Self::PruneMerged { .. } => "prune-merged",
            Self::Stack { .. } => "stack",
            Self::Issue { .. } => "issue",
            Self::Diagram { .. } => "diagram",
            Self::Screenshot { .. } => "screenshot",
            Self::Local { .. } => "local",
        }
    }
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    /// The exact GitHub repository rather than a URL or an inferred one.
    #[arg(long)]
    pub repo: String,
    /// Repository-scoped JSON exception with repository, source URL, and reason.
    #[arg(long)]
    pub title_policy: Option<PathBuf>,
    /// An existing open issue in the destination repository, required for personal non-forks.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    pub issue: Option<u64>,
    /// Private signed evidence from the guarded committed CLI review of this exact diff.
    #[arg(long)]
    pub cli_receipt: Option<PathBuf>,
    /// Private original receipt required when a prior high assessment changes review route.
    #[arg(long)]
    pub prior_cli_receipt: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct DocumentArgs {
    #[command(flatten)]
    target: Target,
    #[arg(long)]
    title: String,
    #[arg(long)]
    body_file: PathBuf,
}

/// The GitHub Actions directory, whose workflows report checks on a head.
const WORKFLOW_DIRECTORY: &str = ".github/workflows";

/// The entries of one tree at a head, which is null when the path is absent.
const WORKFLOWS_QUERY: &str = "query($owner:String!,$name:String!,$expression:String!){repository(owner:$owner,name:$name){object(expression:$expression){... on Tree{entries{name}}}}}";

#[derive(Deserialize)]
struct WorkflowsResponse {
    data: Option<WorkflowsData>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct WorkflowsData {
    repository: Option<WorkflowsRepository>,
}

#[derive(Deserialize)]
struct WorkflowsRepository {
    object: Option<WorkflowsTree>,
}

#[derive(Deserialize)]
struct WorkflowsTree {
    #[serde(default)]
    entries: Vec<TreeEntry>,
}

#[derive(Deserialize)]
struct TreeEntry {
    name: String,
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
    base_ref_name: Option<String>,
    base_ref_oid: Option<String>,
    #[serde(default)]
    labels: Vec<ReviewLabel>,
    author: ReviewAuthor,
    #[serde(flatten)]
    source: LiveSource,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveSource {
    head_ref_name: Option<String>,
    head_repository: Option<HeadRepository>,
    head_repository_owner: Option<ReviewAuthor>,
    is_cross_repository: Option<bool>,
}

#[derive(Deserialize)]
struct HeadRepository {
    name: String,
}

#[derive(Deserialize)]
struct ReviewAuthor {
    login: String,
    #[serde(default)]
    is_bot: bool,
}

#[derive(Deserialize)]
struct ReviewLabel {
    name: String,
}

/// One entry of the head commit's check rollup: a check run, named by `name`, or a commit status, named by `context`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportedCheck {
    status: Option<String>,
    conclusion: Option<String>,
    state: Option<String>,
    name: Option<String>,
    context: Option<String>,
    /// The address of a check run, whose workflow job number names the run.
    #[serde(default)]
    details_url: Option<String>,
}

/// One rule of a branch's rulesets, of which only the required status checks matter here.
#[derive(Deserialize)]
struct BranchRule {
    r#type: String,
    #[serde(default)]
    parameters: Option<RuleParameters>,
}

#[derive(Deserialize)]
struct RuleParameters {
    #[serde(default)]
    required_status_checks: Vec<RequiredCheck>,
}

#[derive(Deserialize)]
struct RequiredCheck {
    context: String,
}

struct BranchRequirements {
    checks: Vec<String>,
    queue: bool,
}

impl ReportedCheck {
    fn label(&self) -> Option<&str> {
        self.name.as_deref().or(self.context.as_deref())
    }

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

    fn optional_skip(&self, required: &[String]) -> bool {
        self.finished()
            && self.conclusion.as_deref() == Some("SKIPPED")
            && self.label().is_some_and(|label| {
                !label.is_empty() && !required.iter().any(|name| name == label)
            })
    }
}

/// Counts the checks, unfinished or failed results, and missing required evidence.
/// Completed optional skips supply no evidence.
fn tally(rollup: &[ReportedCheck], required: &[String]) -> (usize, usize, usize, usize) {
    let total = rollup
        .iter()
        .filter(|check| !check.optional_skip(required))
        .count();
    let unfinished = rollup.iter().filter(|check| !check.finished()).count();
    let failed = rollup
        .iter()
        .filter(|check| check.finished() && !check.passed() && !check.optional_skip(required))
        .count();
    let missing = required
        .iter()
        .filter(|name| {
            !rollup.iter().any(|check| {
                check.label() == Some(name.as_str()) && check.finished() && check.passed()
            })
        })
        .count();
    (total, unfinished, failed, missing)
}

/// The report of a head's rollup, given the checks its base requires.
fn checks(rollup: &[ReportedCheck], required: &[String]) -> Report {
    let (_, unfinished, failed, missing) = tally(rollup, required);
    pr_ready_rules::report(unfinished, failed, missing)
}

#[cfg(test)]
mod check_reports {
    use super::{Report, ReportedCheck, checks};

    fn run(name: Option<&str>, conclusion: &str) -> ReportedCheck {
        ReportedCheck {
            status: Some("COMPLETED".into()),
            conclusion: Some(conclusion.into()),
            state: None,
            name: name.map(str::to_owned),
            context: None,
            details_url: None,
        }
    }

    #[test]
    fn a_named_optional_skip_preserves_a_complete_required_report() {
        let rollup = [
            run(Some("required"), "SUCCESS"),
            run(Some("build-test"), "SKIPPED"),
        ];
        assert_eq!(checks(&rollup, &["required".into()]), Report::Complete);
    }

    #[test]
    fn optional_skips_leave_no_required_ready_work() {
        assert_eq!(
            checks(&[run(Some("build-test"), "SKIPPED")], &[]),
            Report::Complete
        );
    }

    #[test]
    fn required_and_unnamed_skips_still_block() {
        for name in [Some("required"), Some(""), None] {
            let rollup = [run(Some("required"), "SUCCESS"), run(name, "SKIPPED")];
            assert_eq!(checks(&rollup, &["required".into()]), Report::Failed);
        }
    }

    #[test]
    fn failed_cancelled_and_unknown_conclusions_still_block() {
        for conclusion in [
            "FAILURE",
            "CANCELLED",
            "TIMED_OUT",
            "ACTION_REQUIRED",
            "STALE",
            "STARTUP_FAILURE",
            "UNKNOWN",
            "skipped",
        ] {
            let rollup = [
                run(Some("required"), "SUCCESS"),
                run(Some("build-test"), conclusion),
            ];
            assert_eq!(
                checks(&rollup, &["required".into()]),
                Report::Failed,
                "{conclusion}"
            );
        }
    }

    #[test]
    fn optional_skips_never_hide_unfinished_or_missing_required_checks() {
        let mut pending = run(Some("build-test"), "SKIPPED");
        pending.status = Some("IN_PROGRESS".into());
        assert_eq!(
            checks(
                &[run(Some("required"), "SUCCESS"), pending],
                &["required".into()]
            ),
            Report::Pending
        );
        assert_eq!(
            checks(
                &[
                    run(Some("other"), "SUCCESS"),
                    run(Some("build-test"), "SKIPPED")
                ],
                &["required".into()]
            ),
            Report::Partial
        );
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowPolicy {
    schema: u8,
    minimum_remaining: u64,
    personal_owners: Vec<Owner>,
    work_in_progress: WorkInProgress,
}

/// The number of open PRs one person may keep in a repository before new work joins an open PR instead.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkInProgress {
    default: u32,
    /// Overrides by `owner/name`, each recorded with the reason it differs.
    #[serde(default)]
    repositories: std::collections::BTreeMap<String, RepositoryLimit>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryLimit {
    limit: u32,
    reason: String,
}

impl WorkInProgress {
    fn limit(&self, repository: &str) -> u32 {
        crate::wip_rules::limit(
            self.default,
            self.repositories
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(repository))
                .map(|(_, recorded)| recorded.limit),
        )
    }
}

struct Github {
    policy: WorkflowPolicy,
    login: String,
    user_id: u64,
    repositories: RefCell<BTreeMap<String, Repository>>,
}

#[derive(Deserialize)]
struct Owner {
    login: String,
    id: u64,
}

#[derive(Clone, Copy)]
struct Destination {
    personal: bool,
    fork: bool,
    repository_id: u64,
    owner_id: u64,
}

fn workflow_policy() -> Result<WorkflowPolicy> {
    let policy: WorkflowPolicy = serde_json::from_str(include_str!(
        "../../../home/dot_agents/skills/pull-request/assets/workflow-policy.json"
    ))?;
    require(
        policy.schema == 1
            && policy.minimum_remaining > 0
            && !policy.personal_owners.is_empty()
            && policy.work_in_progress.default > 0
            && policy
                .work_in_progress
                .repositories
                .values()
                .all(|recorded| recorded.limit > 0 && single_line(&recorded.reason)),
        || {
            Refusal::new(
                "pr.policy",
                "the installed workflow policy needs schema 1, a nonzero quota reserve, at least one personal owner, and nonzero work-in-progress limits whose overrides each state a reason",
                "git ls-files home/dot_agents/skills/pull-request/assets/workflow-policy.json",
            )
            .evidence(format!(
                "schema {}, reserve {}, {} personal owner(s), work-in-progress default {}",
                policy.schema,
                policy.minimum_remaining,
                policy.personal_owners.len(),
                policy.work_in_progress.default
            ))
        },
    )?;
    Ok(policy)
}

type Repository = platform::source_identity::Repository;

/// An open PR as `gh pr list` reports it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPr {
    number: u64,
    head_ref_name: String,
    base_ref_name: String,
    author: Option<DependencyAuthor>,
    mergeable: Option<String>,
    #[serde(default)]
    is_cross_repository: bool,
}

#[derive(Deserialize)]
struct DependencyAuthor {
    login: String,
    #[serde(default)]
    is_bot: bool,
    #[serde(rename = "type")]
    author_type: Option<String>,
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
    base_ref_oid: Option<String>,
    #[serde(default)]
    labels: Vec<ReviewLabel>,
    author: ReviewAuthor,
    #[serde(flatten)]
    source: LiveSource,
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

#[derive(Debug)]
pub struct ValidatedDocument {
    title: String,
    body: String,
    attachments: Vec<PathBuf>,
}

/// Refuse a title or body that carries a line claiming authorship for an agent.
/// The refusal names the line and the next action.
fn refuse_attribution(part: &str, text: &str, recheck: impl FnOnce() -> String) -> Result<()> {
    let found = dotguard::attribution::find(text);
    if let Verdict::Refuse(reason) = body_verdict(found.is_some()) {
        let (number, line) = found.context("a refused document names its attribution line")?;
        return Err(Refusal::new(
            "pr.attribution",
            format!(
                "{part} line {number} is AI attribution. {}",
                body_next_action(reason)
            ),
            recheck(),
        )
        .evidence(format!("{part} line {number}: {line}"))
        .into());
    }
    Ok(())
}

/// Refuse a title or body that says how its text came about instead of describing the change.
fn refuse_provenance(part: &str, text: &str, recheck: impl FnOnce() -> String) -> Result<()> {
    let found = provenance_rules::find(text);
    if provenance_rules::verdict(found.is_some()) == provenance_rules::Verdict::Refuse {
        let (number, phrase) = found.context("a refused document names its phrase")?;
        return Err(Refusal::new(
            "pr.provenance",
            format!(
                "{part} line {number} says how its text came about. {}",
                provenance_rules::NEXT_ACTION
            ),
            recheck(),
        )
        .evidence(format!("{part} line {number}: {phrase}"))
        .into());
    }
    Ok(())
}

/// Refuses a document that carries workflow evidence, which stays in owner-local state.
fn refuse_local_evidence(body: &str, recheck: impl FnOnce() -> String) -> Result<()> {
    for (number, line) in body.lines().enumerate() {
        let line = line.trim();
        if line.starts_with(review_policy::PACKET_PREFIX) {
            return Err(Refusal::new(
                "pr.local-evidence",
                "published documents can't carry local workflow evidence; delete the line, because pr-workflow keeps assessments and pause exclusions in owner-local state",
                recheck(),
            )
            .evidence(format!(
                "body line {}: {}",
                number + 1,
                line.chars().take(EVIDENCE_PREVIEW).collect::<String>()
            ))
            .into());
        }
    }
    Ok(())
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
    }) || lower.match_indices(PLACEHOLDER_OPEN).any(|(start, _)| {
        !lower[..start].ends_with('$')
            && lower[start + PLACEHOLDER_OPEN.len()..]
                .split_once(PLACEHOLDER_CLOSE)
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
            parts.len() == REPOSITORY_PARTS
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

/// The refusal rule for a head that no check can ever report on.
const ABSENT_CHECKS_RULE: &str = "pr.checks-absent";

/// Why a head without a declared check can't merge.
const ABSENT_CHECKS_CAUSE: &str = "no check reported on the head, the base requires none, and the head carries no workflow under .github/workflows, so none can ever report; add a check";

/// The local check to run on a corrected document.
/// It keeps `title` unless the title caused the failure.
fn recheck(target: &Target, title: Option<&str>) -> String {
    let mut words = vec!["pr-workflow", "check", "--repo", &target.repo];
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
/// `arguments` holds the words after `pr-workflow`, whichever entry point received them.
///
/// # Errors
///
/// Returns a refusal when `action` fails, either its own or one that names the failure and the command to rerun.
pub fn execute(action: Commands, arguments: &[String]) -> Result<()> {
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

pub(crate) fn validate(target: &Target, title: String, body: String) -> Result<ValidatedDocument> {
    let required = target.conventional_required()?;
    let title_refusal = |cause: &str| {
        Refusal::new("pr.title", cause, recheck(target, None)).evidence(format!("title: {title:?}"))
    };
    require(single_line(&title) && title.trim() == title, || {
        title_refusal("title must be a nonempty single line without surrounding whitespace")
    })?;
    require(!unfinished(&title), || {
        title_refusal("title contains an unfinished placeholder")
    })?;
    require(!required || conventional(&title), || {
        title_refusal(
            "title must use type(scope)!: description; scope and ! are optional (or supply a repository-scoped title policy)",
        )
    })?;
    refuse_attribution("title", &title, || recheck(target, None))?;
    refuse_attribution("body", &body, || recheck(target, Some(&title)))?;
    refuse_provenance("title", &title, || recheck(target, None))?;
    refuse_provenance("body", &body, || recheck(target, Some(&title)))?;
    refuse_local_evidence(&body, || recheck(target, Some(&title)))?;
    let body_refusal = |cause: &str, evidence: String| {
        Refusal::new("pr.body", cause, recheck(target, Some(&title))).evidence(evidence)
    };
    let visible = without_comments(&body);
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
    Ok(ValidatedDocument {
        title,
        body,
        attachments: Vec::new(),
    })
}

fn read_document(document: DocumentArgs) -> Result<(Target, ValidatedDocument)> {
    let body = fs::read_to_string(&document.body_file).context("read UTF-8 PR body file")?;
    let mut validated = validate(&document.target, document.title, body)?;
    figure::prepare_document(&mut validated, &document.body_file)?;
    Ok((document.target, validated))
}

/// Refuses a title or body that fails the installed prose checker when the destination takes the personal writing standard.
fn written_to_standard(
    destination: Destination,
    kind: prose::Publication,
    document: &ValidatedDocument,
) -> Result<()> {
    if !standard_applies(destination.personal, destination.fork) {
        return Ok(());
    }
    prose::check_publication(
        &prose::Bundle::installed()?,
        kind,
        &document.title,
        &document.body,
    )
}

/// Applies the personal writing standard without requiring an image for every change.
fn authored_to_standard(
    destination: Destination,
    _repo: &str,
    kind: prose::Publication,
    document: &ValidatedDocument,
) -> Result<()> {
    written_to_standard(destination, kind, document)
}

/// A figure to prepare for attachment to a document.
enum Figure<'a> {
    Diagram(&'a Path),
    Screenshot(&'a Path),
}

/// Prepares a local figure without creating a GitHub branch.
fn store_figure(repo: &str, figure: &Figure<'_>, alt: &str) -> Result<()> {
    offline_destination(repo)?;
    let line = match figure {
        Figure::Diagram(source) => diagram::prepare(source, alt)?,
        Figure::Screenshot(file) => screenshot::prepare(file, alt)?,
    };
    println!("{line}");
    Ok(())
}

fn offline_destination(repo: &str) -> Result<Destination> {
    let policy = workflow_policy()?;
    let owner = repo.split('/').next().unwrap_or_default();
    Ok(Destination {
        personal: policy
            .personal_owners
            .iter()
            .any(|personal| personal.login.eq_ignore_ascii_case(owner)),
        fork: false,
        repository_id: 0,
        owner_id: 0,
    })
}

/// Validates an issue title and body with the checks a PR document receives, apart from title syntax.
fn validate_issue(document: &IssueDocument) -> Result<ValidatedDocument> {
    let target = Target {
        repo: document.repo.clone(),
        title_policy: None,
        issue: None,
        cli_receipt: None,
        prior_cli_receipt: None,
    };
    target.conventional_required()?;
    let body = fs::read_to_string(&document.body_file).context("read UTF-8 issue body file")?;
    let mut validated = validate_issue_contents(&document.title, body)?;
    figure::prepare_document(&mut validated, &document.body_file)?;
    Ok(validated)
}

fn validate_issue_contents(title: &str, body: String) -> Result<ValidatedDocument> {
    ensure!(
        single_line(title) && title.trim() == title && !unfinished(title),
        "the issue title must be a finished single line without surrounding whitespace"
    );
    let issue_check = || {
        command(&[
            "pr-workflow",
            "issue",
            "check",
            "--repo",
            "OWNER/REPO",
            "--title",
            title,
            "--body-file",
            "<body-file>",
        ])
    };
    refuse_provenance("title", title, issue_check)?;
    refuse_provenance("body", &body, issue_check)?;
    refuse_local_evidence(&body, issue_check)?;
    let visible = without_comments(&body);
    ensure!(
        !visible.trim().is_empty() && !unfinished(&prose(&visible)),
        "the issue body is empty, holds only comments, or contains an unfinished placeholder"
    );
    Ok(ValidatedDocument {
        title: title.to_owned(),
        body,
        attachments: Vec::new(),
    })
}

fn gh(args: &[OsString]) -> Result<String> {
    let output = Tool::Gh
        .command()
        .env("GH_HOST", "github.com")
        .args(args)
        .output()
        .context("start gh")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let publication = stdout
        .lines()
        .find(|line| {
            line.starts_with(&format!("{GITHUB}/"))
                && (line.contains("/pull/") || line.contains("/issues/"))
                && !line.chars().any(char::is_whitespace)
        })
        .unwrap_or("none");
    ensure!(
        output.status.success(),
        "gh failed with {}: {}; publication URL: {}; inspect GitHub before retrying an uncertain mutation",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim(),
        publication
    );
    String::from_utf8(output.stdout).context("gh returned non-UTF-8 output")
}

impl Github {
    /// Require a live authenticated user without printing or transferring credentials.
    fn connect() -> Result<Self> {
        let mut github = Self {
            policy: workflow_policy()?,
            login: String::new(),
            user_id: 0,
            repositories: RefCell::default(),
        };
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
        github.login = user.login;
        github.user_id = user.id;
        Ok(github)
    }

    fn repository(&self, repository: &str) -> Result<Repository> {
        let key = repository.to_ascii_lowercase();
        if let Some(metadata) = self.repositories.borrow().get(&key) {
            return Ok(metadata.clone());
        }
        ensure!(
            self.repositories.borrow().len() < MAX_IDENTITY_REPOSITORIES,
            "one PR operation may inspect only its source and destination repository identities"
        );
        let metadata: Repository = self.read(&format!("repos/{repository}"))?;
        self.repositories.borrow_mut().insert(key, metadata.clone());
        Ok(metadata)
    }

    /// Whether `head` carries a GitHub Actions workflow, which can still report a check after a push.
    /// The Git object query returns null for a missing directory, so the read needs no not-found status.
    fn carries_workflow(&self, target: &Target, head: &str) -> Result<bool> {
        let (owner, name) = target
            .repo
            .split_once('/')
            .context("repository identity missing")?;
        let response: WorkflowsResponse = self.graphql(
            WORKFLOWS_QUERY,
            &serde_json::json!({
                "owner": owner,
                "name": name,
                "expression": format!("{head}:{WORKFLOW_DIRECTORY}"),
            }),
        )?;
        ensure!(
            response.errors.is_empty(),
            "GitHub could not read the workflows of {head}"
        );
        let repository = response
            .data
            .and_then(|data| data.repository)
            .context("GitHub reported no repository for the workflow read")?;
        Ok(repository.object.is_some_and(|tree| {
            tree.entries.iter().any(|entry| {
                Path::new(&entry.name)
                    .extension()
                    .is_some_and(|extension| extension == "yml" || extension == "yaml")
            })
        }))
    }

    /// The status checks that the rulesets of `base` require, by the name each check reports.
    fn required_checks(&self, target: &Target, base: &str) -> Result<Vec<String>> {
        Ok(self.branch_requirements(target, base)?.checks)
    }

    fn branch_requirements(&self, target: &Target, base: &str) -> Result<BranchRequirements> {
        let rules: Vec<BranchRule> = self.read(&format!(
            "repos/{}/rules/branches/{}",
            target.repo,
            review_policy::path_segment(base)?
        ))?;
        let queue = rules.iter().any(|rule| rule.r#type == "merge_queue");
        let checks = rules
            .into_iter()
            .filter(|rule| rule.r#type == "required_status_checks")
            .filter_map(|rule| rule.parameters)
            .flat_map(|parameters| parameters.required_status_checks)
            .map(|check| check.context)
            .collect();
        Ok(BranchRequirements { checks, queue })
    }

    /// Inspect live REST quota headers on each serialized prerequisite request.
    fn read<T: serde::de::DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        let args = ["api", endpoint, "--hostname", "github.com", "--include"].map(OsString::from);
        self.response(&gh(&args)?, "core")
    }

    fn graphql<T: serde::de::DeserializeOwned>(
        &self,
        query: &str,
        variables: &serde_json::Value,
    ) -> Result<T> {
        let mut input = tempfile::NamedTempFile::new()?;
        serde_json::to_writer(
            input.as_file_mut(),
            &serde_json::json!({"query":query,"variables":variables}),
        )?;
        input.as_file().sync_all()?;
        let args = [
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            "github.com".into(),
            "--include".into(),
            "--input".into(),
            input.path().as_os_str().to_owned(),
        ];
        self.response(&gh(&args)?, "graphql")
    }

    fn response<T: serde::de::DeserializeOwned>(
        &self,
        response: &str,
        resource: &str,
    ) -> Result<T> {
        self.response_with_status(response, resource, &["200"])
    }

    fn response_with_status<T: serde::de::DeserializeOwned>(
        &self,
        response: &str,
        resource: &str,
        statuses: &[&str],
    ) -> Result<T> {
        let response = response.replace("\r\n", "\n");
        let (headers, body) = response
            .split_once("\n\n")
            .context("missing GitHub response headers")?;
        ensure!(
            headers
                .lines()
                .next()
                .is_some_and(|line| line.starts_with("HTTP/")
                    && line
                        .split_whitespace()
                        .nth(1)
                        .is_some_and(|status| statuses.contains(&status))),
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
            header("x-ratelimit-resource")? == resource && reset > 0,
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

/// Recognize a closing reference in visible Markdown outside examples and comments.
fn references_issue(body: &str, repo: &str, issue: u64) -> bool {
    closing_references(body, repo).contains(&issue)
}

/// The issues of `repo` that visible Markdown outside examples and comments closes, in document order.
fn closing_references(body: &str, repo: &str) -> Vec<u64> {
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
    let url = format!("{GITHUB}/{repo}/issues/");
    let words: Vec<_> = text
        .split_whitespace()
        .map(|word| word.trim_matches([',', '.', ':', ';', '!', '(', ')', '[', ']']))
        .collect();
    words
        .windows(CLOSING_REFERENCE_WORDS)
        .filter(|pair| {
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
            )
        })
        .filter_map(|pair| {
            let digits = pair[1].strip_prefix('#').or_else(|| {
                pair[1]
                    .get(..url.len())
                    .filter(|prefix| prefix.eq_ignore_ascii_case(&url))
                    .and_then(|_| pair[1].get(url.len()..))
            })?;
            let issue: u64 = digits.parse().ok()?;
            (issue.to_string() == digits).then_some(issue)
        })
        .collect()
}

#[cfg(test)]
mod closing_reference_parsing {
    use super::{closing_references, references_issue};

    const REPO: &str = "P4suta/project";

    #[test]
    fn visible_references_keep_their_document_order() {
        let body = "Fixes #56.\n\nAlso resolves https://GitHub.com/p4suta/PROJECT/issues/12 and closes #57.\n";
        assert_eq!(closing_references(body, REPO), [56, 12, 57]);
        assert!(references_issue(body, REPO, 12));
        assert!(!references_issue(body, REPO, 5));
    }

    #[test]
    fn noncanonical_foreign_quoted_and_commented_references_close_nothing() {
        for body in [
            "Closes #023.",
            "Closes #+5.",
            "Closes #.",
            "Closes other/project#5.",
            "Closes https://github.com/other/project/issues/5.",
            "> Closes #5.",
            "<!-- Closes #5. -->",
            "See #5.",
        ] {
            assert_eq!(closing_references(body, REPO), Vec::<u64>::new(), "{body}");
        }
    }
}

/// Where a checked PR body lives, so a refusal names the edit that changes it.
#[derive(Clone, Copy)]
enum BodySource<'a> {
    File(&'a Path),
    Pr(u64),
}

/// Establish the global personal scope of a repository.
fn destination(github: &Github, repo: &str) -> Result<Destination> {
    let repository = github.repository(repo)?;
    require(
        repository.full_name.eq_ignore_ascii_case(repo)
            && repository.owner.id != 0
            && repository
                .owner
                .login
                .eq_ignore_ascii_case(repo.split('/').next().unwrap()),
        || {
            Refusal::new(
                "pr.repository",
                "repository identity differs from the requested destination",
                command(&["gh", "repo", "view", repo, "--json", "nameWithOwner,owner"]),
            )
            .evidence(format!(
                "requested {repo}, GitHub reports {} owned by {} ({})",
                repository.full_name, repository.owner.login, repository.owner.id
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
                    "a personal owner in the workflow policy no longer matches GitHub's login and id; the user must review the policy",
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
    Ok(Destination {
        personal,
        fork: repository.fork,
        repository_id: repository.id,
        owner_id: repository.owner.id,
    })
}

fn source_identity(
    github: &Github,
    root: &Path,
    target: &str,
) -> Result<platform::source_identity::Checked> {
    platform::source_identity::capture(root, target, |repository| github.repository(repository))
        .map_err(source_identity_error)
}

fn source_identity_error(error: anyhow::Error) -> anyhow::Error {
    let Some(changed) = error.downcast_ref::<platform::source_identity::OriginChanged>() else {
        return error;
    };
    Refusal::new(
        "pr.stack",
        "checkout source changed while its destination was being verified; retain the original source before repeating the operation",
        command(&["git", "remote", "get-url", "origin"]),
    )
    .evidence(format!("verified source: {}", changed.expected()))
    .evidence(format!("actual source: {}", changed.actual()))
    .into()
}

fn require_current_source(source: &platform::source_identity::Checked) -> Result<()> {
    source.require_current().map_err(source_identity_error)
}

/// Validate an explicitly selected open issue in a destination with a known personal scope.
/// `rerun` names the refused command, which a body-file refusal repeats after the edit.
fn inspect_issue(
    github: &Github,
    target: &Target,
    destination: Destination,
    body: Option<(&str, BodySource)>,
    rerun: &str,
) -> Result<IssueGate> {
    inspect_issue_for(github, target, destination, body, rerun, false)
}

/// Validate an explicitly selected issue, and admit a closed one only for a merged PR whose body still closes it.
/// A merged PR closes its issue, so a correction to its document names the closed issue.
fn inspect_issue_for(
    github: &Github,
    target: &Target,
    destination: Destination,
    body: Option<(&str, BodySource)>,
    rerun: &str,
    merged: bool,
) -> Result<IssueGate> {
    target.conventional_required()?;
    let Some(number) = target.issue else {
        return Ok(issue_gate(destination.personal, destination.fork, false));
    };
    let issue: LiveIssue = github.read(&format!("repos/{}/issues/{number}", target.repo))?;
    require(
        issue.number == number
            && issue
                .html_url
                .eq_ignore_ascii_case(&format!("{GITHUB}/{}/issues/{number}", target.repo))
            && (issue.state == "open" || merged && issue.state == "closed" && body.is_some())
            && issue.pull_request.is_none(),
        || {
            Refusal::new(
                "pr.issue",
                "--issue must identify an open issue in the exact destination, or the closed issue that a merged PR closes, not a PR",
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
            && !unfinished(&prose(&visible)),
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
    Ok(issue_gate(destination.personal, destination.fork, true))
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

/// Plans `operation` on a validated document, and a refusal names the unmet condition.
/// `pr` names the existing PR that a ready transition acts on.
fn planned(
    operation: Operation,
    target: &Target,
    pr: Option<u64>,
    draft: bool,
    issue: IssueGate,
    checks: Checks,
) -> Result<Effect> {
    let blocker = match plan(operation, true, draft, issue, checks) {
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
    for attachment in &document.attachments {
        args.extend(["--attach".into(), attachment.as_os_str().to_owned()]);
    }
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
        "title,body,state,isDraft,statusCheckRollup,headRefOid,baseRefName,baseRefOid,labels,author,headRefName,headRepository,headRepositoryOwner,isCrossRepository".into(),
    ]);
    let mut live: LiveDocument =
        serde_json::from_str(&gh(&args)?).context("invalid gh PR document")?;
    if let Some(head) = &live.head_ref_oid {
        current_checks::retain_current(&target.repo, head, &mut live.status_check_rollup);
    }
    Ok(live)
}

fn start_implementation(
    target: &Target,
    paths: &[PathBuf],
    adopt_existing: bool,
    promote_local: bool,
    independent: Option<&str>,
    rerun: &str,
) -> Result<()> {
    target.conventional_required()?;
    let github = Github::connect()?;
    let destination = destination(&github, &target.repo)?;
    let gate = inspect_issue(&github, target, destination, None, rerun)?;
    require_issue(gate, target)?;
    let head = git_output(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let work = work::prepare(
        &github,
        target,
        head.trim(),
        None,
        independent,
        work::Operation::Start,
    )?;
    context::start(
        &github,
        target,
        destination,
        paths,
        adopt_existing,
        promote_local,
    )?;
    work.started()?;
    println!("Checked implementation context saved ({gate:?})");
    Ok(())
}

fn run(action: Commands, rerun: &str) -> Result<()> {
    match action {
        Commands::ReviewRoute { target, pr } => {
            let route = current_review_route(&std::env::current_dir()?, &target.repo, pr)?;
            let route = match route {
                rules::review_routing_rules::ReviewRoute::None => "none",
                rules::review_routing_rules::ReviewRoute::Cli => "cli",
                rules::review_routing_rules::ReviewRoute::Pr => "pr",
            };
            println!("{}", serde_json::json!({"route":route}));
        }
        Commands::Assess {
            target,
            base,
            assessment_file,
            pr,
        } => {
            target.conventional_required()?;
            let github = Github::connect()?;
            review_policy::assess(&github, &target, &base, &assessment_file, pr)?;
        }
        Commands::Start {
            target,
            paths,
            adopt_existing,
            promote_local,
            independent,
        } => start_implementation(
            &target,
            &paths,
            adopt_existing,
            promote_local,
            independent.as_deref(),
            rerun,
        )?,
        Commands::ChangeCheck { root, base, staged } => {
            check_issue_context(&root, base.as_deref(), staged)?;
        }
        Commands::Check {
            target,
            title,
            body_file,
            pr,
            r#final,
        } => {
            check_command(target, title, body_file, pr, r#final, rerun)?;
            println!(
                "PR document accepted; local-file checks do not inspect issues and validation claims still require execution evidence"
            );
        }
        Commands::Create {
            document,
            head,
            base,
            independent,
        } => create(
            document,
            &head,
            base.as_deref(),
            &Creation::Regular { independent },
            rerun,
        )?,
        Commands::Edit { document, pr } => edit(document, pr, rerun)?,
        Commands::LinkIssue { repo, pr, issue } => link_issue::run(&repo, pr, issue)?,
        Commands::Ready { target, pr } => ready_command(&target, pr, rerun)?,
        Commands::Merge {
            target,
            prs,
            polling,
        } => {
            target.conventional_required()?;
            let github = Github::connect()?;
            let open = open_prs(&target)?;
            for &pr in &prs {
                require_unstacked(&target, &open, pr)?;
            }
            for pr in prs {
                merge_independent(&github, &target, pr, polling, rerun)?;
            }
        }
        Commands::PruneMerged { target, dry_run } => prune_merged(&target, dry_run)?,
        Commands::MigrateFigures(action) => figure_migration::run(&action)?,
        Commands::Stack { action } => stack(action, rerun)?,
        Commands::Issue { action } => issue(action)?,
        Commands::Diagram { repo, source, alt } => {
            store_figure(&repo, &Figure::Diagram(&source), &alt)?;
        }
        Commands::Screenshot { repo, file, alt } => {
            store_figure(&repo, &Figure::Screenshot(&file), &alt)?;
        }
        Commands::Local { action } => local::run(action)?,
    }
    Ok(())
}

fn check_command(
    target: Target,
    title: Option<String>,
    body_file: Option<PathBuf>,
    pr: Option<u64>,
    r#final: bool,
    rerun: &str,
) -> Result<()> {
    if let Some(pr) = pr {
        return check_live(&target, pr, r#final, rerun);
    }
    let destination = offline_destination(&target.repo)?;
    let (target, document) = read_document(DocumentArgs {
        target,
        title: title.context("title required")?,
        body_file: body_file.context("body file required")?,
    })?;
    authored_to_standard(destination, &target.repo, PULL_REQUEST, &document)?;
    println!(
        "Offline document format accepted; no issue, source, classification, or review credit"
    );
    Ok(())
}

fn ready_command(target: &Target, pr: u64, rerun: &str) -> Result<()> {
    target.conventional_required()?;
    let github = Github::connect()?;
    let live = live_document(&github, target, pr)?;
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
    let (Some(base), Some(head)) = (live.base_ref_name.as_deref(), live.head_ref_oid.as_deref())
    else {
        bail!("GitHub reported no base branch or no head commit");
    };
    let required = github.required_checks(target, base)?;
    let body = make_ready(
        &github,
        target,
        pr,
        live.title,
        live.body,
        live.is_draft,
        checks(&live.status_check_rollup, &required),
        head,
        rerun,
    )?;
    let live = live_document(&github, target, pr)?;
    let checked = review_policy::live_binding(&github, target, &body, &live)?;
    review_policy::describe(&checked)
}

/// Checks the current GitHub document of `pr` and the issue it links.
fn check_live(target: &Target, pr: u64, r#final: bool, rerun: &str) -> Result<()> {
    target.conventional_required()?;
    let github = Github::connect()?;
    let live = live_document(&github, target, pr)?;
    let checked = review_policy::live_binding(&github, target, &live.body, &live)?;
    review_policy::remember(&checked, target, &github, pr)?;
    let document = validate(target, live.title, live.body)?;
    let destination = destination(&github, &target.repo)?;
    written_to_standard(destination, PULL_REQUEST, &document)?;
    require_issue(
        inspect_issue(
            &github,
            target,
            destination,
            Some((&document.body, BodySource::Pr(pr))),
            rerun,
        )?,
        target,
    )?;
    if r#final {
        review_policy::completion(&checked, target, &github, pr)?;
    }
    review_policy::describe(&checked)
}

enum Creation {
    Regular { independent: Option<String> },
    Stack,
}

struct CreationStack {
    parents: Vec<u64>,
    trunk: String,
}

impl CreationStack {
    fn select(target: &Target, work: &work::Plan, creation: &Creation) -> Result<Self> {
        let (parents, trunk) = if work.stacked() || matches!(creation, Creation::Stack) {
            chain(&open_prs(target)?, work.base())
        } else {
            (Vec::new(), work.base().to_owned())
        };
        require(
            !(work.stacked() || matches!(creation, Creation::Stack)) || !parents.is_empty(),
            || {
                Refusal::new(
                    "pr.stack",
                    "the selected base has no open stack parent",
                    command(&["gh", "pr", "list", "--repo", &target.repo]),
                )
                .evidence(format!("selected base {}", work.base()))
            },
        )?;
        Ok(Self { parents, trunk })
    }

    fn published(&self, target: &Target, number: u64) -> Result<()> {
        if !self.parents.is_empty() {
            let mut arguments = vec!["link".to_owned(), "--base".to_owned(), self.trunk.clone()];
            arguments.extend(self.parents.iter().map(u64::to_string));
            arguments.push(number.to_string());
            gh_stack(
                target,
                None,
                &arguments.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
        }
        Ok(())
    }
}

fn create(
    document: DocumentArgs,
    head: &str,
    base: Option<&str>,
    creation: &Creation,
    rerun: &str,
) -> Result<()> {
    let body_file = document.body_file.clone();
    let title = document.title.clone();
    let (target, document) = read_document(document)?;
    branch(head)?;
    if let Some(base) = &base {
        branch(base)?;
    }
    let github = Github::connect()?;
    let independent = match creation {
        Creation::Regular { independent } => independent.as_deref(),
        Creation::Stack => None,
    };
    let work = work::prepare(
        &github,
        &target,
        head,
        base,
        independent,
        if matches!(creation, Creation::Stack) {
            work::Operation::StackCreate
        } else {
            work::Operation::Create
        },
    )?;
    let stack = CreationStack::select(&target, &work, creation)?;
    let checked =
        review_policy::create_binding(&github, &target, &document.body, head, Some(work.base()))?;
    let head = review_policy::qualified_head(&checked, head)?;
    review_policy::admit_publication(&checked)?;
    let destination = destination(&github, &target.repo)?;
    authored_to_standard(destination, &target.repo, PULL_REQUEST, &document)?;
    let issue = inspect_issue(
        &github,
        &target,
        destination,
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
    refuse_dependent(
        &github,
        &target,
        &head,
        Some(work.base()),
        &title,
        &body_file,
        &stack.parents,
    )?;
    let mut args = arguments("create", &target.repo);
    args.extend([OsString::from("--head"), head.into()]);
    args.extend([OsString::from("--base"), work.base().into()]);
    args.push("--draft".into());
    let _body = write_document(&mut args, &document)?;
    work.before_publication()?;
    let created = gh(&args)?;
    print!("{created}");
    let number = created_number(&created)?;
    figure::verify_publication(&document, "pr", &target.repo, number)?;
    review_policy::remember(&checked, &target, &github, number)?;
    stack.published(&target, number)?;
    work.published(number)
}

fn edit(document: DocumentArgs, pr: u64, rerun: &str) -> Result<()> {
    let body_file = document.body_file.clone();
    let (target, document) = read_document(document)?;
    let github = Github::connect()?;
    let live = live_document(&github, &target, pr)?;
    let checked = review_policy::live_binding(&github, &target, &document.body, &live)?;
    review_policy::admit_publication(&checked)?;
    review_policy::preserve_edit(&github, &target, pr, &checked)?;
    let destination = destination(&github, &target.repo)?;
    authored_to_standard(destination, &target.repo, PULL_REQUEST, &document)?;
    let issue = inspect_issue_for(
        &github,
        &target,
        destination,
        Some((&document.body, BodySource::File(&body_file))),
        rerun,
        live.state == "MERGED",
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
        target.repo.clone().into(),
    ];
    let _body = write_document(&mut args, &document)?;
    print!("{}", gh(&args)?);
    figure::verify_publication(&document, "pr", &target.repo, pr)?;
    review_policy::remember(&checked, &target, &github, pr)?;
    Ok(())
}

/// Validates a live document and moves the open draft to review, returning the checked body.
#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one fact of the live PR read in a single response"
)]
fn make_ready(
    github: &Github,
    target: &Target,
    pr: u64,
    title: String,
    body: String,
    draft: bool,
    report: Report,
    head: &str,
    rerun: &str,
) -> Result<String> {
    let live = live_document(github, target, pr)?;
    ensure!(
        live.head_ref_oid.as_deref() == Some(head),
        "PR head changed before its ready admission"
    );
    let reviewed = review_policy::live_binding(github, target, &body, &live)?;
    review_policy::admit_ready(&reviewed, target, github, pr)?;
    let document = validate(target, title, body)?;
    let destination = destination(github, &target.repo)?;
    let issue = inspect_issue(
        github,
        target,
        destination,
        Some((&document.body, BodySource::Pr(pr))),
        rerun,
    )?;
    let checks = if pr_ready_rules::complete(Workflow::Fixed, report) {
        Checks::Success
    } else {
        Checks::Incomplete
    };
    planned(Operation::Ready, target, Some(pr), draft, issue, checks)?;
    dotfiles::hosts::require_publication_admission(Path::new("."), head)?;
    dotfiles::assure::check_adrs(
        Path::new("."),
        &dotfiles::assure::Tree::Commit(Some(
            live.base_ref_oid
                .as_deref()
                .context("PR base commit missing")?
                .to_owned(),
        )),
        &dotfiles::assure::Tree::Commit(Some(head.to_owned())),
    )?;
    // The command walks the model of `specs/pr_ready.qnt`: head 0 carries the report this read returned.
    let reported = ready_step(pr_ready_rules::initial(), Move::Report(report))
        .unwrap_or_else(|_| pr_ready_rules::initial());
    let marked = ready_step(ready_step(reported, Move::View)?, Move::Mark)?;
    let mut args = arguments("ready", &target.repo);
    args.push(pr.to_string().into());
    print!("{}", gh(&args)?);
    review_policy::remember(&reviewed, target, github, pr)?;
    settle_ready(target, pr, head, marked)?;
    Ok(document.body)
}

/// One step of the fixed command in the model, which the plan has already admitted.
fn ready_step(state: State, action: Move) -> Result<State> {
    pr_ready_rules::step(Workflow::Fixed, state, action)
        .with_context(|| format!("the ready model can't take {action:?} in {state:?}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeadView {
    head_ref_oid: String,
}

/// Reads the head again after `gh pr ready` and returns the PR to draft when another push moved it, so no unchecked head stays ready after the command returns.
/// Exempt as semantic-boundary: GitHub's `markPullRequestReadyForReview` takes no expected head, so a push can still land between the mark and this read.
/// Evidence: `crates/pr-workflow/tests/pr_ready_replay.rs::every_trace_of_the_fixed_model_replays_state_for_state_and_covers_every_action`.
/// Reconsider when GitHub accepts an expected head for marking a PR ready.
fn settle_ready(target: &Target, pr: u64, checked: &str, marked: State) -> Result<()> {
    let mut args = arguments("view", &target.repo);
    args.extend([pr.to_string().into(), "--json".into(), "headRefOid".into()]);
    let reread: HeadView = serde_json::from_str(&gh(&args)?).context("invalid gh PR head")?;
    let moved =
        pr_ready_rules::settle(checked, reread.head_ref_oid.as_str()) == Settlement::ReturnToDraft;
    let observed = if moved {
        ready_step(marked, Move::Push)?
    } else {
        marked
    };
    let settled = ready_step(observed, Move::Settle)?;
    ensure!(
        pr_ready_rules::safe(&settled),
        "the ready model left an unchecked head ready: {settled:?}"
    );
    if settled.ready {
        return Ok(());
    }
    let mut undo = arguments("ready", &target.repo);
    undo.extend([pr.to_string().into(), "--undo".into()]);
    print!("{}", gh(&undo)?);
    Err(Refusal::new(
        "pr.head",
        "another push moved the head after its checks were read, so the PR returned to draft",
        command(&[
            "pr-workflow",
            "ready",
            "--repo",
            &target.repo,
            "--pr",
            &pr.to_string(),
        ]),
    )
    .evidence(format!(
        "checked {checked}, head {} after marking ready",
        reread.head_ref_oid
    ))
    .into())
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
            "number,headRefName,baseRefName,isCrossRepository,author,mergeable",
        ]
        .map(OsString::from),
    );
    let listed: Vec<OpenPr> = serde_json::from_str(&gh(&args)?).context("invalid gh PR list")?;
    Ok(listed
        .into_iter()
        .filter(|pr| !pr.is_cross_repository)
        .collect())
}

/// `merge` lands a stack of one.
/// GitHub merges a member of a longer stack only through its asynchronous endpoint, which `stack merge` uses with the members below it.
fn require_unstacked(target: &Target, open: &[OpenPr], pr: u64) -> Result<()> {
    let Some(member) = open.iter().find(|open| open.number == pr) else {
        return Ok(());
    };
    let related: Vec<_> = open
        .iter()
        .filter_map(|other| {
            if other.head_ref_name == member.base_ref_name {
                Some(format!(
                    "parent #{} ({})",
                    other.number, other.head_ref_name
                ))
            } else if other.base_ref_name == member.head_ref_name {
                Some(format!("child #{} ({})", other.number, other.head_ref_name))
            } else {
                None
            }
        })
        .collect();
    if related.is_empty() {
        return Ok(());
    }
    let mut refusal = Refusal::new(
        "pr.stack",
        format!(
            "#{pr} belongs to an open stack; `stack merge` lands it with the members below it through GitHub's asynchronous merge"
        ),
        command(&[
            "pr-workflow",
            "stack",
            "merge",
            "--repo",
            &target.repo,
            "--pr",
            &pr.to_string(),
        ]),
    );
    for relation in related {
        refusal = refusal.evidence(format!(
            "#{pr} ({}) on {} has {relation}",
            member.head_ref_name, member.base_ref_name
        ));
    }
    Err(refusal.into())
}

/// Returns the open PRs from the trunk up to the one whose head matches `head`, bottom first, and the trunk below them.
fn chain(open: &[OpenPr], head: &str) -> (Vec<u64>, String) {
    let mut numbers = Vec::new();
    let mut branch = head.to_owned();
    // The walk takes each PR at most once, so a cycle of bases ends it.
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
        "title,body,state,isDraft,headRefOid,baseRefName,baseRefOid,labels,author,mergeable,mergeStateStatus,statusCheckRollup,closingIssuesReferences,headRefName,headRepository,headRepositoryOwner,isCrossRepository".into(),
    ]);
    let mut view: MergeView =
        serde_json::from_str(&gh(&args)?).context("invalid gh PR merge state")?;
    current_checks::retain_current(
        &target.repo,
        &view.head_ref_oid,
        &mut view.status_check_rollup,
    );
    Ok(view)
}

/// The merge state of `view`.
/// GitHub evaluates the merge requirements of a stacked PR when the stack merges.
fn merge_state(
    github: &Github,
    target: &Target,
    pr: u64,
    view: &MergeView,
    stacked: bool,
    requirements: &BranchRequirements,
    carry: Option<&review_policy::Checked>,
) -> Result<(MergeState, Option<review_policy::Checked>)> {
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
    let (total, unfinished, failed, missing) =
        tally(&view.status_check_rollup, &requirements.checks);
    let declared = total > 0
        || !requirements.checks.is_empty()
        || github.carries_workflow(target, &view.head_ref_oid)?;
    let checks = head_checks(total, unfinished.saturating_add(missing), failed, declared);
    let mut exclusion = Exclusion::None;
    let mut assessment = None;
    if matches!(lifecycle, Lifecycle::Draft | Lifecycle::Ready) {
        review_policy::fetch_pr(
            &std::env::current_dir()?,
            target,
            view.base_ref_oid
                .as_deref()
                .context("actual merge base commit missing")?,
            &view.head_ref_oid,
        )?;
        if let Some(prior) = carry {
            review_policy::carry_forward(github, target, view, prior)?;
        }
        let signer = review_policy::assessment_signer(github, target, &view.author, &view.source)?;
        let reviewed = review_policy::check(
            github,
            target,
            &view.body,
            view.base_ref_oid
                .as_deref()
                .context("actual merge base commit missing")?,
            &view.head_ref_oid,
            signer,
        )?;
        review_policy::admit_publication(&reviewed)?;
        review_policy::remember(&reviewed, target, github, pr)?;
        review_policy::validate_live_source(&reviewed, &view.source, carry)?;
        review_policy::labels(&view.labels)?;
        if lifecycle == Lifecycle::Ready
            && (stacked || requirements.queue || !behind)
            && view.mergeable != "CONFLICTING"
            && checks == rules::pr_rules::HeadChecks::Success
        {
            review_policy::completion(&reviewed, target, github, pr)?;
        }
        exclusion = Exclusion::Owner;
        assessment = Some(reviewed);
    }
    Ok((
        MergeState {
            lifecycle,
            conflict: view.mergeable == "CONFLICTING",
            behind: pr_merge_rules::base_update(behind, stacked, requirements.queue)
                != pr_merge_rules::BaseUpdate::Current,
            checks,
            clean: stacked
                || requirements.queue
                || matches!(view.merge_state_status.as_str(), "CLEAN" | "HAS_HOOKS"),
            exclusion,
        },
        assessment,
    ))
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
        Stop::NoChecks => (
            ABSENT_CHECKS_RULE,
            format!("{ABSENT_CHECKS_CAUSE}, push, and merge again"),
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
        started.elapsed() < Duration::from_secs(polling.timeout.saturating_mul(SECONDS_PER_MINUTE)),
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
    sleep_interval(polling, started);
    Ok(())
}

fn sleep_interval(polling: Polling, started: Instant) {
    let remaining = Duration::from_secs(polling.timeout.saturating_mul(SECONDS_PER_MINUTE))
        .saturating_sub(started.elapsed());
    std::thread::sleep(Duration::from_secs(polling.interval).min(remaining));
}

/// The target of one PR's ready transition: its own closing issue unless the command named one.
/// GitHub reports no closing reference for a PR whose base isn't the default branch, so the visible body supplies it then.
fn ready_target(target: &Target, view: &MergeView) -> Target {
    let mut target = target.clone();
    target.issue = target
        .issue
        .or_else(|| {
            view.closing_issues_references
                .first()
                .map(|issue| issue.number)
        })
        .or_else(|| {
            closing_references(&view.body, &target.repo)
                .first()
                .copied()
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
    let started = Instant::now();
    let mut carry = None;
    let mut updated_head = None;
    loop {
        let view = merge_view(target, pr)?;
        let requirements = github.branch_requirements(target, &view.base_ref_name)?;
        let (state, assessment) = merge_state(
            github,
            target,
            pr,
            &view,
            false,
            &requirements,
            carry.as_ref(),
        )?;
        if updated_head
            .as_deref()
            .is_some_and(|head| head != view.head_ref_oid)
        {
            carry = None;
            updated_head = None;
        }
        match merge_step(state, false, false) {
            Step::Done => {
                println!("#{pr} merged at {}", view.head_ref_oid);
                return Ok(());
            }
            Step::Update => {
                if carry.is_some() {
                    wait(target, pr, polling, started)?;
                    continue;
                }
                let mut args = arguments("update-branch", &target.repo);
                args.push(pr.to_string().into());
                print!("{}", gh(&args)?);
                carry = assessment;
                updated_head = Some(view.head_ref_oid);
                wait(target, pr, polling, started)?;
            }
            Step::Restack => bail!("an independent PR is never restacked"),
            Step::Wait => wait(target, pr, polling, started)?,
            Step::Exclude => bail!("CodeRabbit PR reviews are paused by the user"),
            Step::Ready => {
                make_ready(
                    github,
                    &ready_target(target, &view),
                    pr,
                    view.title,
                    view.body,
                    true,
                    checks(&view.status_check_rollup, &requirements.checks),
                    &view.head_ref_oid,
                    rerun,
                )?;
            }
            Step::Merge => {
                let heads = [(pr, view.head_ref_oid.clone())];
                if requirements.queue {
                    return submit_merge(github, target, pr, &heads, true, polling, started);
                }
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
                return await_merge(github, target, &heads, false, polling, started);
            }
            Step::Stop(stop) => return Err(stop_refusal(target, pr, stop, false, &view).into()),
        }
    }
}

/// The variable and file through which dotguard admits the rewrites of the stack tooling.
/// The `stack` module of dotguard reads the same names.
const STACK_TOKEN: &str = "DOTGUARD_STACK";
const STACK_LEASE: &str = "dotguard-stack.lease";

/// The repository's stack lease, held while gh-stack rewrites and pushes branches and removed when dropped.
struct Lease {
    path: PathBuf,
    token: String,
}

fn lease_token() -> Result<String> {
    let probe = tempfile::NamedTempFile::new()?;
    let mut seed = Sha256::new();
    seed.update(std::process::id().to_le_bytes());
    seed.update(format!("{:?}", std::time::SystemTime::now()));
    seed.update(probe.path().as_os_str().as_encoded_bytes());
    Ok(crate::build_inputs::hex(&seed.finalize()))
}

impl Lease {
    fn acquire() -> Result<Self> {
        let directory = git_output(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let path = Path::new(directory.trim()).join(STACK_LEASE);
        let token = lease_token()?;
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
        platform::cleanup::remove_file(&self.path);
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
    rules::origin_rules::github_slug(url).map(str::to_owned)
}

/// Requires a current directory that holds a checkout whose `origin` matches the target repository.
fn require_checkout(target: &Target) -> Result<()> {
    let url = git_output(&["remote", "get-url", "origin"]).unwrap_or_default();
    require_checkout_origin(target, &url)
}

fn require_checkout_origin(target: &Target, url: &str) -> Result<()> {
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

/// Runs the pinned gh-stack on the target repository and takes the lease when it rewrites or pushes branches.
fn gh_stack(target: &Target, lease: Option<&Lease>, args: &[&str]) -> Result<()> {
    let mut process = tool::owner_gh_stack()?;
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
        .context("start owner-provisioned gh-stack; apply the shared mise tools")?;
    let rebase = || command(&["pr-workflow", "stack", "rebase", "--repo", &target.repo]);
    match status.code() {
        Some(0) => Ok(()),
        Some(STACK_CONFLICT) if args.first() == Some(&"rebase") => Err(Refusal::new(
            "pr.conflict",
            "the stack rebase stopped at a conflict; resolve the listed files, stage them, and continue",
            format!("{} --continue", rebase()),
        )
        .evidence(format!("gh-stack {} exited with {status}", args.join(" ")))
        .into()),
        Some(STACK_CONFLICT) => Err(Refusal::new(
            "pr.conflict",
            "the stack conflicts with its updated parent; gh-stack restored every branch, so rebase the stack to resolve it",
            rebase(),
        )
        .evidence(format!("gh-stack {} exited with {status}", args.join(" ")))
        .into()),
        _ => bail!("gh-stack {} failed with {status}", args.join(" ")),
    }
}

/// One closed PR as the REST list reports it, with the head it merged.
#[derive(Deserialize)]
struct ClosedPr {
    merged_at: Option<String>,
    head: ClosedHead,
}

#[derive(Deserialize)]
struct ClosedHead {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
}

/// Whether merging `branch` into `upstream` leaves the tree `upstream_tree`, so the upstream already holds every change.
fn contained(upstream: &str, branch: &str, upstream_tree: &str) -> Result<bool> {
    let output = Tool::Git
        .command()
        .args(["merge-tree", "--write-tree", upstream, branch])
        .output()
        .context("start git")?;
    Ok(output.status.success()
        && String::from_utf8_lossy(&output.stdout).lines().next() == Some(upstream_tree))
}

/// Deletes each local branch whose PR merged at its exact head, or whose changes the default branch already holds.
/// It keeps the default branch and every branch a worktree checks out, and deletes under the stack lease, which the Git guard admits.
fn prune_merged(target: &Target, dry_run: bool) -> Result<()> {
    require_checkout(target)?;
    let github = Github::connect()?;
    let default = github
        .repository(&target.repo)?
        .default_branch
        .context("the repository reports no default branch")?;
    let upstream = format!("refs/remotes/origin/{default}");
    let upstream_tree = git_output(&["rev-parse", "--verify", &format!("{upstream}^{{tree}}")])
        .map_err(|error| {
            Refusal::new(
                "pr.prune",
                format!("pruning compares branches with {upstream}, which this clone lacks"),
                command(&["git", "fetch", "origin", &default]),
            )
            .evidence(format!("{error:#}"))
        })?;
    let upstream_tree = upstream_tree.trim();
    let owner = target
        .repo
        .split_once('/')
        .map(|(owner, _)| owner)
        .context("the repository has no owner")?;
    let checked_out: BTreeSet<String> = git_output(&["worktree", "list", "--porcelain"])?
        .lines()
        .filter_map(|line| line.strip_prefix("branch refs/heads/"))
        .map(str::to_owned)
        .collect();
    let mut merged = Vec::new();
    for line in git_output(&[
        "for-each-ref",
        "--format=%(refname:short) %(objectname)",
        "refs/heads",
    ])?
    .lines()
    {
        let Some((branch, head)) = line.split_once(' ') else {
            continue;
        };
        if branch == default || checked_out.contains(branch) {
            continue;
        }
        let closed: Vec<ClosedPr> = github.read(&format!(
            "repos/{}/pulls?state=closed&head={owner}:{}&per_page=100",
            target.repo,
            review_policy::path_segment(branch)?
        ))?;
        let reason = if closed
            .iter()
            .any(|pr| pr.merged_at.is_some() && pr.head.name == branch && pr.head.sha == head)
        {
            "its pull request merged at this head"
        } else if contained(&upstream, branch, upstream_tree)? {
            "the default branch holds its changes"
        } else {
            continue;
        };
        merged.push((branch.to_owned(), head.to_owned(), reason));
    }
    if dry_run {
        for (branch, head, reason) in &merged {
            println!("Would delete {branch} at {head}: {reason}");
        }
        return Ok(());
    }
    let lease = Lease::acquire()?;
    for (branch, head, reason) in &merged {
        let status = Tool::Git
            .command()
            .args(["branch", "-D", branch])
            .env(STACK_TOKEN, &lease.token)
            .status()
            .context("start git")?;
        ensure!(
            status.success(),
            "git branch -D {branch} failed with {status}"
        );
        println!("Deleted {branch} at {head}: {reason}");
    }
    Ok(())
}

/// Runs a leased `restack` and carries the issue context of each local stack branch whose own change it kept.
fn restack(target: &Target, restack: impl FnOnce() -> Result<()>) -> Result<()> {
    require_checkout(target)?;
    let root = platform::issue_context::checkout(&std::env::current_dir()?)?;
    let contexts = context::local_contexts(&root)?;
    if contexts.is_empty() {
        return restack();
    }
    let github = Github::connect()?;
    let trunk = github
        .repository(&target.repo)?
        .default_branch
        .context("the repository reports no default branch")?;
    let restacking = context::before_restack(&root, contexts, &open_prs(target)?, &trunk)?;
    restack()?;
    context::after_restack(&github, &root, &target.repo, &trunk, restacking)
}

/// Restacks the checked-out stack under the lease, pushing it with explicit leases.
fn sync_stack(target: &Target) -> Result<()> {
    require_checkout(target)?;
    let branch = git_output(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    sync_stack_at(target, branch.trim())
}

/// Restacks the stack that `selector` names: a pull request address, a stack number, or a branch.
/// Checking it out adopts the stack that GitHub records while this clone still lacks tracking for it.
/// The web address of a pull request names one stack even on a trunk that more than one local stack shares, and a bare number could name a stack first.
fn sync_stack_at(target: &Target, selector: &str) -> Result<()> {
    require_checkout(target)?;
    let lease = Lease::acquire()?;
    gh_stack(target, Some(&lease), &["checkout", selector])?;
    gh_stack(target, Some(&lease), &["sync"])
}

/// Merges the stack up to `top` once every PR in it can merge.
fn check_stack_issue(
    github: &Github,
    target: &Target,
    destination: Destination,
    pr: u64,
    view: &MergeView,
    rerun: &str,
) -> Result<()> {
    if view.state != "OPEN"
        || ((destination.fork || !destination.personal) && target.issue.is_none())
    {
        return Ok(());
    }
    let member = ready_target(target, view);
    inspect_issue(
        github,
        &member,
        destination,
        Some((&view.body, BodySource::Pr(pr))),
        rerun,
    )
    .and_then(|gate| require_issue(gate, &member))
    .map_err(|error| {
        if let Some(refusal) = error.downcast_ref::<Refusal>() {
            let mut refusal = refusal.clone().evidence(format!("stack member #{pr}"));
            let own = view
                .closing_issues_references
                .first()
                .map(|issue| issue.number)
                .or_else(|| closing_references(&view.body, &target.repo).first().copied());
            if let (Some(common), Some(own)) = (target.issue, own)
                && own != common
            {
                refusal = refusal.evidence(format!(
                    "stack member #{pr} closes #{own}; omit --issue {common} so each member needs to close its own issue"
                ));
            }
            anyhow::Error::from(refusal)
        } else {
            error.context(format!("stack member #{pr}"))
        }
    })
}

fn merge_stack(
    github: &Github,
    target: &Target,
    top: u64,
    polling: Polling,
    rerun: &str,
) -> Result<()> {
    let started = Instant::now();
    let destination = destination(github, &target.repo)?;
    let mut carry = BTreeMap::new();
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
        let (numbers, trunk) = chain(&open, &top_head);
        let requirements = github.branch_requirements(target, &trunk)?;
        let mut observed = Vec::new();
        let mut assessments = BTreeMap::new();
        for &pr in &numbers {
            let view = merge_view(target, pr)?;
            check_stack_issue(github, target, destination, pr, &view, rerun)?;
            let (state, assessment) = merge_state(
                github,
                target,
                pr,
                &view,
                true,
                &requirements,
                carry.get(&pr),
            )?;
            if let Some(assessment) = assessment {
                assessments.insert(pr, assessment);
            }
            observed.push((pr, view, merge_step(state, true, false)));
        }
        if observed.iter().any(|(_, _, step)| *step == Step::Restack) {
            for (pr, view, step) in &observed {
                if let Step::Stop(stop) = step {
                    return Err(stop_refusal(target, *pr, *stop, true, view).into());
                }
            }
            sync_stack_at(target, &format!("{GITHUB}/{}/pull/{top}", target.repo))?;
            carry = assessments;
            wait(target, top, polling, started)?;
            continue;
        }
        carry.clear();
        let mut verified = Vec::new();
        for (pr, view, step) in observed {
            match step {
                Step::Merge => verified.push((pr, view.head_ref_oid)),
                Step::Done => {}
                Step::Restack => bail!("the stack must be restacked before its members can merge"),
                Step::Wait => {
                    wait(target, pr, polling, started)?;
                    continue 'restart;
                }
                Step::Exclude => bail!("CodeRabbit PR reviews are paused by the user"),
                Step::Ready => {
                    make_ready(
                        github,
                        &ready_target(target, &view),
                        pr,
                        view.title,
                        view.body,
                        true,
                        checks(&view.status_check_rollup, &requirements.checks),
                        &view.head_ref_oid,
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
        return submit_merge(
            github,
            target,
            top,
            &verified,
            requirements.queue,
            polling,
            started,
        );
    }
}

const GIT_MERGE_CONFLICT: i32 = 1;

fn merge_heads(head: &str, other: &str) -> Result<PathMerge> {
    let output = Tool::Git
        .command()
        .args(["merge-tree", "--write-tree", head, other])
        .output()
        .context("start git merge-tree")?;
    if output.status.success() {
        Ok(PathMerge::Clean)
    } else if output.status.code() == Some(GIT_MERGE_CONFLICT) {
        Ok(PathMerge::Conflict)
    } else {
        bail!(
            "git merge-tree --write-tree {head} {other} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
}

fn report_dependency_skip(pr: &OpenPr) -> bool {
    let author = pr.author.as_ref();
    let login = author.map_or("", |author| author.login.as_str());
    let Some(skip) = dependency_skip(
        login,
        author.and_then(|author| author.author_type.as_deref()),
        author.is_some_and(|author| author.is_bot),
        pr.mergeable.as_deref(),
    ) else {
        return false;
    };
    let reason = match skip {
        DependencySkip::Bot => format!("bot author {login}"),
        DependencySkip::BaseConflict => {
            format!(
                "stale PR already conflicts with its base {}",
                pr.base_ref_name
            )
        }
    };
    println!("skipped #{} ({}): {reason}", pr.number, pr.head_ref_name);
    true
}

/// Refuses shared commits or conflicting heads and names the required stack.
fn refuse_dependent(
    github: &Github,
    target: &Target,
    head: &str,
    base: Option<&str>,
    title: &str,
    body_file: &Path,
    ancestors: &[u64],
) -> Result<()> {
    let open = open_prs(target)?;
    let others: Vec<&OpenPr> = open
        .iter()
        .filter(|pr| pr.head_ref_name != head)
        .filter(|pr| !ancestors.contains(&pr.number))
        .filter(|pr| !report_dependency_skip(pr))
        .collect();
    if others.is_empty() {
        return Ok(());
    }
    let trunk = if let Some(base) = base {
        base.to_owned()
    } else {
        let repository: Repository = github.read(&format!("repos/{}", target.repo))?;
        repository
            .default_branch
            .context("GitHub reported no default branch")?
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
    let paths = |tip: &str| {
        git_set(&[
            "diff",
            "--name-only",
            "--no-renames",
            &format!("{trunk}...{tip}"),
        ])
    };
    let (own_commits, own_paths) = (commits(&tip)?, paths(&tip)?);
    for other in others {
        let theirs = format!("refs/remotes/origin/{}", other.head_ref_name);
        let shared_commits = !own_commits.is_disjoint(&commits(&theirs)?);
        let shared_paths = !own_paths.is_disjoint(&paths(&theirs)?);
        let merge = if shared_paths {
            merge_heads(&tip, &theirs)?
        } else {
            PathMerge::Disjoint
        };
        let Some(found) = dependency(shared_commits, merge) else {
            continue;
        };
        if !ancestors.is_empty() {
            return Err(Refusal::new(
                "pr.dependency",
                format!(
                    "{head} depends on #{} outside its parent chain; resolve that dependency before extending the open stack",
                    other.number
                ),
                command(&["gh", "pr", "view", &other.number.to_string(), "--repo", &target.repo]),
            )
            .evidence(format!("#{} ({}): {found:?}", other.number, other.head_ref_name))
            .into());
        }
        return Err(dependency_refusal(target, head, title, body_file, other, found).into());
    }
    Ok(())
}

fn dependency_refusal(
    target: &Target,
    head: &str,
    title: &str,
    body_file: &Path,
    other: &OpenPr,
    found: Dependency,
) -> Refusal {
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
    Refusal::new(
        "pr.dependency",
        match found {
            Dependency::Commits => format!(
                "{head} contains commits of #{}, so it depends on that PR; open it as a stack on top of it",
                other.number
            ),
            Dependency::Conflict => format!(
                "{head} conflicts with #{}; open it as a stack on top of that PR",
                other.number
            ),
        },
        command(&next),
    )
    .evidence(format!(
        "#{} ({}): {}",
        other.number,
        other.head_ref_name,
        match found {
            Dependency::Commits => "shared commits",
            Dependency::Conflict => "git merge-tree --write-tree reports a conflict between the heads",
        }
    ))
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

/// Opens a draft PR on top of the open PR whose head names `base`, and links the stack.
fn create_stacked(document: DocumentArgs, head: &str, base: &str, rerun: &str) -> Result<()> {
    create(document, head, Some(base), &Creation::Stack, rerun)
}

fn link_stack(target: &Target, numbers: &[u64], base: &str) -> Result<()> {
    target.conventional_required()?;
    require_checkout(target)?;
    let github = Github::connect()?;
    let plan = work::LinkPlan::prepare(&github, target, numbers, base)?;
    let mut before = Vec::new();
    for member in &plan.members {
        let live = live_document(&github, target, member.number)?;
        ensure!(
            live.state == "OPEN" && live.author.login.eq_ignore_ascii_case(&github.login),
            "stack members must remain owned open PRs"
        );
        ensure!(
            live.head_ref_oid.as_deref() == Some(&member.sha),
            "stack PR head changed before linking"
        );
        let checked = review_policy::create_binding(
            &github,
            target,
            &live.body,
            &member.head,
            Some(&member.base),
        )?;
        review_policy::validate_live_source(&checked, &live.source, None)?;
        review_policy::admit_publication(&checked)?;
        review_policy::labels(&live.labels)?;
        before.push(live);
    }
    plan.require_current()?;
    if before
        .iter()
        .zip(&plan.members)
        .any(|(live, member)| live.base_ref_name.as_deref() != Some(&member.base))
    {
        let mut args = vec!["link".to_owned(), "--base".to_owned(), base.to_owned()];
        args.extend(numbers.iter().map(u64::to_string));
        gh_stack(
            target,
            None,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?;
    }
    for (member, original) in plan.members.iter().zip(before) {
        let live = live_document(&github, target, member.number)?;
        ensure!(
            live.state == "OPEN"
                && live.head_ref_oid.as_deref() == Some(&member.sha)
                && live.base_ref_name.as_deref() == Some(&member.base)
                && live.is_draft == original.is_draft
                && live.body == original.body
                && live.title == original.title,
            "stack link readback changed; inspect the remote stack before another write"
        );
    }
    plan.linked()?;
    println!(
        "Linked {} PRs and retired their independent declarations",
        numbers.len()
    );
    Ok(())
}

fn stack(action: StackAction, rerun: &str) -> Result<()> {
    match action {
        StackAction::Link { target, base, prs } => link_stack(&target, &prs, &base)?,
        StackAction::Create {
            document,
            head,
            base,
        } => create_stacked(document, &head, &base, rerun)?,
        StackAction::Sync { target } => {
            target.conventional_required()?;
            restack(&target, || sync_stack(&target))?;
        }
        StackAction::Rebase {
            target,
            resume,
            abort,
        } => {
            target.conventional_required()?;
            require_checkout(&target)?;
            let rebase = || {
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
                Ok(())
            };
            if abort {
                rebase()?;
            } else {
                restack(&target, rebase)?;
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

fn issue(action: IssueAction) -> Result<()> {
    let (document, number) = match action {
        IssueAction::Check { document } => {
            let validated = validate_issue(&document)?;
            authored_to_standard(
                offline_destination(&document.repo)?,
                &document.repo,
                ISSUE,
                &validated,
            )?;
            println!("Issue document accepted");
            return Ok(());
        }
        IssueAction::Create { document } => (document, None),
        IssueAction::Edit { document, number } => (document, Some(number)),
    };
    let validated = validate_issue(&document)?;
    let github = Github::connect()?;
    authored_to_standard(
        destination(&github, &document.repo)?,
        &document.repo,
        ISSUE,
        &validated,
    )?;
    let mut args: Vec<OsString> = vec!["issue".into()];
    match number {
        Some(number) => args.extend(["edit".into(), number.to_string().into()]),
        None => args.push("create".into()),
    }
    args.extend(["--repo".into(), OsString::from(&document.repo)]);
    let _body = write_document(&mut args, &validated)?;
    let published = gh(&args)?;
    print!("{published}");
    let number = match number {
        Some(number) => number,
        None => created_number(&published)?,
    };
    figure::verify_publication(&validated, "issue", &document.repo, number)
}

pub(crate) fn install(binary: &std::path::Path, user_directory: &std::path::Path) -> Result<()> {
    let directory = user_directory.join(".local/bin");
    crate::aside::install(
        binary,
        &directory.join(format!("pr-workflow{}", std::env::consts::EXE_SUFFIX)),
    )?;
    println!("Installed pr-workflow in {}", directory.display());
    Ok(())
}

/// Refresh only the verified executable and its installation origin.
///
/// # Errors
///
/// Refuses a stale or foreign artifact and reports installation failures.
pub fn install_binary(
    source: &std::path::Path,
    binary: &std::path::Path,
    user_directory: &std::path::Path,
) -> Result<()> {
    let built = platform::freshness::Built::PrWorkflow;
    platform::freshness::verify_artifact(source, binary, built)?;
    install(binary, user_directory)?;
    platform::freshness::record_installation(user_directory, source, built)
}

/// The refusal of an installation whose unrelated managed skills differ from the approved catalog.
/// It names each difference and the review command.
/// The skill this command installs isn't unrelated, so a difference in it doesn't appear.
fn changed_skills(
    skills: &std::path::Path,
    retained: &skill_ops::Catalog,
    capture: Option<anyhow::Error>,
) -> Result<Refusal> {
    use skill_ops::skill_catalog_rules::Difference;
    let differences = match skill_ops::catalog_differences(skills, retained) {
        Ok(differences) => differences,
        Err(error) => return Err(capture.unwrap_or(error)),
    };
    let differences: Vec<_> = differences
        .into_iter()
        .filter(|(name, _)| name != "pull-request")
        .collect();
    let mut review = vec!["chezmoi".to_owned(), "diff".to_owned()];
    let mut evidence = Vec::new();
    for (name, difference) in &differences {
        let path = skills.join(name);
        let (state, blocks) = match difference {
            Difference::Changed => ("changed since its approved revision", true),
            Difference::Removed => ("removed though the approved catalog names it", true),
            Difference::Added => (
                "added outside the approved catalog, which leaves installation unaffected",
                false,
            ),
        };
        evidence.push(format!("{name}: {state} ({})", path.display()));
        if blocks {
            review.push(path.to_string_lossy().into_owned());
        }
    }
    if review.len() == 2 {
        review.push(skills.to_string_lossy().into_owned());
        evidence.push(
            "no unrelated managed skill differs in its files, description, or size; their approved references differ"
                .to_owned(),
        );
    }
    if let Some(error) = capture {
        evidence.push(format!("capture: {error:#}"));
    }
    evidence.push(
        "after review, `mise x -- cargo xtask install` in the dotfiles checkout reinstalls the source skills and records their approval"
            .to_owned(),
    );
    let mut refusal = Refusal::new(
        "pr.managed-skills",
        "unrelated installed managed skills have changed; preserve and review them before installation",
        command(&review),
    );
    for line in evidence {
        refusal = refusal.evidence(line);
    }
    Ok(refusal)
}

/// Installs the checked PR skill through `apply` and the `pr-workflow` binary under `user_directory`, then records the new catalog.
/// Experimental decision checks require the machine's opt-in.
///
/// # Errors
///
/// Returns an error when the source catalog fails its check, the installed catalog or policy differs from its approved identity, or another installed managed skill has changed.
/// Also returns an error when `apply` fails, the installed PR skill differs from the source, or reading, copying, or writing a file fails.
pub fn deploy(
    source: &std::path::Path,
    binary: &std::path::Path,
    user_directory: &std::path::Path,
    apply: impl FnMut(&[PathBuf]) -> Result<()>,
) -> Result<()> {
    deploy_selected(source, binary, user_directory, &["pull-request"], apply)
}

/// Installs the PR workflow's authoring skills without changing unrelated managed skills.
///
/// # Errors
/// Returns an error for changed unrelated skills, a missing source, or an incomplete installation.
pub fn deploy_workflow(
    source: &Path,
    binary: &Path,
    user_directory: &Path,
    apply: impl FnMut(&[PathBuf]) -> Result<()>,
) -> Result<()> {
    deploy_selected(
        source,
        binary,
        user_directory,
        &[
            "pull-request",
            "screenshots",
            "diagram-explanations",
            "knowledge-to-skills",
        ],
        apply,
    )
}

fn deploy_selected(
    source: &Path,
    binary: &Path,
    user_directory: &Path,
    selected: &[&str],
    mut apply: impl FnMut(&[PathBuf]) -> Result<()>,
) -> Result<()> {
    use skill_ops as ops;
    let experimental = platform::incidents::enabled_at(user_directory);
    if experimental {
        ops::check_catalog(source)?;
    }
    let manifest = user_directory.join(".config/skill-ops/approved.json");
    let (mut approved, retained) = preserved_catalog(user_directory, selected, experimental)?;
    let expected = ops::catalog(&source.join("home/dot_agents/skills"))?;
    for name in selected {
        approved.catalog.skills.insert(
            (*name).into(),
            expected
                .skills
                .get(*name)
                .with_context(|| format!("source skill {name} is missing"))?
                .clone(),
        );
    }
    let available: BTreeSet<_> = approved.catalog.skills.keys().cloned().collect();
    for name in selected {
        approved
            .catalog
            .skills
            .get_mut(*name)
            .context("planned authoring skill is missing")?
            .references
            .retain(|name| available.contains(name));
    }
    if experimental {
        let reviewed = ops::adoptable_revisions(
            &expected,
            &ops::tracked_decisions(&source.join(ops::DECISIONS))?,
        );
        for name in selected {
            match reviewed.get(*name) {
                Some(skill) => approved.reviewed.insert((*name).into(), skill.clone()),
                None => approved.reviewed.remove(*name),
            };
        }
    }
    let paths: Vec<_> = skill_ops::skill_source_paths(source)?
        .into_iter()
        .filter(|path| {
            selected.iter().any(|name| {
                path.starts_with(source.join("home/dot_agents/skills").join(name))
                    || *path
                        == source
                            .join("home/dot_claude/skills")
                            .join(format!("symlink_{name}"))
            })
        })
        .collect();
    ensure!(
        paths.len() >= SKILL_WITH_ALIAS * selected.len(),
        "scoped PR skill installation is empty"
    );
    apply(&paths)?;
    let installed = ops::catalog_owned(&user_directory.join(".agents/skills"), &approved.catalog)?;
    for name in selected {
        ensure!(
            installed.skills.get(*name) == approved.catalog.skills.get(*name),
            "installed authoring skill differs from the reviewed source"
        );
    }
    let mut unchanged = installed.clone();
    for name in selected {
        unchanged.skills.remove(*name);
    }
    for skill in unchanged.skills.values_mut() {
        skill
            .references
            .retain(|name| !selected.contains(&name.as_str()));
    }
    ensure!(
        unchanged == retained,
        "unrelated installed managed skills changed during scoped installation"
    );
    approved.catalog = installed;
    install(binary, user_directory)?;
    approved.catalog_hash = ops::hash(&approved.catalog)?;
    ops::write_json(&manifest, &approved, true)?;
    Ok(())
}

fn preserved_catalog(
    user_directory: &Path,
    selected: &[&str],
    experimental: bool,
) -> Result<(skill_ops::Approved, skill_ops::Catalog)> {
    use skill_ops as ops;
    let mut approved = ops::read_approved(&user_directory.join(".config/skill-ops/approved.json"))?;
    ensure!(
        ops::hash(&approved.catalog)? == approved.catalog_hash,
        "installed managed catalog is inconsistent"
    );
    let mut retained = approved.catalog.clone();
    for name in selected {
        retained.skills.remove(*name);
    }
    for skill in retained.skills.values_mut() {
        skill
            .references
            .retain(|name| !selected.contains(&name.as_str()));
    }
    let skills = user_directory.join(".agents/skills");
    let installed = ops::catalog_owned(&skills, &retained);
    if installed.is_err()
        || (experimental
            && !installed
                .as_ref()
                .is_ok_and(|installed| *installed == retained))
    {
        return Err(changed_skills(&skills, &retained, installed.err())?.into());
    }
    let retained = installed?;
    for (name, skill) in &retained.skills {
        approved.catalog.skills.insert(name.clone(), skill.clone());
    }
    let policy: ops::Policy =
        ops::read_json(&user_directory.join(".config/skill-ops/policy.json"))?;
    ensure!(
        ops::hash(&policy)? == approved.policy_hash,
        "installed policy differs from its managed identity"
    );
    Ok((approved, retained))
}
