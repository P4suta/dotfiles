use super::evidence::{self, Packet};
use super::{Github, Target, context};
use anyhow::{Context, Result, ensure};
use rules::review_routing_rules::{self as policy, ReviewLevel, ReviewRoute};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod git_fixture;

const SCHEMA: u32 = 1;
const GIT_OBJECT_LENGTHS: [usize; 2] = [40, 64];
/// Starts the line that once carried a signed assessment in a PR body, which a published document now refuses.
pub(super) const PACKET_PREFIX: &str = "<!-- pr-review-assessment:";
const MAX_ASSESSMENT_BYTES: usize = 16 * 1024;
const REVIEW_EXCLUSION: &str = "@coderabbitai ignore";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 20;
const REVIEW_BOT: &str = "coderabbitai[bot]";
const GRAPHQL_BOT: &str = "coderabbitai";
const REVIEW_CONTEXT: &str = "CodeRabbit";
const RUN_ID_LENGTH: usize = 36;
const RUN_ID_SEPARATORS: [usize; 4] = [8, 13, 18, 23];
pub(super) const CRITICAL_LABEL: &str = "review:critical";

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Level {
    Low,
    Medium,
    High,
    Critical,
}

impl From<Level> for ReviewLevel {
    fn from(value: Level) -> Self {
        match value {
            Level::Low => Self::Low,
            Level::Medium => Self::Medium,
            Level::High => Self::High,
            Level::Critical => Self::Critical,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum SevereGround {
    AuthorizationBoundary,
    SecretBoundary,
    IrreversibleDataLoss,
    ExploitableSecurityBoundary,
    FoundationalAssuranceAcceptance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChangeEvidence {
    path: String,
    change: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SevereAssessment {
    ground: SevereGround,
    consequence: String,
    likelihood: String,
    blast_radius: String,
    rollback: String,
    consequence_kind: Consequence,
    likelihood_kind: Likelihood,
    blast_radius_kind: BlastRadius,
    rollback_kind: Rollback,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Consequence {
    Severe,
    Catastrophic,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Likelihood {
    Hypothetical,
    Credible,
    Exposed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum BlastRadius {
    Limited,
    Broad,
    Systemic,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Rollback {
    Recoverable,
    Difficult,
    Irreversible,
}

impl SevereAssessment {
    fn facts(&self) -> policy::SevereRisk {
        policy::SevereRisk {
            consequence: match self.consequence_kind {
                Consequence::Severe => policy::Consequence::Severe,
                Consequence::Catastrophic => policy::Consequence::Catastrophic,
            },
            likelihood: match self.likelihood_kind {
                Likelihood::Hypothetical => policy::Likelihood::Hypothetical,
                Likelihood::Credible => policy::Likelihood::Credible,
                Likelihood::Exposed => policy::Likelihood::Exposed,
            },
            blast_radius: match self.blast_radius_kind {
                BlastRadius::Limited => policy::BlastRadius::Limited,
                BlastRadius::Broad => policy::BlastRadius::Broad,
                BlastRadius::Systemic => policy::BlastRadius::Systemic,
            },
            rollback: match self.rollback_kind {
                Rollback::Recoverable => policy::Rollback::Recoverable,
                Rollback::Difficult => policy::Rollback::Difficult,
                Rollback::Irreversible => policy::Rollback::Irreversible,
            },
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            [
                &self.consequence,
                &self.likelihood,
                &self.blast_radius,
                &self.rollback
            ]
            .into_iter()
            .all(|text| rationale(text)),
            "severe assessment requires concrete consequence, likelihood, blast radius, and rollback reasons"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Assessment {
    level: Level,
    model: String,
    rationale: String,
    evidence: Vec<ChangeEvidence>,
    severe: Option<SevereAssessment>,
    #[serde(default)]
    finding_dispositions: Vec<FindingDisposition>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Resolved,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FindingDisposition {
    id: String,
    disposition: Disposition,
    reason: String,
    evidence: Vec<ChangeEvidence>,
}

fn rationale(text: &str) -> bool {
    !text.trim().is_empty()
        && !text
            .chars()
            .any(|value| value.is_control() && value != '\n')
}

impl Assessment {
    fn validate(&self, changed: &BTreeSet<String>) -> Result<()> {
        ensure!(
            rationale(&self.model) && rationale(&self.rationale),
            "every PR requires an AI assessment model and a concrete rationale"
        );
        ensure!(
            !self.evidence.is_empty(),
            "assessment requires evidence from the actual current diff"
        );
        let mut seen = BTreeSet::new();
        for evidence in &self.evidence {
            ensure!(
                changed.contains(&evidence.path)
                    && rationale(&evidence.change)
                    && seen.insert((&evidence.path, &evidence.change)),
                "assessment evidence is absent from the current diff, duplicated, or unexplained"
            );
        }
        if let Some(severe) = &self.severe {
            severe.validate()?;
        }
        ensure!(
            policy::assessment_allowed(
                self.level.into(),
                self.severe.as_ref().map(SevereAssessment::facts)
            ),
            "high and critical require credible severe evidence, and critical requires catastrophic broad impact with difficult rollback"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Binding {
    repository: String,
    base: String,
    head: String,
    diff_sha256: String,
}

#[derive(Debug)]
struct Current {
    root: PathBuf,
    binding: Binding,
    tree: String,
    changed: BTreeSet<String>,
    source: Option<platform::source_identity::Binding>,
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut arguments = vec!["--no-replace-objects"];
    arguments.extend_from_slice(args);
    platform::signed_git::git(root, &arguments, None).map(|text| text.trim().to_owned())
}

fn object(root: &Path, reference: &str) -> Result<String> {
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )
}

#[cfg(test)]
fn capture_at(root: &Path, repository: &str, base: &str, head: &str) -> Result<Current> {
    capture_source(root, repository, base, head, None)
}

fn capture_verified(
    github: &Github,
    root: &Path,
    repository: &str,
    base: &str,
    head: &str,
) -> Result<Current> {
    let source = super::source_identity(github, root, repository)?;
    let current = capture_source(root, repository, base, head, Some(source.binding().clone()))?;
    super::require_current_source(&source)?;
    Ok(current)
}

fn capture_source(
    root: &Path,
    repository: &str,
    base: &str,
    head: &str,
    source: Option<platform::source_identity::Binding>,
) -> Result<Current> {
    let root = platform::issue_context::checkout(root)?;
    super::require_checkout_origin(
        &Target {
            repo: source
                .as_ref()
                .map_or(repository, platform::source_identity::Binding::source)
                .into(),
            generation: rules::pr_rules::Generation::Local,
            title_policy: None,
            issue: None,
            cli_receipt: None,
            prior_cli_receipt: None,
        },
        platform::issue_context::binding(&root)?.origin(),
    )?;
    let head = object(&root, head)?;
    let base = git(&root, &["merge-base", &object(&root, base)?, &head])?;
    let tree = git(&root, &["rev-parse", &format!("{head}^{{tree}}")])?;
    let diff = platform::tool::Tool::Git
        .command()
        .current_dir(&root)
        .args([
            "--no-replace-objects",
            "--no-pager",
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            &base,
            &head,
            "--",
        ])
        .output()?;
    ensure!(
        diff.status.success(),
        "could not capture the exact review diff"
    );
    let paths = platform::signed_git::git(
        &root,
        &[
            "--no-replace-objects",
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            &base,
            &head,
            "--",
        ],
        None,
    )?;
    let changed = paths
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(Current {
        root,
        tree,
        changed,
        source,
        binding: Binding {
            repository: repository.into(),
            base,
            head,
            diff_sha256: platform::build_inputs::hex(&Sha256::digest(&diff.stdout)),
        },
    })
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Statement {
    schema: u32,
    signer: String,
    binding: Binding,
    assessment: Assessment,
    history: Option<HistoricalReview>,
    #[serde(default)]
    source: Option<platform::source_identity::Binding>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HistoricalReview {
    binding: Binding,
    object: String,
    #[serde(default)]
    source: Option<platform::source_identity::Binding>,
}

pub(super) struct Checked {
    pub route: ReviewRoute,
    /// The signed assessment object this check authenticated.
    pub object: String,
    binding: Binding,
    root: PathBuf,
    assessment: Assessment,
    signer: String,
    history: Option<HistoricalReview>,
    changed: BTreeSet<String>,
    source: platform::source_identity::Binding,
}

fn authenticate(
    current: &Current,
    packet: &Packet,
    signers: &Path,
    expected_signer: &str,
) -> Result<Statement> {
    let verified =
        platform::signed_git::authenticate(&current.root, &packet.object, &packet.raw, signers)?;
    let statement: Statement = serde_json::from_str(&verified.payload)
        .map_err(|_| anyhow::anyhow!("signed importance statement is malformed"))?;
    ensure!(
        statement.schema == SCHEMA
            && statement.signer == expected_signer
            && statement.binding == current.binding
            && verified.tree == current.tree
            && verified.parents == [current.binding.head.clone()],
        "importance assessment signature, repository, base, head, tree, or diff is stale or mismatched"
    );
    statement.assessment.validate(&current.changed)?;
    ensure!(
        statement.source == current.source
            || statement.source.is_none()
                && current
                    .source
                    .as_ref()
                    .is_some_and(platform::source_identity::Binding::same_repository),
        "importance assessment source and destination identities are stale or mismatched"
    );
    Ok(statement)
}

pub(super) fn check(
    github: &Github,
    target: &Target,
    body: &str,
    base: &str,
    head: &str,
    signer: &str,
) -> Result<Checked> {
    check_at(
        &std::env::current_dir()?,
        github,
        target,
        body,
        base,
        head,
        signer,
    )
}

pub(super) fn fetch_pr(root: &Path, target: &Target, base: &str, head: &str) -> Result<()> {
    ensure!(
        [base, head]
            .into_iter()
            .all(|object| GIT_OBJECT_LENGTHS.contains(&object.len())
                && object.bytes().all(|byte| byte.is_ascii_hexdigit())),
        "PR head and base must be full Git object names"
    );
    git(
        root,
        &[
            "fetch",
            "--quiet",
            &format!("{}/{}.git", super::GITHUB, target.repo),
            base,
            head,
        ],
    )?;
    Ok(())
}

pub(super) fn carry_forward(
    github: &Github,
    target: &Target,
    view: &super::MergeView,
    prior: &Checked,
) -> Result<()> {
    use rules::pr_merge_rules::{AssessmentChange, assessment_change};
    let current = capture_verified(
        github,
        &prior.root,
        &target.repo,
        view.base_ref_oid
            .as_deref()
            .context("updated PR base missing")?,
        &view.head_ref_oid,
    )?;
    let signer = assessment_signer(github, target, &view.author, &view.source)?;
    let decision = assessment_change(
        current.binding == prior.binding,
        current.binding.diff_sha256 == prior.binding.diff_sha256,
        current.source.as_ref() == Some(&prior.source) && signer == prior.signer,
    );
    match decision {
        AssessmentChange::Keep | AssessmentChange::Reassess => return Ok(()),
        AssessmentChange::Resign => {}
    }
    let signers = context::signers_for_login(github, signer)?;
    let old = capture_verified(
        github,
        &prior.root,
        &target.repo,
        &prior.binding.base,
        &prior.binding.head,
    )?;
    let packet = evidence::object(&target.repo, &prior.object)?;
    let mut statement = authenticate(&old, &packet, signers.path(), signer)?;
    statement.binding = current.binding;
    statement.source = current.source;
    let signature = platform::signed_git::create(
        &current.root,
        &statement.binding.head,
        &current.tree,
        &serde_json::to_vec(&statement)?,
    )?;
    let packet = Packet {
        object: signature.object,
        raw: signature.raw,
    };
    let rebound = capture_verified(
        github,
        &prior.root,
        &target.repo,
        view.base_ref_oid
            .as_deref()
            .context("updated PR base missing")?,
        &view.head_ref_oid,
    )?;
    authenticate(&rebound, &packet, signers.path(), signer)?;
    evidence::store_assessment(
        &platform::home::home().context("owner home missing")?,
        &target.repo,
        &statement.binding.head,
        &statement.binding.diff_sha256,
        &packet.object,
        &packet.raw,
    )?;
    Ok(())
}

fn check_at(
    root: &Path,
    github: &Github,
    target: &Target,
    body: &str,
    base: &str,
    head: &str,
    signer: &str,
) -> Result<Checked> {
    super::refuse_local_evidence(body, || super::recheck(target, None))?;
    let current = capture_verified(github, root, &target.repo, base, head)?;
    let signers = context::signers_for_login(github, signer)?;
    let packet = evidence::assessment(
        &current.binding.repository,
        &current.binding.head,
        &current.binding.diff_sha256,
    )?;
    let statement = authenticate(&current, &packet, signers.path(), signer)?;
    let route = policy::route(statement.assessment.level.into());
    let ignore_count = body
        .lines()
        .filter(|line| line.trim() == REVIEW_EXCLUSION)
        .count();
    ensure!(ignore_count <= 1, "duplicate CodeRabbit suppression lines");
    if route != ReviewRoute::Pr {
        ensure!(
            ignore_count == 1 && target.generation == rules::pr_rules::Generation::Local,
            "low, medium, and high use a standing CodeRabbit PR suppression and local document generation"
        );
    }
    Ok(Checked {
        route,
        object: packet.object,
        binding: current.binding,
        root: current.root,
        assessment: statement.assessment,
        signer: signer.into(),
        history: statement.history,
        changed: current.changed,
        source: current
            .source
            .context("verified assessment source missing")?,
    })
}

pub(super) fn assess(
    github: &Github,
    target: &Target,
    base: &str,
    assessment_file: &Path,
    pr: Option<u64>,
) -> Result<()> {
    ensure!(
        fs::symlink_metadata(assessment_file)?.is_file(),
        "AI assessment must be a regular file"
    );
    let file = fs::File::open(assessment_file)?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(MAX_ASSESSMENT_BYTES)? + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_ASSESSMENT_BYTES,
        "AI assessment exceeds its bounded input"
    );
    let assessment: Assessment = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("AI assessment is malformed"))?;
    let current = capture_verified(
        github,
        &std::env::current_dir()?,
        &target.repo,
        base,
        "HEAD",
    )?;
    assessment.validate(&current.changed)?;
    let history = prepare_history(github, target, pr, &current, &assessment)?;
    let statement = Statement {
        schema: SCHEMA,
        signer: github.login.clone(),
        binding: current.binding,
        assessment,
        history,
        source: current.source,
    };
    let signature = platform::signed_git::create(
        &current.root,
        &statement.binding.head,
        &current.tree,
        &serde_json::to_vec(&statement)?,
    )?;
    let packet = Packet {
        object: signature.object,
        raw: signature.raw,
    };
    let signers = context::signers_for_login(github, &github.login)?;
    let final_current = capture_verified(
        github,
        &std::env::current_dir()?,
        &target.repo,
        base,
        "HEAD",
    )?;
    authenticate(&final_current, &packet, signers.path(), &github.login)?;
    let stored = evidence::store_assessment(
        &platform::home::home().context("the owner home for the assessment is unavailable")?,
        &statement.binding.repository,
        &statement.binding.head,
        &statement.binding.diff_sha256,
        &packet.object,
        &packet.raw,
    )?;
    println!(
        "Current-diff importance assessment saved in owner-local state at {}; this is not review completion",
        stored.display()
    );
    Ok(())
}

pub(super) fn paused(checked: &Checked) -> Result<bool> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .context("review owner home is unavailable")?;
    let scope = coderabbit_guard::pause(&home)?;
    Ok(match checked.route {
        ReviewRoute::None => false,
        ReviewRoute::Cli => coderabbit_guard::review_rules::cli_reviews_paused(scope),
        ReviewRoute::Pr => coderabbit_guard::review_rules::pr_reviews_paused(scope),
    })
}

pub(super) fn completion(
    checked: &Checked,
    target: &Target,
    github: &Github,
    pr: u64,
) -> Result<()> {
    let initially_paused = paused(checked)?;
    ensure!(
        !initially_paused,
        "the required review route is paused; this PR cannot be declared complete"
    );
    require_resolved_threads(github, target, pr, &checked.binding.head)?;
    let mut findings = history_findings(checked, target, github)?;
    match checked.route {
        ReviewRoute::None => ensure!(
            target.cli_receipt.is_none(),
            "this PR must not supply an unrelated CLI review"
        ),
        ReviewRoute::Cli => {
            let path = target
                .cli_receipt
                .as_ref()
                .context("high importance requires a current guarded CLI receipt")?;
            let packet = coderabbit_guard::read_cli_receipt(path)?;
            let signers = context::signers_for_login(github, &checked.signer)?;
            let binding = cli_binding(&checked.root, &checked.binding, Some(&checked.source))?;
            let review = coderabbit_guard::require_cli_evidence(
                &checked.root,
                &binding,
                &packet,
                signers.path(),
            )?;
            findings.extend(review.findings);
        }
        ReviewRoute::Pr => {
            ensure!(
                target.cli_receipt.is_none(),
                "critical importance requires PR review, not a CLI substitute"
            );
            require_pr_review(github, target, pr, &checked.binding, &checked.assessment)?;
        }
    }
    let mut unique = std::collections::BTreeMap::new();
    for finding in findings {
        unique.entry(finding.id.clone()).or_insert(finding);
    }
    let findings = unique.into_values().collect::<Vec<_>>();
    validate_findings(
        &checked.assessment,
        &findings,
        &checked.root,
        &checked.binding.head,
        &checked.changed,
        true,
    )?;
    let evidence = match checked.route {
        ReviewRoute::None => policy::ReviewEvidence::None,
        ReviewRoute::Cli => policy::ReviewEvidence::Cli,
        ReviewRoute::Pr => policy::ReviewEvidence::Pr,
    };
    let paused = paused(checked)?;
    ensure!(
        policy::completion(
            checked.route,
            checked.route == ReviewRoute::Cli && paused,
            checked.route == ReviewRoute::Pr && paused,
            evidence,
            true
        ) == policy::Completion::Accepted,
        "selected review route has not completed with resolved supported findings"
    );
    Ok(())
}

fn validate_findings(
    assessment: &Assessment,
    findings: &[coderabbit_guard::CliFinding],
    root: &Path,
    head: &str,
    changed: &BTreeSet<String>,
    exact: bool,
) -> Result<()> {
    let actual: BTreeSet<_> = findings.iter().map(|finding| finding.id.as_str()).collect();
    ensure!(
        actual.len() == findings.len(),
        "CLI finding inventory is duplicated"
    );
    let mut dispositions = BTreeSet::new();
    for disposition in &assessment.finding_dispositions {
        ensure!(
            (!exact || actual.contains(disposition.id.as_str()))
                && dispositions.insert(disposition.id.as_str())
                && rationale(&disposition.reason)
                && !disposition.evidence.is_empty(),
            "CLI finding disposition is unexplained, duplicated, or belongs to another review"
        );
        if !actual.contains(disposition.id.as_str()) {
            continue;
        }
        let finding = findings
            .iter()
            .find(|finding| finding.id == disposition.id)
            .context("actual CLI finding missing")?;
        ensure!(
            disposition
                .evidence
                .iter()
                .all(|evidence| rationale(&evidence.change))
                && disposition
                    .evidence
                    .iter()
                    .any(|evidence| evidence.path == finding.path),
            "CLI finding disposition needs concrete current-source evidence at the reported path"
        );
        for evidence in &disposition.evidence {
            platform::issue_context::relative(Path::new(&evidence.path))?;
            match disposition.disposition {
                Disposition::Resolved => ensure!(
                    changed.contains(&evidence.path),
                    "resolved CLI finding requires evidence in the actual current diff"
                ),
                Disposition::Unsupported => {
                    git(
                        root,
                        &["cat-file", "-e", &format!("{head}:{}", evidence.path)],
                    )?;
                }
            }
        }
    }
    ensure!(
        if exact {
            dispositions == actual
        } else {
            actual.is_subset(&dispositions)
        },
        "current CLI findings still need supported fixes or justified unsupported dispositions"
    );
    Ok(())
}

pub(super) fn labels(checked: &Checked, labels: &[super::ReviewLabel]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for label in labels {
        ensure!(seen.insert(&label.name), "duplicate PR label");
    }
    ensure!(
        checked.route == ReviewRoute::Pr || !seen.contains(&CRITICAL_LABEL.to_owned()),
        "noncritical PR cannot opt into automatic CodeRabbit PR review"
    );
    Ok(())
}

pub(super) fn describe(checked: &Checked) -> Result<()> {
    println!(
        "CodeRabbit review route: {:?}; required route paused: {}; assessment is not review completion",
        checked.route,
        paused(checked)?
    );
    Ok(())
}

#[derive(Deserialize)]
struct Bot {
    login: String,
}

#[derive(Deserialize)]
struct SubmittedReview {
    id: u64,
    user: Bot,
    commit_id: String,
    state: String,
    submitted_at: Option<String>,
    body: String,
}

#[derive(Deserialize)]
struct CommitStatus {
    id: u64,
    creator: Bot,
    context: String,
    state: String,
    description: String,
}

fn pages<T: serde::de::DeserializeOwned>(github: &Github, endpoint: &str) -> Result<Vec<T>> {
    let mut all = Vec::new();
    for page in 1..=MAX_PAGES {
        let items: Vec<T> = github.read(&format!("{endpoint}?per_page={PAGE_SIZE}&page={page}"))?;
        ensure!(
            items.len() <= PAGE_SIZE,
            "review API page exceeds its contract"
        );
        let last = items.len() < PAGE_SIZE;
        all.extend(items);
        if last {
            return Ok(all);
        }
    }
    anyhow::bail!("review API inventory exceeds its bound; no partial review credit")
}

fn require_pr_review(
    github: &Github,
    target: &Target,
    pr: u64,
    binding: &Binding,
    assessment: &Assessment,
) -> Result<()> {
    let head = &binding.head;
    let reviews: Vec<SubmittedReview> =
        pages(github, &format!("repos/{}/pulls/{pr}/reviews", target.repo))?;
    let statuses: Vec<CommitStatus> = pages(
        github,
        &format!("repos/{}/commits/{head}/statuses", target.repo),
    )?;
    validate_pr_evidence(&reviews, &statuses, binding, assessment, pr)?;
    let live = super::live_document(github, target, pr)?;
    ensure!(
        live.head_ref_oid.as_deref() == Some(head.as_str()),
        "PR head changed while review evidence was inspected"
    );
    Ok(())
}

fn validate_pr_evidence(
    reviews: &[SubmittedReview],
    statuses: &[CommitStatus],
    binding: &Binding,
    assessment: &Assessment,
    pr: u64,
) -> Result<()> {
    let review = reviews
        .iter()
        .filter(|review| review.user.login == REVIEW_BOT)
        .max_by_key(|review| review.id)
        .ok_or_else(|| {
            missing_pr_review(
                binding,
                pr,
                "critical PR has no submitted CodeRabbit review of its current head",
            )
        })?;
    validate_submitted_review(review, binding, assessment)?;
    let status = statuses
        .iter()
        .filter(|status| status.creator.login == REVIEW_BOT && status.context == REVIEW_CONTEXT)
        .max_by_key(|status| status.id)
        .ok_or_else(|| {
            missing_pr_review(
                binding,
                pr,
                "current-head CodeRabbit terminal status is unavailable",
            )
        })?;
    ensure!(
        status.state == "success" && status.description == "Review completed",
        "current-head CodeRabbit terminal is skipped, partial, pending, failed, or unknown"
    );
    Ok(())
}

fn missing_pr_review(binding: &Binding, pr: u64, cause: &str) -> platform::refusal::Refusal {
    platform::refusal::Refusal::new(
        "pr.review",
        cause,
        platform::refusal::command(&[
            "gh",
            "pr",
            "view",
            &pr.to_string(),
            "--repo",
            &binding.repository,
            "--comments",
        ]),
    )
    .evidence(format!(
        "{}#{pr}: full review {}..{}; diff SHA256 {}",
        binding.repository, binding.base, binding.head, binding.diff_sha256
    ))
}

fn validate_submitted_review(
    review: &SubmittedReview,
    binding: &Binding,
    assessment: &Assessment,
) -> Result<()> {
    ensure!(
        review.id != 0
            && review.commit_id == binding.head
            && matches!(review.state.as_str(), "COMMENTED" | "APPROVED")
            && review
                .submitted_at
                .as_ref()
                .is_some_and(|time| !time.is_empty())
            && !review.body.trim().is_empty(),
        "current-head CodeRabbit review is pending, dismissed, empty, or requests changes"
    );
    let files = full_review_body(&review.body, &binding.base, &binding.head)?;
    ensure!(
        assessment
            .evidence
            .iter()
            .all(|evidence| files.contains(&evidence.path)),
        "critical severity-bearing source paths were not selected by the completed full PR review"
    );
    Ok(())
}

fn full_review_body(body: &str, expected_base: &str, head: &str) -> Result<BTreeSet<String>> {
    const FOOTER: &str =
        "<!-- This is an auto-generated comment by CodeRabbit for review status -->";
    const COMMENTS: &str = "**Actionable comments posted: ";
    const RUN: &str = "- **Run ID**: `";
    const RANGE: &str = "Reviewing files that changed from the base of the PR and between ";
    const FILES: &str = "<summary>📒 Files selected for processing (";
    ensure!(
        body.lines().last() == Some(FOOTER) && body.matches(FOOTER).count() == 1,
        "CodeRabbit submitted review has no recognized complete-review body"
    );
    let comments = body
        .lines()
        .next()
        .and_then(|line| line.strip_prefix(COMMENTS))
        .and_then(|line| line.strip_suffix("**"))
        .context("CodeRabbit actionable inventory missing")?;
    comments
        .parse::<usize>()
        .context("CodeRabbit actionable inventory malformed")?;
    let unique = |prefix: &str| -> Result<&str> {
        let mut found = body.lines().filter_map(|line| line.strip_prefix(prefix));
        let value = found
            .next()
            .context("CodeRabbit full-review metadata missing")?;
        ensure!(
            found.next().is_none(),
            "CodeRabbit full-review metadata is ambiguous"
        );
        Ok(value)
    };
    let run = unique(RUN)?
        .strip_suffix('`')
        .context("CodeRabbit run identity malformed")?;
    ensure!(
        run.len() == RUN_ID_LENGTH
            && run
                .bytes()
                .enumerate()
                .all(|(index, byte)| if RUN_ID_SEPARATORS.contains(&index) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }),
        "CodeRabbit run identity malformed"
    );
    let range = unique(RANGE)?
        .strip_suffix('.')
        .context("CodeRabbit reviewed range malformed")?;
    let (base, reviewed) = range
        .split_once(" and ")
        .context("CodeRabbit reviewed range missing")?;
    ensure!(
        base == expected_base && reviewed == head,
        "critical review must cover the full canonical PR base-to-head range"
    );
    let count = unique(FILES)?
        .strip_suffix(")</summary>")
        .context("CodeRabbit selected file inventory malformed")?
        .parse::<usize>()?;
    ensure!(
        count > 0,
        "CodeRabbit selected no files for the critical review"
    );
    let selected = body
        .split_once(FILES)
        .context("CodeRabbit selected files missing")?
        .1
        .split_once("</details>")
        .context("CodeRabbit selected files unterminated")?
        .0;
    let mut files = BTreeSet::new();
    for line in selected.lines().filter_map(|line| line.strip_prefix("* `")) {
        let path = line
            .strip_suffix('`')
            .context("CodeRabbit selected path malformed")?;
        platform::issue_context::relative(Path::new(path))?;
        ensure!(
            files.insert(path.to_owned()),
            "CodeRabbit selected file inventory duplicated"
        );
    }
    ensure!(
        files.len() == count,
        "CodeRabbit selected file inventory is incomplete"
    );
    Ok(files)
}

const THREADS_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!,$cursor:String){repository(owner:$owner,name:$name){pullRequest(number:$number){headRefOid reviewThreads(first:100,after:$cursor){pageInfo{hasNextPage endCursor} nodes{id isResolved isOutdated comments(first:100){pageInfo{hasNextPage endCursor} nodes{author{login} body}}}}}}}";
const COMMENTS_QUERY: &str = "query($id:ID!,$cursor:String){node(id:$id){... on PullRequestReviewThread{comments(first:100,after:$cursor){pageInfo{hasNextPage endCursor} nodes{author{login} body}}}}}";

#[derive(Deserialize)]
struct Graphql<T> {
    data: T,
    errors: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection<T> {
    page_info: PageInfo,
    nodes: Vec<T>,
}

#[derive(Deserialize)]
struct ReviewComment {
    author: Option<Bot>,
    body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewThread {
    id: String,
    is_resolved: bool,
    comments: Connection<ReviewComment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadPr {
    head_ref_oid: String,
    review_threads: Connection<ReviewThread>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadRepository {
    pull_request: ThreadPr,
}

#[derive(Deserialize)]
struct ThreadsData {
    repository: ThreadRepository,
}

#[derive(Deserialize)]
struct CommentsNode {
    comments: Connection<ReviewComment>,
}

#[derive(Deserialize)]
struct CommentsData {
    node: CommentsNode,
}

fn graph_data<T>(response: Graphql<T>) -> Result<T> {
    ensure!(
        response.errors.is_none(),
        "GitHub review evidence has GraphQL errors"
    );
    Ok(response.data)
}

fn next_cursor(info: &PageInfo, seen: &mut BTreeSet<String>) -> Result<Option<String>> {
    if !info.has_next_page {
        return Ok(None);
    }
    let cursor = info
        .end_cursor
        .as_ref()
        .filter(|cursor| !cursor.is_empty())
        .context("review evidence pagination cursor missing")?;
    ensure!(
        seen.insert(cursor.clone()),
        "review evidence pagination cursor repeated"
    );
    Ok(Some(cursor.clone()))
}

fn bot_comment(comments: &[ReviewComment]) -> Result<bool> {
    ensure!(
        comments.len() <= PAGE_SIZE,
        "review comment page exceeds its bound"
    );
    ensure!(
        comments
            .iter()
            .all(|comment| comment.author.is_some() && !comment.body.trim().is_empty()),
        "unresolved review comment has incomplete author or body evidence"
    );
    Ok(comments.iter().any(|comment| {
        comment
            .author
            .as_ref()
            .is_some_and(|author| matches!(author.login.as_str(), GRAPHQL_BOT | REVIEW_BOT))
    }))
}

fn thread_has_bot(github: &Github, thread: &ReviewThread) -> Result<bool> {
    let mut found = bot_comment(&thread.comments.nodes)?;
    let mut cursors = BTreeSet::new();
    let mut cursor = next_cursor(&thread.comments.page_info, &mut cursors)?;
    for _ in 0..MAX_PAGES {
        let Some(after) = cursor else {
            return Ok(found);
        };
        let data: CommentsData = graph_data(github.graphql(
            COMMENTS_QUERY,
            &serde_json::json!({"id":thread.id,"cursor":after}),
        )?)?;
        found |= bot_comment(&data.node.comments.nodes)?;
        cursor = next_cursor(&data.node.comments.page_info, &mut cursors)?;
    }
    anyhow::bail!("review thread comments exceed their complete-read bound")
}

fn require_resolved_threads(github: &Github, target: &Target, pr: u64, head: &str) -> Result<()> {
    let (owner, name) = target
        .repo
        .split_once('/')
        .context("review repository identity malformed")?;
    let mut cursor: Option<String> = None;
    let mut cursors = BTreeSet::new();
    let mut threads = BTreeSet::new();
    for _ in 0..MAX_PAGES {
        let data: ThreadsData = graph_data(github.graphql(
            THREADS_QUERY,
            &serde_json::json!({"owner":owner,"name":name,"number":pr,"cursor":cursor}),
        )?)?;
        let report = data.repository.pull_request;
        ensure!(
            report.head_ref_oid == head && report.review_threads.nodes.len() <= PAGE_SIZE,
            "review thread evidence belongs to another head or exceeds its page bound"
        );
        for thread in report.review_threads.nodes {
            ensure!(
                !thread.id.is_empty() && threads.insert(thread.id.clone()),
                "duplicate or missing review thread identity"
            );
            if !thread.is_resolved {
                ensure!(
                    !thread_has_bot(github, &thread)?,
                    "a prior CodeRabbit finding remains unresolved, including outdated threads"
                );
            }
        }
        cursor = next_cursor(&report.review_threads.page_info, &mut cursors)?;
        if cursor.is_none() {
            return Ok(());
        }
    }
    anyhow::bail!("review thread inventory exceeds its complete-read bound")
}

#[derive(Deserialize)]
struct CommitObject {
    sha: String,
}

#[derive(Deserialize)]
struct Branch {
    name: String,
    commit: CommitObject,
}

pub(super) fn path_segment(value: &str) -> Result<String> {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(encoded, "%{byte:02X}").context("encode GitHub branch path")?;
        }
    }
    Ok(encoded)
}

pub(super) fn create_binding(
    github: &Github,
    target: &Target,
    body: &str,
    head: &str,
    base: Option<&str>,
) -> Result<Checked> {
    let default: super::Repository;
    let base = if let Some(base) = base {
        base
    } else {
        default = github.repository(&target.repo)?;
        default
            .default_branch
            .as_deref()
            .context("actual default PR base branch missing")?
    };
    super::branch(base)?;
    let branch: Branch = github.read(&format!(
        "repos/{}/branches/{}",
        target.repo,
        path_segment(base)?
    ))?;
    ensure!(branch.name == base, "actual PR base branch differs");
    let root = std::env::current_dir()?;
    let source = super::source_identity(github, &root, &target.repo)?;
    let head = platform::source_identity::head_branch(&root, source.binding(), head)?;
    let reference = format!("refs/heads/{head}");
    if !source.binding().same_repository() {
        let published: Branch = github.read(&format!(
            "repos/{}/branches/{}",
            source.binding().source(),
            path_segment(&head)?
        ))?;
        ensure!(
            published.name == head && published.commit.sha == object(&root, &reference)?,
            "actual fork head branch differs from the local assessed source"
        );
    }
    check(
        github,
        target,
        body,
        &branch.commit.sha,
        &reference,
        &github.login,
    )
}

pub(super) fn live_binding(
    github: &Github,
    target: &Target,
    body: &str,
    live: &super::LiveDocument,
) -> Result<Checked> {
    live_binding_at(&std::env::current_dir()?, github, target, body, live)
}

pub(super) fn live_binding_at(
    root: &Path,
    github: &Github,
    target: &Target,
    body: &str,
    live: &super::LiveDocument,
) -> Result<Checked> {
    fetch_pr(
        root,
        target,
        live.base_ref_oid
            .as_deref()
            .context("actual PR base commit is missing")?,
        live.head_ref_oid
            .as_deref()
            .context("actual PR head commit is missing")?,
    )?;
    let signer = assessment_signer(github, target, &live.author, &live.source)?;
    let checked = check_at(
        root,
        github,
        target,
        body,
        live.base_ref_oid
            .as_deref()
            .context("actual PR base commit is missing")?,
        live.head_ref_oid
            .as_deref()
            .context("actual PR head commit is missing")?,
        signer,
    )?;
    validate_live_source(&checked, &live.source)?;
    labels(&checked, &live.labels)?;
    Ok(checked)
}

pub(super) fn assessment_signer<'a>(
    github: &'a Github,
    target: &Target,
    creator: &'a super::ReviewAuthor,
    source: &super::LiveSource,
) -> Result<&'a str> {
    let repository = github.repository(&target.repo)?;
    match policy::assessment_signer(
        creator.is_bot,
        github.login.eq_ignore_ascii_case(&repository.owner.login),
        source.is_cross_repository == Some(false),
    ) {
        policy::AssessmentSigner::Creator => Ok(&creator.login),
        policy::AssessmentSigner::RepositoryOwner => Ok(&github.login),
        policy::AssessmentSigner::Unavailable => anyhow::bail!(
            "bot assessments require the authenticated repository owner and the same source repository"
        ),
    }
}

pub(super) fn qualified_head(checked: &Checked, head: &str) -> Result<String> {
    let branch = platform::source_identity::head_branch(&checked.root, &checked.source, head)?;
    if checked.source.same_repository() {
        Ok(head.to_owned())
    } else {
        let owner = checked
            .source
            .source()
            .split('/')
            .next()
            .context("verified source owner missing")?;
        Ok(format!("{owner}:{branch}"))
    }
}

pub(super) fn validate_live_source(checked: &Checked, source: &super::LiveSource) -> Result<()> {
    let supplied = source.head_repository.is_some()
        || source.head_repository_owner.is_some()
        || source.is_cross_repository.is_some();
    if checked.source.same_repository() && !supplied {
        return Ok(());
    }
    let repository = source
        .head_repository
        .as_ref()
        .context("actual PR source repository missing")?;
    let owner = source
        .head_repository_owner
        .as_ref()
        .context("actual PR source owner missing")?;
    ensure!(
        format!("{}/{}", owner.login, repository.name)
            .eq_ignore_ascii_case(checked.source.source())
            && source.is_cross_repository == Some(!checked.source.same_repository()),
        "actual PR head repository differs from the verified checkout source"
    );
    let branch = source
        .head_ref_name
        .as_deref()
        .context("actual PR source branch missing")?;
    let branch = platform::source_identity::head_branch(&checked.root, &checked.source, branch)?;
    ensure!(
        object(&checked.root, &format!("refs/heads/{branch}"))? == checked.binding.head,
        "actual PR head branch differs from the locally assessed source"
    );
    Ok(())
}

pub(super) fn admit_ready(
    checked: &Checked,
    target: &Target,
    github: &Github,
    pr: u64,
    body: &str,
) -> Result<()> {
    if checked.route == ReviewRoute::Pr {
        ensure!(
            !paused(checked)?,
            "critical PR review is paused; keep this PR draft until its required route resumes"
        );
        ensure!(
            !body.lines().any(|line| line.trim() == REVIEW_EXCLUSION),
            "critical PR must remove review suppression before requesting review"
        );
        require_history(checked, target, github)?;
        require_resolved_threads(github, target, pr, &checked.binding.head)
    } else {
        completion(checked, target, github, pr)
    }
}

/// The assessment that `pr` last verified on this machine.
fn previous(
    github: &Github,
    target: &Target,
    pr: u64,
    signer: &str,
    root: &Path,
) -> Result<Option<Statement>> {
    let Some(object) = evidence::remembered(&target.repo, pr)? else {
        return Ok(None);
    };
    let packet = evidence::object(&target.repo, &object)?;
    let (_, payload) = packet
        .raw
        .split_once("\n\n")
        .context("prior importance signed body missing")?;
    let prior: Statement = serde_json::from_str(payload)
        .map_err(|_| anyhow::anyhow!("prior importance statement malformed"))?;
    let current = capture_verified(
        github,
        root,
        &target.repo,
        &prior.binding.base,
        &prior.binding.head,
    )?;
    let signers = context::signers_for_login(github, signer)?;
    Ok(Some(authenticate(
        &current,
        &packet,
        signers.path(),
        signer,
    )?))
}

fn cli_binding(
    root: &Path,
    binding: &Binding,
    source: Option<&platform::source_identity::Binding>,
) -> Result<coderabbit_guard::ReviewBinding> {
    let repository = platform::source_identity::repository(root)?;
    let expected = source.map_or(
        binding.repository.as_str(),
        platform::source_identity::Binding::source,
    );
    ensure!(
        repository.eq_ignore_ascii_case(expected)
            && source
                .is_none_or(|source| source.target().eq_ignore_ascii_case(&binding.repository)),
        "CLI evidence source differs from the checked source and destination composition"
    );
    let captured =
        coderabbit_guard::ReviewBinding::capture(root, &repository, &binding.base, &binding.head)?;
    ensure!(
        captured.diff_sha256 == binding.diff_sha256,
        "CLI evidence diff differs from the signed assessment"
    );
    Ok(captured)
}

fn historical_review(
    github: &Github,
    target: &Target,
    binding: &Binding,
    signer: &str,
    root: &Path,
    source: Option<&platform::source_identity::Binding>,
) -> Result<(String, Vec<coderabbit_guard::CliFinding>)> {
    let path = target.prior_cli_receipt.as_ref().context("prior high review history is unknown; provide its exact private --prior-cli-receipt before changing the route")?;
    let packet = coderabbit_guard::read_cli_receipt(path)?;
    let signers = context::signers_for_login(github, signer)?;
    let binding = cli_binding(root, binding, source)?;
    let findings =
        coderabbit_guard::require_historical_cli_findings(root, &binding, &packet, signers.path())?;
    Ok((packet.object, findings))
}

fn prepare_history(
    github: &Github,
    target: &Target,
    pr: Option<u64>,
    current: &Current,
    assessment: &Assessment,
) -> Result<Option<HistoricalReview>> {
    let Some(pr) = pr else {
        ensure!(
            target.prior_cli_receipt.is_none(),
            "historical CLI evidence requires the actual --pr being reassessed"
        );
        return Ok(None);
    };
    let live = super::live_document(github, target, pr)?;
    let signer = assessment_signer(github, target, &live.author, &live.source)?;
    let Some(prior) = previous(github, target, pr, signer, &current.root)? else {
        return Ok(None);
    };
    if policy::route(prior.assessment.level.into()) == ReviewRoute::Cli
        && policy::route(assessment.level.into()) != ReviewRoute::Cli
    {
        let (object, findings) = historical_review(
            github,
            target,
            &prior.binding,
            signer,
            &current.root,
            prior.source.as_ref(),
        )?;
        validate_findings(
            assessment,
            &findings,
            &current.root,
            &current.binding.head,
            &current.changed,
            false,
        )?;
        Ok(Some(HistoricalReview {
            binding: prior.binding,
            object,
            source: prior.source,
        }))
    } else {
        Ok(prior.history)
    }
}

fn require_history(checked: &Checked, target: &Target, github: &Github) -> Result<()> {
    let findings = history_findings(checked, target, github)?;
    validate_findings(
        &checked.assessment,
        &findings,
        &checked.root,
        &checked.binding.head,
        &checked.changed,
        false,
    )
}

fn history_findings(
    checked: &Checked,
    target: &Target,
    github: &Github,
) -> Result<Vec<coderabbit_guard::CliFinding>> {
    let Some(history) = &checked.history else {
        return Ok(Vec::new());
    };
    ensure!(
        history.binding.repository == checked.binding.repository
            && (history.source.as_ref() == Some(&checked.source)
                || history.source.is_none() && checked.source.same_repository()),
        "historical CLI repository differs"
    );
    git(
        &checked.root,
        &[
            "merge-base",
            "--is-ancestor",
            &history.binding.head,
            &checked.binding.head,
        ],
    )?;
    let (object, findings) = historical_review(
        github,
        target,
        &history.binding,
        &checked.signer,
        &checked.root,
        history.source.as_ref(),
    )?;
    ensure!(
        object == history.object,
        "historical CLI receipt was replaced"
    );
    Ok(findings)
}

pub(super) fn preserve_edit(
    github: &Github,
    target: &Target,
    pr: u64,
    checked: &Checked,
) -> Result<()> {
    if let Some(prior) = previous(github, target, pr, &checked.signer, &checked.root)? {
        if policy::route(prior.assessment.level.into()) == ReviewRoute::Cli
            && checked.route != ReviewRoute::Cli
        {
            let history = checked.history.as_ref().context("prior high review history must be carried into the signed reassessment; run assess --pr with --prior-cli-receipt")?;
            ensure!(
                history.binding == prior.binding && history.source == prior.source,
                "reassessment omitted or substituted the prior high scope"
            );
        } else if let Some(history) = prior.history {
            let current = checked
                .history
                .as_ref()
                .context("reassessment dropped historical CLI findings")?;
            ensure!(
                history.binding == current.binding
                    && history.object == current.object
                    && history.source == current.source,
                "historical CLI finding identity changed"
            );
        }
    }
    require_history(checked, target, github)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_source_failures_preserve_their_original_error_types() {
        let transport = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "fixture transport failure",
        );
        assert!(crate::source_identity_error(transport.into()).is::<std::io::Error>());
        let decode =
            serde_json::from_str::<platform::source_identity::Repository>("{}").unwrap_err();
        assert!(crate::source_identity_error(decode.into()).is::<serde_json::Error>());
        let unknown = anyhow::anyhow!("checkout source changed after repository verification");
        assert!(
            crate::source_identity_error(unknown)
                .downcast_ref::<platform::refusal::Refusal>()
                .is_none()
        );
        for rule in ["pr.auth", "pr.quota", "pr.repository"] {
            let refusal = platform::refusal::Refusal::new(
                rule,
                "original observed refusal",
                "inspect the original gate",
            );
            let mapped = crate::source_identity_error(refusal.clone().into());
            assert_eq!(
                mapped.downcast_ref::<platform::refusal::Refusal>(),
                Some(&refusal)
            );
        }
    }

    fn input(level: &str) -> String {
        format!(
            r#"{{"level":"{level}","model":"fixture-model","rationale":"A bounded documented change.","evidence":[{{"path":"README.md","change":"Corrects one documented example."}}],"severe":null}}"#
        )
    }

    #[test]
    fn assessments_are_closed_rationalized_and_bound_to_changed_evidence() -> Result<()> {
        let changed = BTreeSet::from(["README.md".into()]);
        for level in ["low", "medium"] {
            let assessment: Assessment = serde_json::from_str(&input(level))?;
            assessment.validate(&changed)?;
            assert!(assessment.validate(&BTreeSet::new()).is_err());
        }
        for invalid in [
            input("unknown"),
            input("low").replace("\"model\":\"fixture-model\"", "\"model\":null"),
            input("low").replace(
                "\"level\":\"low\"",
                "\"level\":\"low\",\"level\":\"medium\"",
            ),
            input("low").replace("\"severe\":null", "\"severe\":null,\"unrecognized\":true"),
        ] {
            assert!(serde_json::from_str::<Assessment>(&invalid).is_err());
        }
        for level in ["high", "critical"] {
            let assessment: Assessment = serde_json::from_str(&input(level))?;
            assert!(assessment.validate(&changed).is_err());
        }
        Ok(())
    }

    #[test]
    fn a_published_document_refuses_local_workflow_evidence() {
        let recheck = || "pr-workflow check".to_owned();
        crate::refuse_local_evidence("## Why\n\nA reason.\n\n@coderabbitai ignore\n", recheck)
            .expect("the standing ignore line stays allowed");
        for body in [
            format!("Text.\n{PACKET_PREFIX} {{\"object\":\"o\",\"raw\":\"r\"}} -->\n"),
            format!("  {PACKET_PREFIX}{{}} -->"),
            format!("Text.\n{}\n", crate::PAUSE_EXCLUSION),
        ] {
            let error = crate::refuse_local_evidence(&body, recheck).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("can't carry local workflow evidence"),
                "{error:#}"
            );
        }
    }

    fn review_fixture() -> Result<(Binding, Assessment, SubmittedReview)> {
        let binding = Binding {
            repository: "fixture/repository".into(),
            base: "a".repeat(40),
            head: "b".repeat(40),
            diff_sha256: "c".repeat(64),
        };
        let assessment = serde_json::from_str(&input("medium"))?;
        let body = format!(
            "**Actionable comments posted: 0**\n- **Run ID**: `00000000-0000-0000-0000-000000000001`\nReviewing files that changed from the base of the PR and between {} and {}.\n<summary>📒 Files selected for processing (1)</summary>\n* `README.md`\n</details>\n<!-- This is an auto-generated comment by CodeRabbit for review status -->",
            binding.base, binding.head
        );
        let review = SubmittedReview {
            id: 1,
            user: Bot {
                login: REVIEW_BOT.into(),
            },
            commit_id: binding.head.clone(),
            state: "COMMENTED".into(),
            submitted_at: Some("2026-10-14T00:00:00Z".into()),
            body,
        };
        Ok((binding, assessment, review))
    }

    fn status(id: u64, state: &str, description: &str) -> CommitStatus {
        CommitStatus {
            id,
            creator: Bot {
                login: REVIEW_BOT.into(),
            },
            context: REVIEW_CONTEXT.into(),
            state: state.into(),
            description: description.into(),
        }
    }

    #[test]
    fn critical_review_requires_full_range_and_severity_path_coverage() -> Result<()> {
        let (binding, assessment, mut review) = review_fixture()?;
        validate_submitted_review(&review, &binding, &assessment)?;
        let original = review.body.clone();
        for body in [
            original.replace(&binding.base, &"d".repeat(40)),
            original.replace(&binding.head, &"e".repeat(40)),
            original.replace("README.md", "other.md"),
            original.replace("* `README.md`", "* `README.md`\n* `README.md`"),
            format!("Review skipped due to rate limit.\n{original}"),
            "Pre-merge checks failed. Please resolve the failing checks before merging.".into(),
        ] {
            review.body = body;
            assert!(validate_submitted_review(&review, &binding, &assessment).is_err());
        }
        review.body = original;
        review.state = "DISMISSED".into();
        assert!(validate_submitted_review(&review, &binding, &assessment).is_err());
        Ok(())
    }

    #[test]
    fn finding_prose_is_not_a_provider_terminal_state() -> Result<()> {
        let (binding, assessment, mut review) = review_fixture()?;
        review.body = review.body.replace("- **Run ID**:", "Finding: Add rate limits to this endpoint.\nFinding: Failed validation permits partial review output.\n```text\nReview skipped due to rate limit.\n```\n- **Run ID**:");
        validate_pr_evidence(
            &[review],
            &[status(1, "success", "Review completed")],
            &binding,
            &assessment,
            1,
        )?;
        Ok(())
    }

    #[test]
    fn critical_review_uses_latest_status_and_latest_submitted_review() -> Result<()> {
        let (binding, assessment, review) = review_fixture()?;
        let mut reviews = vec![review];
        validate_pr_evidence(
            &reviews,
            &[status(1, "success", "Review completed")],
            &binding,
            &assessment,
            1,
        )?;
        for latest in [
            status(2, "pending", "Review in progress"),
            status(2, "success", "Review skipped"),
            status(2, "success", "Rate limit exceeded"),
        ] {
            assert!(
                validate_pr_evidence(
                    &reviews,
                    &[status(1, "success", "Review completed"), latest],
                    &binding,
                    &assessment,
                    1,
                )
                .is_err()
            );
        }
        let (_, _, mut latest) = review_fixture()?;
        latest.id = 2;
        latest.commit_id = "d".repeat(40);
        reviews.push(latest);
        assert!(
            validate_pr_evidence(
                &reviews,
                &[status(1, "success", "Review completed")],
                &binding,
                &assessment,
                1,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn missing_pr_review_evidence_is_a_scoped_structured_refusal() -> Result<()> {
        let (binding, assessment, review) = review_fixture()?;
        let reviews = [review];
        let statuses = [status(1, "success", "Review completed")];
        for (reviews, statuses) in [(&[][..], &statuses[..]), (&reviews[..], &[][..])] {
            let error =
                validate_pr_evidence(reviews, statuses, &binding, &assessment, 17).unwrap_err();
            let refusal = error
                .downcast_ref::<platform::refusal::Refusal>()
                .context("missing review needs its domain refusal")?;
            assert_eq!(refusal.rule, "pr.review");
            assert!(refusal.is_complete());
            assert!(refusal.evidence[0].contains("fixture/repository#17"));
            assert!(refusal.evidence[0].contains(&binding.head));
            assert!(refusal.evidence[0].contains(&binding.diff_sha256));
        }
        let error = validate_pr_evidence(
            &reviews,
            &[status(2, "success", "Review skipped")],
            &binding,
            &assessment,
            17,
        )
        .unwrap_err();
        assert!(error.downcast_ref::<platform::refusal::Refusal>().is_none());
        Ok(())
    }

    #[test]
    fn unresolved_comments_keep_bot_findings_and_refuse_incomplete_authors() {
        for login in [GRAPHQL_BOT, REVIEW_BOT] {
            assert!(
                bot_comment(&[ReviewComment {
                    author: Some(Bot {
                        login: login.into()
                    }),
                    body: "Finding".into()
                }])
                .unwrap()
            );
        }
        assert!(
            bot_comment(&[ReviewComment {
                author: None,
                body: "Finding".into()
            }])
            .is_err()
        );
        assert!(
            bot_comment(&[ReviewComment {
                author: Some(Bot {
                    login: GRAPHQL_BOT.into()
                }),
                body: String::new()
            }])
            .is_err()
        );
    }

    #[test]
    fn review_pagination_and_graphql_fail_closed() {
        let mut seen = BTreeSet::new();
        let next = PageInfo {
            has_next_page: true,
            end_cursor: Some("next".into()),
        };
        assert_eq!(next_cursor(&next, &mut seen).unwrap(), Some("next".into()));
        assert!(next_cursor(&next, &mut seen).is_err());
        assert!(
            next_cursor(
                &PageInfo {
                    has_next_page: true,
                    end_cursor: None
                },
                &mut seen
            )
            .is_err()
        );
        assert!(
            graph_data(Graphql {
                data: (),
                errors: Some(Vec::new())
            })
            .is_err()
        );
    }

    fn fixture_git(root: &Path, arguments: &[&str]) -> Result<String> {
        let output = git_fixture::command("git", &root.join("fixture.gitconfig"))
            .current_dir(root)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "fixture Git {arguments:?} exited {}: stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    #[test]
    fn unsigned_replace_refs_cannot_rebind_an_assessment_source() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path();
        fs::write(
            root.join("fixture.gitconfig"),
            "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n",
        )?;
        fixture_git(root, &["init", "--quiet"])?;
        fixture_git(
            root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/fixture/repository.git",
            ],
        )?;
        fs::write(root.join("README.md"), "Base\n")?;
        fixture_git(root, &["add", "README.md"])?;
        fixture_git(root, &["commit", "--quiet", "-m", "base"])?;
        let base = fixture_git(root, &["rev-parse", "HEAD"])?;
        fs::write(root.join("README.md"), "Actual reviewed change\n")?;
        fixture_git(root, &["add", "README.md"])?;
        fixture_git(root, &["commit", "--quiet", "-m", "head"])?;
        let head = fixture_git(root, &["rev-parse", "HEAD"])?;
        let expected = capture_at(root, "fixture/repository", &base, &head)?;
        fs::write(root.join("README.md"), "Unsigned replacement\n")?;
        fixture_git(root, &["add", "README.md"])?;
        let tree = fixture_git(root, &["write-tree"])?;
        let replacement = fixture_git(
            root,
            &["commit-tree", &tree, "-p", &base, "-m", "replacement"],
        )?;
        fixture_git(root, &["replace", &head, &replacement])?;
        let observed = capture_at(root, "fixture/repository", &base, &head)?;
        assert_eq!(observed.binding, expected.binding);
        assert_eq!(observed.tree, expected.tree);
        assert_eq!(observed.changed, expected.changed);
        Ok(())
    }
}
