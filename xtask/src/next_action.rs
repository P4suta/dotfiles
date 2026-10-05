//! Inspects the repository, GitHub, and host state and recommends the single next step of the workflow.

use crate::next_action_rules::{Checks, Head, Host, Pr, State, Step, Upstream, decide, executable};
use crate::skill_ops::{DECISIONS, catalog, complete, tracked_decisions};
use crate::skill_rules::{Tracked, tracked_decision};
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The base for the default branch, and for a branch whose PR is unknown.
pub const BASE: &str = "origin/main";
pub const DEFAULT_BRANCH: &str = "main";

/// Where this host keeps the state that the inspection reads.
pub struct Environment {
    pub home: PathBuf,
    /// `TEMP` on Windows, `None` when it is unset; the system temporary directory on other hosts.
    pub temp: Option<PathBuf>,
    pub cargo_target: Option<PathBuf>,
    pub offline: bool,
}

impl Environment {
    pub fn current(offline: bool) -> Result<Self> {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .context("the home directory is unavailable; set HOME or USERPROFILE")?;
        Ok(Self {
            home: home.into(),
            temp: if cfg!(windows) {
                std::env::var_os("TEMP").map(PathBuf::from)
            } else {
                Some(std::env::temp_dir())
            },
            cargo_target: std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from),
            offline,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum HostStatus {
    Ready,
    TempUnset,
    VolumeMissing { path: PathBuf },
    TempMissing { path: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub draft: bool,
    /// The branch the PR merges into.
    pub base: String,
    pub checks: Checks,
}

/// The facts behind a recommendation, reported with it so an agent can check the reasoning.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Observed {
    pub host: HostStatus,
    pub branch: Option<String>,
    pub changed_files: usize,
    /// The ref the branch is compared with: the PR's base branch when a PR exists, otherwise `origin/main`.
    pub base: String,
    pub behind_base: u64,
    pub ahead_base: u64,
    pub upstream: Option<String>,
    /// The upstream commit the inspection saw, which a force-push leases.
    pub upstream_commit: Option<String>,
    /// The upstream tip was once on the local branch, so every upstream commit passed through it.
    pub upstream_held: bool,
    pub unpushed: u64,
    /// Commits on the upstream that `HEAD` lacks.
    pub unpulled: u64,
    pub undecided_skills: Vec<String>,
    pub incomplete_decisions: Vec<String>,
    pub push_paused: bool,
    pub review_paused: bool,
    pub repository: Option<String>,
    pub workflows: bool,
    pub github_inspected: bool,
    pub pull_request: Option<PullRequest>,
}

impl Observed {
    pub fn state(&self) -> State {
        State {
            host: match self.host {
                HostStatus::Ready => Host::Ready,
                HostStatus::TempUnset => Host::TempUnset,
                HostStatus::VolumeMissing { .. } => Host::VolumeMissing,
                HostStatus::TempMissing { .. } => Host::TempMissing,
            },
            head: match self.branch.as_deref() {
                None => Head::Detached,
                Some(DEFAULT_BRANCH) => Head::Default,
                Some(_) => Head::Feature,
            },
            dirty: self.changed_files > 0,
            undecided: !self.undecided_skills.is_empty(),
            incomplete: !self.incomplete_decisions.is_empty(),
            behind_base: self.behind_base > 0,
            ahead_base: self.ahead_base > 0,
            upstream: match (&self.upstream, self.unpushed > 0, self.unpulled > 0) {
                (None, _, _) => Upstream::Absent,
                (Some(_), false, false) => Upstream::Current,
                (Some(_), true, false) => Upstream::Ahead,
                (Some(_), false, true) => Upstream::Behind,
                (Some(_), true, true) if self.upstream_held => Upstream::Rewritten,
                (Some(_), true, true) => Upstream::Diverged,
            },
            push_paused: self.push_paused,
            pr: match (&self.pull_request, self.github_inspected) {
                (_, false) => Pr::Unknown,
                (None, true) => Pr::None,
                (Some(pr), true) => match pr.state.as_str() {
                    "MERGED" => Pr::Merged,
                    "CLOSED" => Pr::Closed,
                    _ if pr.draft => Pr::Draft,
                    _ => Pr::Ready,
                },
            },
            checks: self
                .pull_request
                .as_ref()
                .map_or(Checks::None, |pr| pr.checks),
            workflows: self.workflows,
            review_paused: self.review_paused,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Recommendation {
    pub action: Step,
    pub executable: bool,
    pub summary: String,
    pub command: Option<String>,
    pub state: Observed,
}

/// An unset `TEMP` comes first, then a required directory on an unmounted volume such as the Dev Drive, then a missing temporary directory.
/// The volume is the path's root, so only a Windows drive letter can be missing.
pub fn host_status(temp: Option<&Path>, cargo_target: Option<&Path>) -> HostStatus {
    let Some(temp) = temp else {
        return HostStatus::TempUnset;
    };
    for path in [Some(temp), cargo_target]
        .into_iter()
        .flatten()
        .filter(|path| path.is_absolute())
    {
        if let Some(volume) = path.ancestors().last()
            && !volume.is_dir()
        {
            return HostStatus::VolumeMissing {
                path: volume.to_path_buf(),
            };
        }
    }
    if temp.is_dir() {
        HostStatus::Ready
    } else {
        HostStatus::TempMissing {
            path: temp.to_path_buf(),
        }
    }
}

/// Failing checks outrank pending ones, and an empty rollup means no check has reported yet.
pub fn classify_checks(rollup: &[Value]) -> Checks {
    let mut pending = false;
    for item in rollup {
        let outcome = match item["state"].as_str() {
            Some(state) => match state {
                "SUCCESS" => Checks::Passing,
                "PENDING" | "EXPECTED" => Checks::Pending,
                _ => Checks::Failing,
            },
            None if item["status"].as_str() != Some("COMPLETED") => Checks::Pending,
            None => match item["conclusion"].as_str() {
                Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => Checks::Passing,
                _ => Checks::Failing,
            },
        };
        match outcome {
            Checks::Failing => return Checks::Failing,
            Checks::Pending => pending = true,
            _ => {}
        }
    }
    match (rollup.is_empty(), pending) {
        (true, _) => Checks::None,
        (false, true) => Checks::Pending,
        (false, false) => Checks::Passing,
    }
}

/// The `OWNER/REPO` of a GitHub remote URL in HTTPS, SSH, or scp form.
pub fn github_repository(url: &str) -> Option<String> {
    let (_, rest) = url.trim().split_once("github.com")?;
    let path = rest
        .strip_prefix(':')
        .or_else(|| rest.strip_prefix('/'))?
        .trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    (!owner.is_empty() && !name.is_empty() && !name.contains('/')).then(|| path.to_owned())
}

/// Skills whose tracked decision is missing or bound to another revision, and those whose current decision lacks its reason or evidence.
pub fn decision_backlog(root: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let tree = root.join("dot_agents/skills");
    if !tree.is_dir() {
        return Ok(Default::default());
    }
    let catalog = catalog(&tree)?;
    let decisions = tracked_decisions(&root.join(DECISIONS))?;
    let mut undecided = Vec::new();
    let mut incomplete = Vec::new();
    for (name, skill) in &catalog.skills {
        let decision = decisions.get(name);
        match tracked_decision(
            decision.is_some(),
            decision.is_some_and(|decision| decision.revision == skill.revision),
            decision.is_some_and(|decision| {
                complete(
                    decision.outcome,
                    &decision.reason,
                    &decision.evidence,
                    decision.revisit.as_ref(),
                )
            }),
        ) {
            Tracked::Current => {}
            Tracked::Missing | Tracked::Stale => undecided.push(name.clone()),
            Tracked::Incomplete => incomplete.push(name.clone()),
        }
    }
    Ok((undecided, incomplete))
}

fn git_optional(root: &Path, arguments: &[&str]) -> Result<Option<String>> {
    let output = Tool::Git
        .command()
        .current_dir(root)
        .args(arguments)
        .output()
        .context("start git")?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned()))
}

fn git(root: &Path, arguments: &[&str], next: &str) -> Result<String> {
    let output = Tool::Git
        .command()
        .current_dir(root)
        .args(arguments)
        .output()
        .context("start git")?;
    ensure!(
        output.status.success(),
        "git {} failed: {}; {next}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn count(text: &str) -> Result<u64> {
    text.trim()
        .parse()
        .with_context(|| format!("git returned a non-numeric count: {text}"))
}

fn pull_request(
    root: &Path,
    repository: Option<&str>,
    branch: &str,
) -> Result<Option<PullRequest>> {
    let mut command = Tool::Gh.command();
    command
        .current_dir(root)
        .env("GH_HOST", "github.com")
        .args([
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "all",
            "--limit",
            "1",
            "--json",
            "number,url,state,isDraft,baseRefName,statusCheckRollup",
        ]);
    if let Some(repository) = repository {
        command.args(["--repo", repository]);
    }
    let output = command
        .output()
        .context("gh is unavailable; install it, or rerun with --offline")?;
    ensure!(
        output.status.success(),
        "gh pr list failed: {}; run `gh auth status`, or rerun with --offline",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let listed: Vec<Value> =
        serde_json::from_slice(&output.stdout).context("gh returned an unexpected PR list")?;
    listed
        .first()
        .map(|pr| {
            Ok(PullRequest {
                number: pr["number"].as_u64().context("PR without a number")?,
                url: pr["url"].as_str().unwrap_or_default().to_owned(),
                state: pr["state"]
                    .as_str()
                    .context("PR without a state")?
                    .to_owned(),
                draft: pr["isDraft"]
                    .as_bool()
                    .context("PR without a draft state")?,
                base: pr["baseRefName"]
                    .as_str()
                    .context("PR without a base branch")?
                    .to_owned(),
                checks: classify_checks(
                    pr["statusCheckRollup"]
                        .as_array()
                        .map_or(&[][..], Vec::as_slice),
                ),
            })
        })
        .transpose()
}

/// Commits only `other` has and commits only `HEAD` has.
fn divergence(root: &Path, other: &str, next: &str) -> Result<(u64, u64)> {
    let counts = git(
        root,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{other}...HEAD"),
        ],
        next,
    )?;
    let (behind, ahead) = counts
        .split_once(char::is_whitespace)
        .context("git rev-list returned an unexpected count")?;
    Ok((count(behind)?, count(ahead)?))
}

/// Whether `commit` is reachable from an entry of the branch's reflog, that is, whether the branch once held it.
fn held_by_branch(root: &Path, branch: &str, commit: &str) -> Result<bool> {
    let reflog = git(
        root,
        &[
            "log",
            "--walk-reflogs",
            "--format=%H",
            &format!("refs/heads/{branch}"),
        ],
        "inspect the branch reflog with `git reflog show`",
    )?;
    let mut revisions = format!("{commit}\n");
    for entry in reflog.lines() {
        revisions.push_str(&format!("^{entry}\n"));
    }
    let mut child = Tool::Git
        .command()
        .current_dir(root)
        .args(["rev-list", "--count", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start git")?;
    child
        .stdin
        .take()
        .context("git rev-list stdin")?
        .write_all(revisions.as_bytes())?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "git rev-list --stdin failed: {}; inspect the branch reflog with `git reflog show`",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(count(&String::from_utf8_lossy(&output.stdout))? == 0)
}

/// Whether the owner paused CodeRabbit PR reviews: the all-scope `paused` marker or the PR-scope `paused-pr` marker.
pub fn pr_reviews_paused(home: &Path) -> Result<bool> {
    let state = home.join(".local/state/coderabbit-guard");
    for marker in ["paused", "paused-pr"] {
        if state
            .join(marker)
            .try_exists()
            .with_context(|| format!("inspect the CodeRabbit pause marker {marker}"))?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether the repository defines GitHub Actions workflows that report checks on a PR.
fn workflows(root: &Path) -> Result<bool> {
    let directory = root.join(".github/workflows");
    if !directory.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "yml" || extension == "yaml")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn inspect(root: &Path, environment: &Environment) -> Result<Observed> {
    let host = host_status(
        environment.temp.as_deref(),
        environment.cargo_target.as_deref(),
    );
    let branch = git_optional(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let changed_files = git(
        root,
        &["status", "--porcelain"],
        "run next-action inside a Git worktree",
    )?
    .lines()
    .count();
    if !environment.offline {
        git(
            root,
            &["fetch", "--quiet", "origin"],
            "check the network and credentials, or rerun with --offline to use the last fetched state",
        )?;
    }
    let repository = git_optional(root, &["remote", "get-url", "origin"])?
        .as_deref()
        .and_then(github_repository);
    let feature = branch.as_deref().filter(|branch| *branch != DEFAULT_BRANCH);
    let pull_request = match feature {
        Some(branch) if !environment.offline => pull_request(root, repository.as_deref(), branch)?,
        _ => None,
    };
    let base = pull_request
        .as_ref()
        .filter(|pr| pr.state == "OPEN")
        .map_or_else(|| BASE.to_owned(), |pr| format!("origin/{}", pr.base));
    let (behind_base, ahead_base) = divergence(
        root,
        &base,
        &format!(
            "fetch the base with `git fetch origin {}`",
            &base["origin/".len()..]
        ),
    )?;
    let upstream = git_optional(
        root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )?;
    let (unpulled, unpushed) = match upstream {
        Some(_) => divergence(root, "@{upstream}", "fetch the upstream branch")?,
        None => (0, ahead_base),
    };
    let upstream_commit = match upstream {
        Some(_) => Some(git(
            root,
            &["rev-parse", "@{upstream}"],
            "fetch the upstream branch",
        )?),
        None => None,
    };
    let upstream_held = match (&branch, &upstream_commit) {
        (Some(branch), Some(commit)) if unpushed > 0 && unpulled > 0 => {
            held_by_branch(root, branch, commit)?
        }
        _ => false,
    };
    let (undecided_skills, incomplete_decisions) = decision_backlog(root)?;
    Ok(Observed {
        host,
        branch,
        changed_files,
        base,
        behind_base,
        ahead_base,
        upstream,
        upstream_commit,
        upstream_held,
        unpushed,
        unpulled,
        undecided_skills,
        incomplete_decisions,
        push_paused: environment.home.join(".config/git/push-paused").exists(),
        review_paused: pr_reviews_paused(&environment.home)?,
        repository,
        workflows: workflows(root)?,
        github_inspected: !environment.offline,
        pull_request,
    })
}

pub fn recommend(state: Observed) -> Recommendation {
    let step = decide(state.state());
    let branch = state.branch.as_deref().unwrap_or("HEAD");
    let repository = state.repository.as_deref().unwrap_or("<OWNER/REPO>");
    let number = state
        .pull_request
        .as_ref()
        .map_or(0, |pull_request| pull_request.number);
    let base = &state.base;
    let upstream = state.upstream.as_deref().unwrap_or("the upstream");
    let host_path = match &state.host {
        HostStatus::Ready | HostStatus::TempUnset => String::new(),
        HostStatus::VolumeMissing { path } | HostStatus::TempMissing { path } => {
            path.display().to_string()
        }
    };
    let (summary, command) = match step {
        Step::SetTemp => (
            "TEMP is unset; set it to a directory on the volume that holds the build directories, then rerun `just next`".into(),
            None,
        ),
        Step::MountVolume => (
            format!(
                "Mount the volume {host_path} that holds the temporary and build directories (the Dev Drive on Windows), then rerun `just next`"
            ),
            None,
        ),
        Step::CreateTemp => (
            format!("Create the missing temporary directory {host_path}"),
            Some(format!("mkdir \"{host_path}\"")),
        ),
        Step::AttachBranch => (
            "HEAD is detached; switch to the branch for this work".into(),
            Some("git switch -c <type>/<topic>".into()),
        ),
        Step::BranchFromDefault => (
            format!("Work on {branch} belongs on a feature branch for its issue; move it there"),
            Some("git switch -c <type>/<topic>".into()),
        ),
        Step::FastForward => (
            format!(
                "Fast-forward {branch} by {} commits to {BASE}",
                state.behind_base
            ),
            Some(format!("git merge --ff-only {BASE}")),
        ),
        Step::StartBranch => (
            format!("{branch} matches {BASE}; branch from it for the next issue"),
            Some(format!("git switch -c <type>/<topic> {BASE}")),
        ),
        Step::DraftDecisions => {
            let skill = &state.undecided_skills[0];
            (
                format!(
                    "Draft the review record for {skill}, one of {} skills without a decision for their current revision",
                    state.undecided_skills.len()
                ),
                Some(format!("just decide {skill}")),
            )
        }
        Step::CompleteDecisions => (
            format!(
                "Write the reason, evidence, and any revisit condition in {DECISIONS}/{}.json, then run `just check`",
                state.incomplete_decisions[0]
            ),
            None,
        ),
        Step::Commit => (
            format!(
                "Review the {} uncommitted changes and commit them when a commit is authorized",
                state.changed_files
            ),
            Some("git status --short".into()),
        ),
        Step::Finished => (
            format!("PR #{number} is merged; retire {branch} and its worktree"),
            None,
        ),
        Step::Reopen => (
            format!("PR #{number} was closed without merging; reopen it or retire {branch}"),
            Some(format!("gh pr reopen {number} --repo {repository}")),
        ),
        Step::SyncUpstream => (
            format!(
                "{upstream} has {} commits that {branch} lacks; fast-forward to it before continuing",
                state.unpulled
            ),
            Some("git merge --ff-only @{upstream}".into()),
        ),
        Step::IntegrateUpstream => (
            format!(
                "{upstream} has {} commits that {branch} never held, such as a push from another clone; rebase the {} local commits onto them so a push keeps them",
                state.unpulled, state.unpushed
            ),
            Some("git rebase @{upstream}".into()),
        ),
        Step::Rebase => (
            format!(
                "{branch} is {} commits behind {base}; rebase it before publishing",
                state.behind_base
            ),
            Some(format!("git rebase {base}")),
        ),
        Step::StartWork => (
            format!("{branch} has no commits beyond {base}; implement the issue and commit"),
            None,
        ),
        Step::AwaitPushResume => (
            format!(
                "{} commits are unpushed, but push is paused on this machine; continue local work until the owner resumes pushing",
                state.unpushed
            ),
            None,
        ),
        Step::Push => (
            format!(
                "Push the {} unpushed commits of {branch} when a push is authorized",
                state.unpushed
            ),
            Some(format!("git push -u origin {branch}")),
        ),
        Step::ForcePush => (
            format!(
                "{branch} rewrote the {} upstream commits it once held into {} local commits; confirm the replacement with `git range-diff @{{upstream}}...HEAD`, then force-push when authorized",
                state.unpulled, state.unpushed
            ),
            Some(format!(
                "ALLOW_FORCE=1 git push --force-with-lease={branch}:{} origin {branch}",
                state.upstream_commit.as_deref().unwrap_or("<upstream commit>")
            )),
        ),
        Step::InspectGithub => (
            "The next step depends on the PR, but GitHub was not inspected; rerun without --offline".into(),
            Some("just next".into()),
        ),
        Step::CreatePr => (
            format!("Open a draft PR for {branch} through the checked PR workflow"),
            Some(format!(
                "pr-workflow create --repo {repository} --issue <issue> --title <title> --body-file <file> --head {branch}"
            )),
        ),
        Step::FixChecks => (
            format!("Checks fail on PR #{number}; inspect and fix them"),
            Some(format!("gh pr checks {number} --repo {repository}")),
        ),
        Step::AwaitChecks => (
            format!("Checks on PR #{number} have not finished"),
            Some(format!("gh pr checks {number} --repo {repository} --watch")),
        ),
        Step::AwaitReviewResume => (
            format!(
                "CodeRabbit is paused by the owner; leave PR #{number} unchanged and continue local work until review resumes"
            ),
            None,
        ),
        Step::MarkReady => (
            format!(
                "Checks pass on draft PR #{number}; move it to review once no work remains and it is authorized"
            ),
            Some(format!(
                "pr-workflow ready --repo {repository} --issue <issue> --pr {number}"
            )),
        ),
        Step::AwaitReview => (
            format!(
                "PR #{number} is in review; inspect the review of the current head and address its findings"
            ),
            Some(format!(
                "gh pr view {number} --repo {repository} --comments"
            )),
        ),
    };
    Recommendation {
        action: step,
        executable: executable(step),
        summary,
        command,
        state,
    }
}

/// Runs an executable step and returns what it printed; any other step is refused with its summary and command.
pub fn perform(root: &Path, recommendation: &Recommendation) -> Result<Option<String>> {
    let state = &recommendation.state;
    match (recommendation.action, &state.host) {
        (Step::CreateTemp, HostStatus::TempMissing { path }) => fs::create_dir_all(path)
            .map(|()| None)
            .with_context(|| format!("create {}; check its permissions", path.display())),
        (Step::FastForward, _) => git(
            root,
            &["merge", "--ff-only", BASE],
            "inspect `git status` and rerun `just next`",
        )
        .map(|_| None),
        (Step::SyncUpstream, _) => git(
            root,
            &["merge", "--ff-only", "@{upstream}"],
            "inspect `git status` and rerun `just next`",
        )
        .map(|_| None),
        (Step::DraftDecisions, _) => {
            crate::skill_ops::decide(root, &state.undecided_skills[0]).map(Some)
        }
        (action, _) => bail!(
            "`{}` is not a local, reversible step, so --execute does not run it; next action: {}{}",
            serde_json::to_value(action)?.as_str().unwrap_or_default(),
            recommendation.summary,
            recommendation
                .command
                .as_deref()
                .map(|command| format!(" (`{command}`)"))
                .unwrap_or_default()
        ),
    }
}

fn report(recommendation: &Recommendation) -> Result<()> {
    println!("{}", serde_json::to_string(recommendation)?);
    eprintln!("next: {}", recommendation.summary);
    Ok(())
}

/// Prints the recommendation as one JSON line on stdout and a summary on stderr; `--execute` first runs an executable step and then reports the step after it.
pub fn run(root: &Path, offline: bool, execute: bool) -> Result<()> {
    let environment = Environment::current(offline)?;
    let recommendation = recommend(inspect(root, &environment)?);
    if !execute {
        return report(&recommendation);
    }
    if let Some(output) = perform(root, &recommendation)? {
        eprintln!("{output}");
    }
    eprintln!("done: {}", recommendation.summary);
    report(&recommend(inspect(root, &environment)?))
}
