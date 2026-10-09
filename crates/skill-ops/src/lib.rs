//! The skill operations engine: the catalog, the maintenance reports and decisions, and the installation of the shared skills.

pub mod skill_install;
mod tree;

pub use platform::paths::canonical;
pub use platform::{aside, build_inputs, freshness, refusal, report, tool};
pub use rules::{
    incident_rules, proof_budget_rules, skill_gate_rules, skill_plan_rules, skill_rules,
};
pub(crate) use tree::collect;
pub use tree::{
    Metadata, export_checks, metadata, skill_source_paths, skills, split_frontmatter,
    validate_links, validate_tree,
};

use crate::skill_rules::{
    DiffBase, Maintenance, Sizing, Tracked, adopted, composition_candidate, diff_base,
    gate_complete, maintenance_current, size_disposition, tracked_decision, triage_complete,
};
pub use crate::skill_rules::{Outcome, decision_complete};
use anyhow::{Context, Result, bail, ensure};
use pulldown_cmark::{Event as MarkdownEvent, Parser};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub skill: String,
    #[serde(alias = "also")]
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub id: String,
    pub kind: String,
    pub skills: Vec<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub id: String,
    #[serde(with = "outcome_key")]
    pub outcome: Outcome,
    pub reason: String,
    pub evidence: Vec<String>,
    pub revisit: Option<String>,
}

/// Stores an outcome as the name its rule module gives it.
mod outcome_key {
    use super::Outcome;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde passes every field to a serializer by reference"
    )]
    pub fn serialize<S: Serializer>(outcome: &Outcome, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(outcome.key())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Outcome, D::Error> {
        let key = String::deserialize(deserializer)?;
        Outcome::from_key(&key).ok_or_else(|| {
            D::Error::custom(format!(
                "unknown outcome `{key}`, expected one of {:?}",
                Outcome::ALL.map(Outcome::key)
            ))
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub report_hash: String,
    pub decisions: Vec<Decision>,
}

/// The tracked judgment for one skill, bound to the content revision it assessed.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SkillDecision {
    pub revision: String,
    #[serde(with = "outcome_key")]
    pub outcome: Outcome,
    pub reason: String,
    pub evidence: Vec<String>,
    pub revisit: Option<String>,
    /// Required exactly while the entrypoint exceeds the policy's word limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<SizeDecision>,
}

/// The judgment on an entrypoint over the word limit, sharing the decision's revision and evidence.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SizeDecision {
    #[serde(with = "outcome_key")]
    pub outcome: Outcome,
    pub reason: String,
    pub revisit: Option<String>,
}

/// A skill revision whose tracked decision a host adopts, and whether it also settles the size finding.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reviewed {
    pub revision: String,
    pub size: bool,
}

pub const DECISIONS: &str = "docs/skills/decisions";

/// Where the canonical skills live now, then where commits before the source state moved under `home/` hold them.
const SKILL_TREES: [&str; 2] = ["home/dot_agents/skills", "dot_agents/skills"];

/// Orders the requested skills after every skill they depend on.
///
/// # Errors
///
/// Returns an error when a requested or required skill names no catalog skill, a skill declares or lists a dependency twice, or the dependencies form a cycle.
pub fn dependency_plan(
    available: &BTreeSet<String>,
    requested: &[String],
    requirements: &[Requirement],
) -> Result<Vec<String>> {
    let mut graph = BTreeMap::new();
    for requirement in requirements {
        ensure!(
            available.contains(&requirement.skill),
            "unknown required skill: {}",
            requirement.skill
        );
        ensure!(
            graph
                .insert(&requirement.skill, &requirement.dependencies)
                .is_none(),
            "duplicate dependency declaration"
        );
        let mut unique = BTreeSet::new();
        for name in &requirement.dependencies {
            ensure!(
                available.contains(name) && unique.insert(name),
                "unknown or duplicate dependency: {name}"
            );
        }
    }
    // `skill_plan_rules` names each skill by its index in the sorted catalog.
    let names: Vec<&String> = available.iter().collect();
    let index = |name: &str| names.iter().position(|candidate| *candidate == name);
    let deps: Vec<Vec<usize>> = names
        .iter()
        .map(|name| {
            graph
                .get(name)
                .into_iter()
                .flat_map(|also| also.iter())
                .filter_map(|dependency| index(dependency))
                .collect()
        })
        .collect();
    let deps: Vec<&[usize]> = deps.iter().map(Vec::as_slice).collect();
    let mut wanted = Vec::new();
    for name in requested {
        wanted.push(index(name).with_context(|| format!("unknown skill: {name}"))?);
    }
    match skill_plan_rules::input(&deps, &wanted).and_then(skill_plan_rules::plan) {
        Ok(plan) => Ok(plan
            .skills()
            .iter()
            .copied()
            .filter_map(|skill| names.get(skill).map(|name| (*name).clone()))
            .collect()),
        Err(skill_plan_rules::Failure::Cycle) => bail!("cyclic skill dependency"),
        Err(skill_plan_rules::Failure::Unknown) => bail!("unknown skill dependency"),
    }
}

#[must_use]
pub fn complete(
    outcome: Outcome,
    reason: &str,
    evidence: &[String],
    revisit: Option<&String>,
) -> bool {
    decision_complete(
        outcome,
        !reason.trim().is_empty(),
        !evidence.is_empty(),
        revisit.is_some_and(|value| !value.trim().is_empty()),
    )
}

fn check_evidence(evidence: &[String], root: &Path) -> Result<()> {
    for evidence in evidence {
        let path = Path::new(evidence);
        ensure!(
            !evidence.is_empty()
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "evidence must be a relative local path: {evidence}"
        );
        let resolved = crate::canonical(&root.join(path))
            .with_context(|| format!("missing decision evidence: {evidence}"))?;
        ensure!(
            resolved.starts_with(root) && resolved.is_file(),
            "decision evidence escapes its root or is not a file: {evidence}"
        );
    }
    Ok(())
}

/// Verifies a runtime disposition.
///
/// # Errors
///
/// Returns an error when the review covers another report, a finding or decision lacks a match or repeats, or a decision lacks a field or cites invalid evidence.
/// It also returns an error when a finding has neither a decision nor an adoption.
pub fn check_review(
    report_hash: &str,
    findings: &[Finding],
    adopted: &BTreeSet<String>,
    review: &Review,
    evidence_root: &Path,
) -> Result<()> {
    // `skill_gate_rules::review_complete` names each finding by its index and each decision by the index of its finding.
    let index = |id: &str| {
        findings
            .iter()
            .position(|finding| finding.id == id)
            .unwrap_or(findings.len())
    };
    let facts: Vec<(usize, bool)> = findings
        .iter()
        .map(|finding| (index(&finding.id), adopted.contains(&finding.id)))
        .collect();
    let decisions: Vec<(usize, bool)> = review
        .decisions
        .iter()
        .map(|decision| {
            (
                index(&decision.id),
                complete(
                    decision.outcome,
                    &decision.reason,
                    &decision.evidence,
                    decision.revisit.as_ref(),
                ),
            )
        })
        .collect();
    match skill_gate_rules::review_complete(report_hash == review.report_hash, &facts, &decisions) {
        Ok(()) => {}
        Err(skill_gate_rules::Gap::Stale) => {
            bail!("stale skill analysis; analyze the current catalog and evidence")
        }
        Err(skill_gate_rules::Gap::RepeatedFinding) => bail!("duplicate analysis finding"),
        Err(skill_gate_rules::Gap::UnknownDecision) => {
            bail!("unknown or duplicate skill decision")
        }
        Err(skill_gate_rules::Gap::IncompleteDecision) => bail!("incomplete decision"),
        Err(skill_gate_rules::Gap::Undecided) => {
            bail!("unprocessed skill recommendations; record every disposition with evidence")
        }
    }
    let root = crate::canonical(evidence_root)?;
    for decision in &review.decisions {
        check_evidence(&decision.evidence, &root)?;
    }
    Ok(())
}

/// Holds the store lock of `state` until the returned file drops, so an analysis and a triage never interleave.
///
/// # Errors
///
/// Returns an error when opening or locking the lock file fails.
pub fn lock_store(state: &Path) -> Result<fs::File> {
    fs::create_dir_all(state)?;
    let file = fs::File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(state.join("store.lock"))
        .context("open the skill-ops store lock")?;
    file.lock().context("lock the skill-ops store")?;
    Ok(file)
}

/// Reads the tracked decisions, one `<skill>.json` file per skill.
///
/// # Errors
///
/// Returns an error when reading the directory fails, an entry doesn't match `<skill>.json`, or a decision fails validation.
pub fn tracked_decisions(directory: &Path) -> Result<BTreeMap<String, SkillDecision>> {
    let mut result = BTreeMap::new();
    if !directory.try_exists()? {
        return Ok(result);
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".json"))
            .filter(|name| !name.is_empty() && entry.file_type().is_ok_and(|kind| kind.is_file()))
            .with_context(|| {
                format!(
                    "{} is not a skill decision; keep only <skill>.json files in {DECISIONS}",
                    path.display()
                )
            })?;
        result.insert(name.to_owned(), read_json(&path)?);
    }
    Ok(result)
}

/// Requires every catalog skill to have a complete decision bound to its current revision, with a size disposition exactly while its entrypoint exceeds the word limit.
///
/// # Errors
///
/// Returns an error when resolving `evidence_root` fails.
/// It also lists every refusal when a skill lacks a decision or a decision names no skill.
/// The list also covers a decision that has a stale revision, lacks a field, cites invalid evidence, or has a wrong size disposition.
pub fn check_decisions(
    catalog: &Catalog,
    policy: &Policy,
    decisions: &BTreeMap<String, SkillDecision>,
    evidence_root: &Path,
) -> Result<()> {
    let root = crate::canonical(evidence_root)?;
    let mut refusals = Vec::new();
    for name in decisions.keys() {
        if !catalog.skills.contains_key(name) {
            refusals.push(format!(
                "{DECISIONS}/{name}.json names no skill in the catalog; delete it or restore the skill"
            ));
        }
    }
    let limit = policy.max_entry_words;
    for (name, skill) in &catalog.skills {
        let decision = decisions.get(name);
        let state = tracked_decision(
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
        );
        let decision = match (state, decision) {
            (Tracked::Current, Some(decision)) => decision,
            (Tracked::Missing, _) | (Tracked::Current, None) => {
                refusals.push(format!(
                    "{name} has no decision; run `just decide {name}` and write its reason"
                ));
                continue;
            }
            (Tracked::Stale, _) => {
                refusals.push(format!(
                    "{name} changed since its decision; run `just decide {name}` and write the reason for this revision"
                ));
                continue;
            }
            (Tracked::Incomplete, _) => {
                refusals.push(format!(
                    "the decision for {name} is incomplete; write a reason, evidence, and a revisit condition for a deferral in {DECISIONS}/{name}.json"
                ));
                continue;
            }
        };
        if let Err(error) = check_evidence(&decision.evidence, &root) {
            refusals.push(format!(
                "{name}: {error:#}; fix the evidence in {DECISIONS}/{name}.json"
            ));
        }
        let words = skill.entry_words;
        match size_disposition(
            words > limit,
            decision.size.is_some(),
            decision.size.as_ref().is_some_and(|size| {
                complete(
                    size.outcome,
                    &size.reason,
                    &decision.evidence,
                    size.revisit.as_ref(),
                )
            }),
        ) {
            Sizing::Within | Sizing::Settled => {}
            Sizing::Missing => refusals.push(format!(
                "{name} has {words} entrypoint words, over the limit of {limit}; run `just decide {name}` and write its size disposition"
            )),
            Sizing::Incomplete => refusals.push(format!(
                "the size disposition for {name} is incomplete; write its reason, and a revisit condition for a deferral, in {DECISIONS}/{name}.json"
            )),
            Sizing::Unexpected => refusals.push(format!(
                "{name} is within the limit of {limit} entrypoint words; run `just decide {name}` to remove its size disposition"
            )),
        }
    }
    ensure!(
        refusals.is_empty(),
        "skill decisions are not current:\n{}",
        refusals.join("\n")
    );
    Ok(())
}

fn size_draft(previous: Option<&SizeDecision>) -> SizeDecision {
    SizeDecision {
        outcome: previous.map_or(Outcome::Keep, |size| size.outcome),
        reason: String::new(),
        revisit: previous.and_then(|size| size.revisit.clone()),
    }
}

/// Drafts the decision for a changed or new skill, or the size disposition that its bound decision lacks or no longer needs.
///
/// # Errors
///
/// Returns an error when the previous decision already binds `revision` and its size disposition needs no change.
pub fn draft_decision(
    previous: Option<&SkillDecision>,
    revision: &str,
    over_limit: bool,
    skill: &str,
) -> Result<SkillDecision> {
    Ok(match previous {
        Some(previous) if previous.revision == revision => {
            match size_disposition(over_limit, previous.size.is_some(), true) {
                Sizing::Missing => SkillDecision {
                    size: Some(size_draft(None)),
                    ..previous.clone()
                },
                Sizing::Unexpected => SkillDecision {
                    size: None,
                    ..previous.clone()
                },
                Sizing::Within | Sizing::Settled | Sizing::Incomplete => anyhow::bail!(
                    "the decision for {skill} is already bound to its current revision; edit {DECISIONS}/{skill}.json directly"
                ),
            }
        }
        Some(previous) => SkillDecision {
            revision: revision.into(),
            reason: String::new(),
            size: over_limit.then(|| size_draft(previous.size.as_ref())),
            ..previous.clone()
        },
        None => SkillDecision {
            revision: revision.into(),
            outcome: Outcome::Keep,
            reason: String::new(),
            evidence: vec![format!("{}/{skill}/SKILL.md", SKILL_TREES[0])],
            revisit: None,
            size: over_limit.then(|| size_draft(None)),
        },
    })
}

fn git_bytes(root: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = crate::tool::Tool::Git
        .command()
        .current_dir(root)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git_bytes(root, arguments)?)?
        .trim()
        .to_owned())
}

/// Hashes a skill tree at a commit as the catalog hashes it, or returns None when that tree lies outside what the catalog can hold.
fn revision_at(root: &Path, commit: &str, skill: &str) -> Result<Option<String>> {
    let mut found = None;
    for skills in SKILL_TREES {
        let listing = String::from_utf8(git_bytes(
            root,
            &[
                "ls-tree",
                "-r",
                "-z",
                commit,
                "--",
                &format!("{skills}/{skill}"),
            ],
        )?)?;
        if !listing.is_empty() {
            found = Some((skills, listing));
            break;
        }
    }
    let Some((skills, listing)) = found else {
        return Ok(None);
    };
    let mut files = Vec::new();
    for entry in listing.split('\0').filter(|entry| !entry.is_empty()) {
        let (header, path) = entry
            .split_once('\t')
            .context("malformed git ls-tree entry")?;
        let mut fields = header.split(' ');
        let (Some(mode), Some(object)) = (fields.next(), fields.nth(1)) else {
            anyhow::bail!("malformed git ls-tree entry");
        };
        let relative = path
            .strip_prefix(skills)
            .and_then(|rest| rest.strip_prefix('/'))
            .context("git ls-tree listed a path outside the skill")?;
        if relative.split('/').any(|part| part == "target") {
            continue;
        }
        if !matches!(mode, "100644" | "100755") {
            return Ok(None);
        }
        files.push((relative.to_owned(), object.to_owned()));
    }
    if files.is_empty() {
        return Ok(None);
    }
    files.sort_by(|(left, _), (right, _)| left.split('/').cmp(right.split('/')));
    let mut snapshots = Vec::new();
    for (relative, object) in files {
        snapshots.push((
            relative,
            digest(&git_bytes(root, &["cat-file", "blob", &object])?),
        ));
    }
    Ok(Some(hash(&snapshots)?))
}

/// Finds the newest commit whose skill tree holds the assessed revision.
fn assessed_commit(root: &Path, skill: &str, revision: &str) -> Result<Option<String>> {
    let history = git_output(
        root,
        &[
            "log",
            "--format=%H",
            "--",
            &format!("{}/{skill}", SKILL_TREES[0]),
            &format!("{}/{skill}", SKILL_TREES[1]),
        ],
    )?;
    for commit in history.lines() {
        if revision_at(root, commit, skill)?.as_deref() == Some(revision) {
            return Ok(Some(commit.to_owned()));
        }
    }
    Ok(None)
}

/// Writes the draft decision for one skill and returns the context its writer needs.
///
/// # Errors
///
/// Returns an error when capturing the catalog fails, the catalog lacks `skill`, or reading the previous decision or policy fails.
/// It also returns an error when the decision already binds the current revision.
/// It also returns an error when analysis, `git`, or writing the draft fails.
pub fn decide(root: &Path, skill: &str) -> Result<String> {
    let catalog = catalog(&root.join(SKILL_TREES[0]))?;
    let current = catalog.skills.get(skill).with_context(|| {
        format!(
            "{skill} is not a skill; pass a directory name under {}",
            SKILL_TREES[0]
        )
    })?;
    let relative = format!("{DECISIONS}/{skill}.json");
    let path = root.join(&relative);
    let previous: Option<SkillDecision> = if path.try_exists()? {
        Some(read_json(&path)?)
    } else {
        None
    };
    let policy: Policy = read_json(&root.join("home/dot_config/skill-ops/policy.json"))?;
    let draft = draft_decision(
        previous.as_ref(),
        &current.revision,
        current.entry_words > policy.max_entry_words,
        skill,
    )?;
    let mut context: Vec<String> = analyze(&catalog, &policy, &[], &[])?
        .findings
        .iter()
        .filter(|finding| finding.skills == [skill])
        .map(|finding| format!("Finding {}: {}", finding.kind, finding.detail))
        .collect();
    let tree = format!("{}/{skill}", SKILL_TREES[0]);
    if let Some(previous) = &previous {
        context.push(format!(
            "Previous decision ({}): {}",
            serde_json::Value::from(previous.outcome.key()),
            previous.reason
        ));
    }
    let assessed = match &previous {
        Some(previous) => assessed_commit(root, skill, &previous.revision)?,
        None => None,
    };
    match (diff_base(previous.is_some(), assessed.is_some()), assessed) {
        (DiffBase::Assessed, Some(commit)) => {
            context.push(format!(
                "Changes to {tree} since {commit}, the latest commit holding the content the previous decision assessed:"
            ));
            context.push(git_output(
                root,
                &["--no-pager", "diff", &commit, "--", &tree],
            )?);
        }
        (DiffBase::Unavailable | DiffBase::Assessed, _) => context.push(format!(
            "No commit holds the content the previous decision assessed; assess the whole of {tree} against that decision"
        )),
        (DiffBase::Whole, _) => context.push(format!(
            "{skill} has no previous decision; assess the whole skill"
        )),
    }
    write_json(&path, &draft, true)?;
    context.push(format!(
        "Drafted {relative}; write each empty reason for this revision, adjust the outcome, evidence, and revisit condition, then run just check"
    ));
    Ok(context.join("\n"))
}

/// Selects the skill revisions whose tracked decisions a host adopts without local triage.
#[must_use]
pub fn adoptable_revisions(
    catalog: &Catalog,
    decisions: &BTreeMap<String, SkillDecision>,
) -> BTreeMap<String, Reviewed> {
    catalog
        .skills
        .iter()
        .filter_map(|(name, skill)| {
            let decision = decisions.get(name)?;
            adopted(decision.revision == skill.revision, decision.outcome).then(|| {
                (
                    name.clone(),
                    Reviewed {
                        revision: skill.revision.clone(),
                        size: decision
                            .size
                            .as_ref()
                            .is_some_and(|size| adopted(true, size.outcome)),
                    },
                )
            })
        })
        .collect()
}

/// Identifies the revision-bound findings that a reviewed revision already settles.
#[must_use]
pub fn adopted_findings(
    report: &Report,
    reviewed: &BTreeMap<String, Reviewed>,
) -> BTreeSet<String> {
    report
        .findings
        .iter()
        .filter(|finding| {
            let [skill] = finding.skills.as_slice() else {
                return false;
            };
            let Some(reviewed) = reviewed.get(skill) else {
                return false;
            };
            let revision = &reviewed.revision;
            (finding.kind == "catalog-review"
                && finding.id == format!("catalog:{skill}:{revision}"))
                || (reviewed.size
                    && finding.kind == "progressive-disclosure"
                    && finding.id == format!("size:{skill}:{revision}"))
        })
        .map(|finding| finding.id.clone())
        .collect()
}

/// Hashes the compact JSON serialization of `value` with SHA-256, as lowercase hex.
///
/// # Errors
///
/// Returns an error when serializing `value` to JSON fails.
pub fn hash(value: &impl Serialize) -> Result<String> {
    Ok(digest(&serde_json::to_vec(value)?))
}

/// Hashes the content of every evidence file that the review's decisions cite.
///
/// # Errors
///
/// Returns an error when resolving or reading `root` or an evidence file fails, or an evidence path stays empty, lacks a relative form, escapes `root`, or names something other than a file.
pub fn evidence_hashes(review: &Review, root: &Path) -> Result<BTreeMap<String, String>> {
    let root = crate::canonical(root)?;
    let mut result = BTreeMap::new();
    for name in review
        .decisions
        .iter()
        .flat_map(|decision| &decision.evidence)
    {
        let path = Path::new(name);
        ensure!(
            !name.is_empty()
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "evidence must be a relative local path"
        );
        let resolved = crate::canonical(&root.join(path))?;
        ensure!(
            resolved.starts_with(&root) && resolved.is_file(),
            "evidence escapes its root"
        );
        result.insert(name.clone(), digest(&fs::read(resolved)?));
    }
    Ok(result)
}

/// Writes an immutable JSON record, accepting an existing file only when it holds the same value.
///
/// # Errors
///
/// Returns an error when writing the file fails, or the file already exists and either can't open for reading or holds a different value.
pub fn preserve_json(path: &Path, value: &impl Serialize) -> Result<()> {
    match write_json(path, value, false) {
        Ok(()) => Ok(()),
        Err(error)
            if error
                .downcast_ref::<tempfile::PersistError>()
                .is_some_and(|error| error.error.kind() == std::io::ErrorKind::AlreadyExists) =>
        {
            ensure!(
                read_json::<serde_json::Value>(path)? == serde_json::to_value(value)?,
                "conflicting immutable history record"
            );
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// Hashes the sources compiled into this engine, so a report or manifest from another engine reads as stale.
///
/// # Errors
///
/// Returns an error when serializing the sources fails.
pub fn engine_hash() -> Result<String> {
    hash(&(
        include_str!("lib.rs"),
        include_str!("../../rules/src/skill_rules.rs"),
        include_str!("skill_install.rs"),
        include_str!("tree.rs"),
        include_str!("main.rs"),
        include_str!("../../../xtask/src/skill_proofs.rs"),
        include_str!("../../../policy/obligations.toml"),
        include_str!("../../../xtask/proofs/Dockerfile"),
        include_str!("../../../home/dot_config/opencode/plugins/skill-ops.ts"),
        include_str!("../Cargo.toml"),
        include_str!("../../../Cargo.toml"),
        include_str!("../../../Cargo.lock"),
        include_str!("../../../package.json"),
        include_str!("../../../bun.lock"),
        include_str!("../../../tsconfig.json"),
        (
            include_str!("../../../mise.toml"),
            include_str!("../../../home/.chezmoiignore"),
            include_str!("../../../lefthook.yml"),
            include_str!("../../../.github/workflows/ci.yml"),
            include_str!("../../rules/src/pr_rules.rs"),
            include_str!("../../pr-workflow/src/lib.rs"),
            include_str!("../../pr-workflow/src/main.rs"),
            include_str!("../../coderabbit-guard/src/lib.rs"),
            include_str!("../../rules/src/review_rules.rs"),
        ),
    ))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Approved {
    pub catalog: Catalog,
    pub catalog_hash: String,
    pub policy_hash: String,
    pub engine_hash: String,
    pub reviewed: BTreeMap<String, Reviewed>,
}

/// Reads the installation manifest, which `just install-skill-ops` writes together with the engine that reads it.
///
/// # Errors
///
/// Returns an error when the manifest doesn't exist, reading it fails, or it doesn't match this engine's format.
pub fn read_approved(path: &Path) -> Result<Approved> {
    read_json(path).with_context(|| {
        format!(
            "the skill-ops installation manifest {} is missing or was written by another engine; run `just install-skill-ops` in the dotfiles checkout",
            path.display()
        )
    })
}

fn digest(bytes: &[u8]) -> String {
    crate::build_inputs::hex(&Sha256::digest(bytes))
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: u32,
    pub review_every: usize,
    pub minimum_pair_sessions: usize,
    pub pair_percent: usize,
    pub max_entry_words: usize,
    pub requirements: Vec<Requirement>,
    pub bundles: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Skill {
    pub revision: String,
    pub description: String,
    pub entry_words: usize,
    pub files: Vec<String>,
    pub references: BTreeSet<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub skills: BTreeMap<String, Skill>,
}

/// Captures every skill directory in `tree` with its revision, description, entrypoint size, files, and references.
///
/// # Errors
///
/// Returns an error when a skill name holds non-UTF-8 text or names something other than a directory, or reading or validating an entrypoint or resource fails.
/// It also returns an error when a skill changes during capture or the catalog holds no skill.
pub fn catalog(tree: &Path) -> Result<Catalog> {
    capture_catalog(tree, None)
}

/// Captures only the skills and files that `scope` names in `tree`.
///
/// # Errors
///
/// Returns an error when a managed skill or resource doesn't exist, fails reading or validation, or escapes its skill, or a skill changes during capture.
pub fn catalog_owned(tree: &Path, scope: &Catalog) -> Result<Catalog> {
    capture_catalog(tree, Some(scope))
}

fn capture_catalog(tree: &Path, scope: Option<&Catalog>) -> Result<Catalog> {
    let tree = crate::canonical(tree)?;
    let mut skills = BTreeMap::new();
    for entry in fs::read_dir(&tree)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-UTF-8 skill name"))?;
        if scope.is_some_and(|scope| !scope.skills.contains_key(&name)) {
            continue;
        }
        ensure!(
            entry.file_type()?.is_dir(),
            "skill catalog requires directories"
        );
        let entrypoint = entry.path().join("SKILL.md");
        let text = fs::read_to_string(&entrypoint)?;
        let metadata = crate::metadata(&text, &name)?;
        let mut files = Vec::new();
        if let Some(scope) = scope {
            for relative in &scope.skills[&name].files {
                let path = Path::new(relative);
                ensure!(
                    path.starts_with(&name)
                        && path
                            .components()
                            .all(|component| matches!(component, Component::Normal(_))),
                    "managed resource escapes its skill"
                );
                crate::collect(&tree, path, &mut files)?;
            }
        } else {
            crate::collect(&tree, Path::new(&name), &mut files)?;
        }
        let mut snapshots = Vec::new();
        let mut references = BTreeSet::new();
        let mut names = Vec::new();
        for (relative, bytes) in &files {
            let path = relative
                .to_str()
                .context("non-UTF-8 skill resource")?
                .replace('\\', "/");
            names.push(path.clone());
            snapshots.push((path, digest(bytes)));
            if relative.extension().is_some_and(|value| value == "md") {
                let markdown = std::str::from_utf8(bytes)?;
                crate::validate_links(markdown, &tree.join(relative), &tree)?;
                for event in Parser::new(markdown) {
                    if let MarkdownEvent::Code(value) = event {
                        references.insert(value.to_string());
                    }
                }
            }
        }
        for (relative, bytes) in files {
            ensure!(
                fs::read(tree.join(relative))? == bytes,
                "skill changed during catalog capture"
            );
        }
        skills.insert(
            name,
            Skill {
                revision: hash(&snapshots)?,
                description: metadata.description,
                entry_words: crate::split_frontmatter(&text)?
                    .1
                    .split_whitespace()
                    .count(),
                files: names,
                references,
            },
        );
    }
    ensure!(!skills.is_empty(), "empty skill catalog");
    if let Some(scope) = scope {
        ensure!(
            skills.keys().eq(scope.skills.keys()),
            "a managed skill is missing"
        );
    }
    let available: BTreeSet<_> = skills.keys().cloned().collect();
    for skill in skills.values_mut() {
        skill.references.retain(|name| available.contains(name));
    }
    Ok(Catalog { skills })
}

/// Validates the policy's bounds, dependency declarations, and bundles with the catalog.
///
/// # Errors
///
/// Returns an error when the schema or a numeric bound fails validation, or a dependency names no catalog skill, repeats, or forms a cycle.
/// It also returns an error when a bundle holds no skill, repeats a skill, or names an unknown skill.
pub fn validate_policy(policy: &Policy, catalog: &Catalog) -> Result<()> {
    ensure!(
        policy.schema == 1
            && policy.review_every > 0
            && policy.minimum_pair_sessions >= skill_rules::MINIMUM_PAIR_FLOOR
            && (1..=skill_rules::PERCENT).contains(&policy.pair_percent)
            && policy.max_entry_words > 0,
        "invalid skill operations policy"
    );
    let available: BTreeSet<_> = catalog.skills.keys().cloned().collect();
    dependency_plan(
        &available,
        &available.iter().cloned().collect::<Vec<_>>(),
        &policy.requirements,
    )?;
    for (name, bundle) in &policy.bundles {
        ensure!(
            !name.trim().is_empty() && !bundle.is_empty(),
            "empty skill bundle"
        );
        let unique: BTreeSet<_> = bundle.iter().collect();
        ensure!(unique.len() == bundle.len(), "duplicate bundled skill");
        dependency_plan(&available, bundle, &policy.requirements)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Signal {
    Loader,
    FileRead,
    ShellReference,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub schema: u32,
    pub id: String,
    pub client: String,
    pub session: String,
    pub skill: String,
    pub revision: String,
    pub signal: Signal,
}

fn response_text(value: &serde_json::Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value["stdout"].as_str())
        .or_else(|| value["output"].as_str())
}

/// Extracts the skill uses that one successful `PostToolUse` hook event shows: a loader call, an entrypoint read, or a shell reference.
///
/// # Errors
///
/// Returns an error when the client names no known client, resolving the owned tree fails, or the loader output fails validation or names an unknown skill.
/// It also returns an error when an observed event lacks a session or call identity.
pub fn observations(
    client: &str,
    input: &serde_json::Value,
    catalog: &Catalog,
    owned_tree: &Path,
) -> Result<Vec<Observation>> {
    ensure!(
        matches!(client, "codex" | "claude" | "opencode"),
        "unknown observation client"
    );
    if input["hook_event_name"] != "PostToolUse"
        || input["tool_response"]["is_error"] == true
        || input["tool_response"]["exit_code"]
            .as_i64()
            .is_some_and(|code| code != 0)
    {
        return Ok(Vec::new());
    }
    let tool = input["tool_name"].as_str().unwrap_or_default();
    let args = &input["tool_input"];
    let mut found = BTreeMap::new();
    match tool {
        "Read" | "read" => {
            if let Some(path) = args["file_path"]
                .as_str()
                .or_else(|| args["filePath"].as_str())
                && let Ok(resolved) = crate::canonical(Path::new(path))
            {
                let root = crate::canonical(owned_tree)?;
                for name in catalog.skills.keys() {
                    if resolved == root.join(name).join("SKILL.md") {
                        found.insert(name.clone(), Signal::FileRead);
                    }
                }
            }
        }
        "Skill" | "skill" => {
            if let Some(name) = args["skill"].as_str().or_else(|| args["name"].as_str())
                && catalog.skills.contains_key(name)
            {
                found.insert(name.into(), Signal::Loader);
            }
        }
        "Bash" | "bash" | "exec_command" => {
            if let Some(response) = response_text(&input["tool_response"]) {
                for line in response.lines() {
                    if let Some(loaded) = line.strip_prefix("SKILL_OPS_LOAD ") {
                        let loaded: Vec<String> = serde_json::from_str(loaded)?;
                        for name in loaded {
                            ensure!(
                                catalog.skills.contains_key(&name),
                                "loader reported unknown skill"
                            );
                            found.insert(name, Signal::Loader);
                        }
                        break;
                    }
                }
            }
            if found.is_empty()
                && let Some(command) = args["command"].as_str().or_else(|| args["cmd"].as_str())
            {
                let command = command.replace('\\', "/");
                for name in catalog.skills.keys() {
                    if command.contains(&format!("skills/{name}/SKILL.md")) {
                        found.insert(name.clone(), Signal::ShellReference);
                    }
                }
            }
        }
        _ => {}
    }
    if found.is_empty() {
        return Ok(Vec::new());
    }
    let session = input["session_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .context("skill observation lacks session identity")?;
    let call = input["tool_use_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .context("skill observation lacks call identity")?;
    found
        .into_iter()
        .map(|(name, signal)| {
            Ok(Observation {
                schema: 1,
                id: hash(&(client, session, call, &name))?,
                client: client.into(),
                session: hash(&(client, session))?,
                revision: catalog.skills[&name].revision.clone(),
                skill: name,
                signal,
            })
        })
        .collect()
}

#[cfg_attr(
    not(unix),
    expect(
        clippy::unnecessary_wraps,
        reason = "the Unix form reports a directory that fails to sync"
    )
)]
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?
        .sync_all()
        .context("persist skill operations directory")?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Writes pretty JSON durably through a synced temporary file, replacing an existing file only when `replace` holds true.
///
/// # Errors
///
/// Returns an error when the path has no parent, or creating, writing, or syncing a directory or the file fails.
/// It also returns an error when the file exists and `replace` holds false.
pub fn write_json(path: &Path, value: &impl Serialize, replace: bool) -> Result<()> {
    let mut text = serde_json::to_vec_pretty(value)?;
    text.push(b'\n');
    write_file(path, &text, replace)
}

/// Writes `bytes` durably through a temporary file, creating missing directories and replacing an existing file only when `replace` holds.
///
/// # Errors
///
/// Returns an error when creating the directories or the file fails, or the file exists and `replace` holds false.
pub fn write_file(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    let absolute = std::path::absolute(path)?;
    let path = absolute.as_path();
    let directory = path.parent().context("missing output directory")?;
    let mut missing = Vec::new();
    let mut ancestor = directory;
    while !ancestor.try_exists()? {
        missing.push(ancestor);
        ancestor = ancestor
            .parent()
            .context("output directory has no existing ancestor")?;
    }
    fs::create_dir_all(directory)?;
    for created in missing.into_iter().rev() {
        sync_directory(created)?;
        sync_directory(
            created
                .parent()
                .context("missing created directory parent")?,
        )?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if replace {
        temporary.persist(path)?;
    } else {
        temporary.persist_noclobber(path)?;
    }
    sync_directory(directory)?;
    Ok(())
}

/// Stores an observation once under `events/`, returning whether the store lacked it before.
///
/// # Errors
///
/// Returns an error when the observation has an invalid schema or identity, a stored observation with the same identity differs, or writing the file fails.
pub fn record(state: &Path, observation: &Observation) -> Result<bool> {
    ensure!(
        observation.schema == 1
            && observation.id.len() == skill_rules::DIGEST_LENGTH
            && observation.id.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid skill observation identity"
    );
    let directory = state.join("events");
    let destination = directory.join(format!("{}.json", observation.id));
    if destination.try_exists()? {
        let existing: Observation = read_json(&destination)?;
        ensure!(
            existing == *observation,
            "conflicting replay of a skill observation"
        );
        return Ok(false);
    }
    match write_json(&destination, observation, false) {
        Ok(()) => {
            sync_directory(state)?;
            Ok(true)
        }
        Err(error)
            if error
                .downcast_ref::<tempfile::PersistError>()
                .is_some_and(|error| error.error.kind() == std::io::ErrorKind::AlreadyExists) =>
        {
            let existing: Observation = read_json(&destination)?;
            ensure!(
                existing == *observation,
                "conflicting concurrent skill observation: {error}"
            );
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// The largest JSON file the engine reads, 16 MiB.
const JSON_INPUT_LIMIT: u64 = 16 * 1024 * 1024;

/// Reads and deserializes a JSON file of at most 16 MiB.
///
/// # Errors
///
/// Returns an error when reading the file fails, the file exceeds 16 MiB, or it doesn't deserialize into `T`.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    ensure!(
        fs::metadata(path)?.len() <= JSON_INPUT_LIMIT,
        "skill operations input exceeds its size bound"
    );
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("invalid skill operations data: {}", path.display()))
}

/// Reads the recorded observations under `events/`, sorted by identity.
///
/// # Errors
///
/// Returns an error when reading the directory fails, a `.json` entry names something other than a regular file or fails validation, or a filename differs from its observation's identity.
pub fn recorded(state: &Path) -> Result<Vec<Observation>> {
    let directory = state.join("events");
    if !directory.try_exists()? {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|value| value == "json")
        {
            ensure!(
                entry.file_type()?.is_file(),
                "observation must be a regular file"
            );
            let observation: Observation = read_json(&entry.path())?;
            ensure!(
                observation.schema == 1
                    && entry.file_name().to_str() == Some(&format!("{}.json", observation.id)),
                "observation identity does not match its file"
            );
            result.push(observation);
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub kind: String,
    pub skills: Vec<String>,
    pub summary: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub loads: usize,
    pub file_reads: usize,
    pub shell_references: usize,
    pub sessions: BTreeSet<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema: u32,
    pub engine_hash: String,
    pub catalog_hash: String,
    pub policy_hash: String,
    pub event_ids: BTreeSet<String>,
    pub note_ids: BTreeSet<String>,
    pub sessions: usize,
    pub usage: BTreeMap<String, Usage>,
    pub findings: Vec<Finding>,
}

/// Builds the maintenance report: per-skill usage and the review, size, discovery, composition, dependency, and observation findings.
///
/// # Errors
///
/// Returns an error when the policy fails validation, an observation repeats or has an unknown schema, or a note fails validation, repeats, or names an unknown skill.
pub fn analyze(
    catalog: &Catalog,
    policy: &Policy,
    events: &[Observation],
    notes: &[Note],
) -> Result<Report> {
    validate_policy(policy, catalog)?;
    let mut report = Report {
        schema: 1,
        engine_hash: engine_hash()?,
        catalog_hash: hash(catalog)?,
        policy_hash: hash(policy)?,
        event_ids: BTreeSet::new(),
        note_ids: BTreeSet::new(),
        sessions: 0,
        usage: catalog
            .skills
            .keys()
            .map(|name| (name.clone(), Usage::default()))
            .collect(),
        findings: Vec::new(),
    };
    let mut sessions = BTreeSet::new();
    for event in events {
        ensure!(
            event.schema == 1 && report.event_ids.insert(event.id.clone()),
            "duplicate or invalid observation"
        );
        sessions.insert(event.session.clone());
        if let Some(usage) = report.usage.get_mut(&event.skill) {
            match event.signal {
                Signal::Loader => usage.loads += 1,
                Signal::FileRead => usage.file_reads += 1,
                Signal::ShellReference => usage.shell_references += 1,
            }
            usage.sessions.insert(event.session.clone());
        }
    }
    report.sessions = sessions.len();
    for (name, skill) in &catalog.skills {
        report.findings.push(Finding { id: format!("catalog:{name}:{}", skill.revision), kind: "catalog-review".into(), skills: vec![name.clone()], detail: "Assess trigger precision, composition, duplication, missing workflows, executable enforcement, realistic regression evidence, portability, and maintenance ownership for this revision.".into() });
        if skill.entry_words > policy.max_entry_words {
            report.findings.push(Finding { id: format!("size:{name}:{}", skill.revision), kind: "progressive-disclosure".into(), skills: vec![name.clone()], detail: format!("Entrypoint has {} words; inspect conditional material and scope before splitting.", skill.entry_words) });
        }
        if report.sessions >= policy.review_every && report.usage[name].sessions.is_empty() {
            report.findings.push(Finding { id: format!("discovery:{name}"), kind: "discovery-review".into(), skills: vec![name.clone()], detail: "No observed use in this sample; inspect eligibility and collection coverage before changing discovery or retiring the skill.".into() });
        }
    }
    let names: Vec<_> = report.usage.keys().cloned().collect();
    for (index, first) in names.iter().enumerate() {
        for second in &names[index + 1..] {
            let a = &report.usage[first].sessions;
            let b = &report.usage[second].sessions;
            let shared = a.intersection(b).count();
            if composition_candidate(
                shared,
                a.len(),
                b.len(),
                policy.minimum_pair_sessions,
                policy.pair_percent,
            ) {
                report.findings.push(Finding { id: format!("composition:{first}:{second}"), kind: "composition-review".into(), skills: vec![first.clone(), second.clone()], detail: format!("Observed together in {shared} sessions ({}/{}, {}/{}); inspect common tasks, dependency or bundle routing, and independent uses before proposing a merge.", shared, a.len(), shared, b.len()) });
            }
        }
    }
    for requirement in &policy.requirements {
        let primary = &report.usage[&requirement.skill].sessions;
        for dependency in &requirement.dependencies {
            let missing = primary
                .difference(&report.usage[dependency].sessions)
                .count();
            if missing > 0 {
                report.findings.push(Finding { id: format!("dependency:{}:{dependency}", requirement.skill), kind: "dependency-observation-gap".into(), skills: vec![requirement.skill.clone(), dependency.clone()], detail: format!("{missing} sessions lack an observed dependency load; inspect collection coverage and use the dependency-aware loader.") });
            }
        }
    }
    for note in notes {
        ensure!(
            matches!(
                note.kind.as_str(),
                "missing-skill" | "mechanize" | "failure" | "conflict" | "evaluation"
            ) && !note.summary.trim().is_empty()
                && !note.evidence.is_empty(),
            "invalid improvement observation"
        );
        ensure!(
            note.skills
                .iter()
                .all(|name| catalog.skills.contains_key(name)),
            "improvement observation names an unknown skill"
        );
        let id = format!("observation:{}", hash(note)?);
        ensure!(
            report.note_ids.insert(id.clone()),
            "duplicate improvement observation"
        );
        report.findings.push(Finding {
            id,
            kind: note.kind.clone(),
            skills: note.skills.clone(),
            detail: note.summary.clone(),
        });
    }
    report.findings.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(report)
}

/// The milliseconds in a second, which turn the proof budget into the words a skill would use.
const MILLIS_PER_SECOND: u64 = 1000;

/// The texts the checkout's gates pin, which a skill points at instead of repeating: each tool version of `mise.toml`, and the proof budget.
///
/// # Errors
///
/// Returns an error when `mise.toml` exists but doesn't parse.
pub fn pinned_terms(root: &Path) -> Result<Vec<String>> {
    let mut terms = Vec::new();
    let manifest = root.join("mise.toml");
    if manifest.is_file() {
        let value: toml::Table = toml::from_str(&fs::read_to_string(&manifest)?)
            .with_context(|| manifest.display().to_string())?;
        for spec in value
            .get("tools")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flat_map(toml::Table::values)
        {
            let version = match spec {
                toml::Value::String(version) => Some(version.as_str()),
                toml::Value::Table(table) => table.get("version").and_then(toml::Value::as_str),
                _ => None,
            };
            if let Some(version) = version.filter(|version| version.contains('.')) {
                terms.push(version.to_owned());
            }
        }
    }
    let seconds = proof_budget_rules::BUDGET_MILLIS / MILLIS_PER_SECOND;
    terms.push(format!("{seconds}-second"));
    terms.push(format!("{seconds} seconds"));
    terms.sort();
    terms.dedup();
    Ok(terms)
}

/// Refuses a skill document that names a pin of the checkout's gates, since it drifts once the pin moves.
///
/// # Errors
///
/// Returns an error naming each document and pin, or when reading the skill tree or `mise.toml` fails.
pub fn check_pins(root: &Path) -> Result<()> {
    let terms = pinned_terms(root)?;
    let mut files = Vec::new();
    collect(root, Path::new(SKILL_TREES[0]), &mut files)?;
    let mut found = Vec::new();
    for (path, bytes) in &files {
        if path.extension().is_none_or(|extension| extension != "md") {
            continue;
        }
        for term in &terms {
            if skill_rules::names(bytes, term.as_bytes()) {
                found.push(format!("{}: names `{term}`", path.display()));
            }
        }
    }
    ensure!(
        found.is_empty(),
        "a skill repeats what a gate pins, which drifts once the pin moves; point at the gate or the file that holds the pin instead:\n{}",
        found.join("\n")
    );
    Ok(())
}

/// Validates the checkout's skill tree, catalog, policy, and tracked decisions.
///
/// # Errors
///
/// Returns an error when the tree or catalog fails validation, reading or validating the policy fails, or a skill lacks a current and complete tracked decision.
pub fn check_catalog(root: &Path) -> Result<()> {
    crate::validate_tree(root)?;
    check_pins(root)?;
    let catalog = catalog(&root.join(SKILL_TREES[0]))?;
    let policy: Policy = read_json(&root.join("home/dot_config/skill-ops/policy.json"))?;
    validate_policy(&policy, &catalog)?;
    check_decisions(
        &catalog,
        &policy,
        &tracked_decisions(&root.join(DECISIONS))?,
        root,
    )
}

#[derive(Clone, Copy, Debug)]
pub struct Runtime<'a> {
    pub catalog: &'a Catalog,
    pub policy: &'a Policy,
    pub events: &'a [Observation],
    pub notes: &'a [Note],
    pub report: Option<&'a Report>,
    pub review: Option<&'a Review>,
    pub approved: &'a Approved,
    pub evidence_root: &'a Path,
}

/// Whether the runtime maintenance gate has open work: a stale analysis, unprocessed findings, or enough new observations for a review.
///
/// # Errors
///
/// Returns an error when the policy fails validation, events or notes that an earlier report counted disappeared, the recorded review doesn't verify, or a fresh analysis fails.
pub fn runtime_pending(runtime: Runtime<'_>) -> Result<bool> {
    let Runtime {
        catalog,
        policy,
        events,
        notes,
        report,
        review,
        approved,
        evidence_root: root,
    } = runtime;
    validate_policy(policy, catalog)?;
    let current: BTreeSet<_> = events.iter().map(|event| event.id.clone()).collect();
    let note_ids: BTreeSet<_> = notes
        .iter()
        .map(|note| hash(note).map(|digest| format!("observation:{digest}")))
        .collect::<Result<_>>()?;
    let recorded_current = if let Some(report) = report {
        ensure!(
            report.schema == 1 && report.event_ids.is_subset(&current),
            "previously observed events disappeared; inspect local state"
        );
        ensure!(
            report.note_ids.is_subset(&note_ids),
            "previous improvement observations disappeared"
        );
        let adopted = adopted_findings(report, &approved.reviewed);
        let review_verified = match review {
            Some(review) => {
                check_review(&hash(report)?, &report.findings, &adopted, review, root)?;
                true
            }
            None => false,
        };
        let unadopted = report.findings.len() - adopted.len();
        maintenance_current(Maintenance {
            catalog_current: report.catalog_hash == hash(catalog)?
                && report.engine_hash == engine_hash()?,
            policy_current: report.policy_hash == hash(policy)?,
            observations_preserved: report.event_ids.is_subset(&current),
            notes_current: report.note_ids == note_ids,
            review_complete: triage_complete(unadopted, review_verified),
            new_observations: current.difference(&report.event_ids).count(),
            threshold: policy.review_every,
        })
    } else {
        hash(catalog)? == approved.catalog_hash
            && hash(policy)? == approved.policy_hash
            && engine_hash()? == approved.engine_hash
            && current.len() < policy.review_every
            && notes.is_empty()
    };
    if recorded_current {
        return Ok(false);
    }
    let analysis = analyze(catalog, policy, events, notes)?;
    let open = analysis.findings.len() - adopted_findings(&analysis, &approved.reviewed).len();
    Ok(!gate_complete(recorded_current, open))
}

/// Reads the recorded improvement observations under `notes/`, sorted by kind, summary, skills, and evidence.
///
/// # Errors
///
/// Returns an error when reading the directory fails, a `.json` entry names something other than a regular file or fails validation, or a filename differs from the hash of its note.
pub fn notes(state: &Path) -> Result<Vec<Note>> {
    let directory = state.join("notes");
    if !directory.try_exists()? {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|value| value == "json")
        {
            ensure!(
                entry.file_type()?.is_file(),
                "improvement observation must be a regular file"
            );
            let note: Note = read_json(&entry.path())?;
            ensure!(
                entry.file_name().to_str() == Some(&format!("{}.json", hash(&note)?)),
                "improvement observation identity does not match its file"
            );
            result.push(note);
        }
    }
    result.sort_by_cached_key(|note| {
        (
            note.kind.clone(),
            note.summary.clone(),
            note.skills.clone(),
            note.evidence.clone(),
        )
    });
    Ok(result)
}

/// One recorded incident: a failure that needs a prevention decision for its rule.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Incident {
    pub schema: u32,
    pub id: String,
    pub rule: String,
    pub session: String,
    #[serde(with = "incident_source_key")]
    pub source: incident_rules::Source,
    pub evidence: Vec<String>,
}

/// Stores an incident source as the name its rule module gives it.
mod incident_source_key {
    use super::incident_rules::Source;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde passes every field to a serializer by reference"
    )]
    pub fn serialize<S: Serializer>(source: &Source, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(source.key())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Source, D::Error> {
        let key = String::deserialize(deserializer)?;
        Source::from_key(&key).ok_or_else(|| {
            D::Error::custom(format!(
                "unknown incident source `{key}`, expected one of {:?}",
                Source::ALL.map(Source::key)
            ))
        })
    }
}

impl Incident {
    /// A new incident of `rule`, identified by the content that makes a retry of the same failure deduplicate.
    ///
    /// # Errors
    ///
    /// Returns an error when the rule or the session holds no name.
    pub fn new(
        rule: &str,
        session: &str,
        source: incident_rules::Source,
        evidence: Vec<String>,
    ) -> Result<Self> {
        ensure!(!rule.trim().is_empty(), "an incident names its rule");
        ensure!(
            platform::refusal::Refusal::new(rule, "failure observation", "inspect the failure")
                .evidence("native event")
                .is_complete(),
            "an incident names a valid dotted rule"
        );
        ensure!(!session.trim().is_empty(), "an incident names its session");
        Ok(Self {
            schema: 1,
            id: hash(&(rule, session, source.key(), &evidence))?,
            rule: rule.into(),
            session: session.into(),
            source,
            evidence,
        })
    }
}

/// Records failed native tool events and structured verification diagnostics.
///
/// # Errors
///
/// Returns an error for an unknown source or a recording failure.
pub fn record_native_failure(
    state: &Path,
    client: &str,
    event: &serde_json::Value,
) -> Result<bool> {
    let response = event
        .get("error")
        .or_else(|| event.get("tool_response"))
        .or_else(|| event.get("output"));
    let output = response
        .and_then(|response| {
            response.as_str().or_else(|| {
                response["output"]
                    .as_str()
                    .or_else(|| response["stderr"].as_str())
            })
        })
        .unwrap_or_default();
    let refusal = platform::refusal::Refusal::find(output);
    let failed = event["hook_event_name"] == "PostToolUseFailure"
        || event["status"] == "error"
        || event["status"] == "failed"
        || event["is_error"].as_bool().unwrap_or(false)
        || response.is_some_and(|response| {
            response["is_error"].as_bool().unwrap_or(false)
                || response["exit_code"].as_i64().is_some_and(|code| code != 0)
        });
    let source = match event["failure_source"].as_str() {
        Some(source) => {
            incident_rules::Source::from_key(source).context("unknown native failure source")?
        }
        None if refusal.is_some() => incident_rules::Source::GateRefusal,
        None if failed => incident_rules::Source::FailedDelivery,
        None => return Ok(false),
    };
    let session = event["session_id"]
        .as_str()
        .or_else(|| event["thread_id"].as_str())
        .filter(|session| !session.is_empty())
        .unwrap_or("no-session");
    let rule = refusal.as_ref().map_or_else(
        || "native.tool-failure".to_owned(),
        |refusal| refusal.rule.clone(),
    );
    let evidence = vec![
        format!("{client} native event {}", hash(event)?),
        refusal.map_or_else(|| source.key().to_owned(), |refusal| refusal.cause),
    ];
    let expected = event["expected_failure"].as_str().unwrap_or_default();
    record_incident(
        state,
        &Incident::new(&rule, session, source, evidence)?,
        incident_rules::Expected {
            red_test: expected == "red-test",
            counterexample: expected == "counterexample",
            dry_run: expected == "dry-run",
        },
    )
}

/// Records one incident under `incidents/` unless the recorder marks the failure expected, returning whether the store lacked it before.
///
/// # Errors
///
/// Returns an error when the incident has an invalid schema or identity, a stored incident with the same identity differs, or writing the file fails.
pub fn record_incident(
    state: &Path,
    incident: &Incident,
    expected: incident_rules::Expected,
) -> Result<bool> {
    if incident_rules::excluded(expected) {
        return Ok(false);
    }
    ensure!(
        incident.schema == 1
            && incident.id.len() == skill_rules::DIGEST_LENGTH
            && incident.id.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !incident.rule.trim().is_empty(),
        "invalid incident identity"
    );
    ensure!(
        Incident::new(
            &incident.rule,
            &incident.session,
            incident.source,
            incident.evidence.clone()
        )?
        .id == incident.id,
        "incident content differs from its identity"
    );
    let directory = state.join("incidents");
    let destination = directory.join(format!("{}.json", incident.id));
    if let Ok(existing) = read_json::<Incident>(&destination) {
        ensure!(
            existing == *incident,
            "conflicting replay of an incident record"
        );
        return Ok(false);
    }
    match write_json(&destination, incident, false) {
        Ok(()) => Ok(true),
        Err(error) => match read_json::<Incident>(&destination) {
            Ok(existing) if existing == *incident => Ok(false),
            Ok(_) => Err(anyhow::anyhow!(
                "conflicting concurrent incident record: {error}"
            )),
            Err(_) => Err(error),
        },
    }
}

/// Reads the recorded incidents under `incidents/`, sorted by rule, session, and identity.
///
/// # Errors
///
/// Returns an error when reading the directory fails, a `.json` entry names something other than a regular file or fails validation, or a filename differs from its incident's identity.
pub fn incidents(state: &Path) -> Result<Vec<Incident>> {
    let directory = state.join("incidents");
    if !directory.try_exists()? {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|value| value == "json")
        {
            ensure!(
                entry.file_type()?.is_file(),
                "incident record must be a regular file"
            );
            let incident: Incident = read_json(&entry.path())?;
            ensure!(
                entry.file_name().to_str() == Some(&format!("{}.json", incident.id)),
                "incident identity does not match its file"
            );
            result.push(incident);
        }
    }
    result.sort_by_cached_key(|incident| {
        (
            incident.rule.clone(),
            incident.session.clone(),
            incident.id.clone(),
        )
    });
    Ok(result)
}

/// The disposition of one recorded incident: mechanized prevention, or an exemption.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Discharge {
    pub schema: u32,
    pub rule: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incident: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// `mechanized` names gate or test evidence, and `exempt` names a reason with a reconsideration condition.
    pub outcome: String,
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconsideration: Option<String>,
}

/// Reads the recorded discharges under `discharges/`, sorted by rule.
///
/// # Errors
///
/// Returns an error when reading the directory fails, a `.json` entry names something other than a regular file or fails validation, or a filename differs from its discharge's identity.
pub fn discharges(state: &Path) -> Result<Vec<Discharge>> {
    let directory = state.join("discharges");
    if !directory.try_exists()? {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|value| value == "json")
        {
            ensure!(
                entry.file_type()?.is_file(),
                "discharge record must be a regular file"
            );
            let discharge: Discharge = read_json(&entry.path())?;
            ensure!(discharge.schema == 1, "unknown discharge schema");
            if let Some(incident) = &discharge.incident {
                ensure!(
                    incident.len() == skill_rules::DIGEST_LENGTH
                        && incident.bytes().all(|byte| byte.is_ascii_hexdigit()),
                    "invalid discharge incident identity"
                );
                let outcome = match discharge.outcome.as_str() {
                    "mechanized" => incident_rules::Outcome::Mechanized {
                        evidence_present: !discharge.evidence.is_empty()
                            && discharge.evidence.iter().all(|evidence| {
                                evidence.split_once(EVIDENCE_SEPARATOR).is_some_and(
                                    |(path, test)| !path.is_empty() && !test.is_empty(),
                                )
                            }),
                        reproduces: discharge.commit.as_ref().is_some_and(|commit| {
                            GIT_COMMIT_HEX_LENGTHS.contains(&commit.len())
                                && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
                        }),
                    },
                    "exempt" => incident_rules::Outcome::Exempt {
                        reason: discharge
                            .reason
                            .as_deref()
                            .and_then(incident_rules::Reason::from_key),
                        reconsideration: discharge
                            .reconsideration
                            .as_deref()
                            .is_some_and(|text| !text.trim().is_empty()),
                    },
                    other => anyhow::bail!("unknown discharge outcome `{other}`"),
                };
                ensure!(
                    incident_rules::complete(outcome),
                    "incomplete discharge record"
                );
            }
            ensure!(
                entry.file_name().to_str()
                    == Some(&format!(
                        "{}.json",
                        discharge.incident.as_deref().unwrap_or(&discharge.rule)
                    )),
                "discharge identity does not match its file"
            );
            result.push(discharge);
        }
    }
    result.sort_by_cached_key(|discharge| discharge.rule.clone());
    Ok(result)
}

/// The gate and test evidence a mechanized discharge names, each as `path::test`.
const EVIDENCE_SEPARATOR: &str = "::";
const GIT_COMMIT_HEX_LENGTHS: [usize; 2] = [40, 64];
pub const INCIDENT_EVIDENCE_ROOT: &str = "SKILL_OPS_INCIDENT_EVIDENCE_ROOT";

/// The path half of one evidence entry, or an empty path when the entry names no test.
fn evidence_path(evidence: &str) -> &str {
    evidence
        .split_once(EVIDENCE_SEPARATOR)
        .map_or("", |(path, _)| path)
}

fn evidence_at_commit(root: &Path, evidence: &str, commit: &str) -> bool {
    if !GIT_COMMIT_HEX_LENGTHS.contains(&commit.len())
        || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return false;
    }
    let Ok(root) = platform::paths::canonical(root) else {
        return false;
    };
    let Ok(path) = platform::paths::canonical(&root.join(evidence_path(evidence))) else {
        return false;
    };
    let Ok(relative) = path.strip_prefix(&root) else {
        return false;
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    let Ok(contents) = fs::read(&path) else {
        return false;
    };
    let Ok(kind) = platform::tool::Tool::Git
        .command()
        .current_dir(&root)
        .args(["cat-file", "-t", commit])
        .output()
    else {
        return false;
    };
    if !kind.status.success() || kind.stdout != b"commit\n" {
        return false;
    }
    let Ok(source) = platform::tool::Tool::Git
        .command()
        .current_dir(&root)
        .args(["show", &format!("{commit}:{relative}")])
        .output()
    else {
        return false;
    };
    source.status.success() && source.stdout == contents
}

fn evidence_incident(root: &Path, rule: &str) -> bool {
    let Ok(root) = platform::paths::canonical(root) else {
        return false;
    };
    let Ok(entries) = fs::read_dir(&root) else {
        return false;
    };
    entries.filter_map(std::result::Result::ok).any(|entry| {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            return false;
        }
        let Ok(state) = platform::paths::canonical(&entry.path().join("state")) else {
            return false;
        };
        state.starts_with(&root)
            && incidents(&state).is_ok_and(|incidents| {
                incidents.into_iter().any(|incident| {
                    incident.rule == rule
                        && incident.schema == 1
                        && !incident.evidence.is_empty()
                        && Incident::new(
                            &incident.rule,
                            &incident.session,
                            incident.source,
                            incident.evidence.clone(),
                        )
                        .is_ok_and(|recorded| recorded.id == incident.id)
                })
            })
    })
}

/// Whether one mechanized evidence entry exists under `root` and names a test that reproduces the incident.
fn evidence_reproduces(root: &Path, evidence: &str, rule: &str) -> bool {
    let Some((path, test)) = evidence.split_once(EVIDENCE_SEPARATOR) else {
        return false;
    };
    if test.is_empty()
        || !test
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_:".contains(&byte))
    {
        return false;
    }
    let Ok(root) = platform::paths::canonical(root) else {
        return false;
    };
    let Ok(path) = platform::paths::canonical(&root.join(path)) else {
        return false;
    };
    if !path.starts_with(&root) {
        return false;
    }
    let Some(package) = path
        .ancestors()
        .skip(1)
        .take_while(|parent| parent.starts_with(&root))
        .find(|parent| parent.join("Cargo.toml").is_file())
    else {
        return false;
    };
    let Ok(target) = tempfile::tempdir() else {
        return false;
    };
    let Ok(incident_evidence) = tempfile::tempdir() else {
        return false;
    };
    let mut command = platform::tool::Tool::Cargo.command();
    command
        .current_dir(package)
        .args(["test", "--offline", "--manifest-path"])
        .arg(package.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", target.path())
        .env(INCIDENT_EVIDENCE_ROOT, incident_evidence.path())
        .env("SKILL_OPS_STATE", incident_evidence.path().join("state"))
        .env("DOTFILES_EXPECTED_FAILURE", "counterexample");
    if path
        .parent()
        .is_some_and(|parent| parent == package.join("tests"))
    {
        let Some(name) = path.file_stem() else {
            return false;
        };
        command.arg("--test").arg(name);
    } else {
        command.arg("--lib");
    }
    let Ok(output) = command
        .arg(test)
        .args(["--", "--nocapture", "--test-threads=1"])
        .output()
    else {
        return false;
    };
    let transcript = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        eprintln!("incident evidence validator failed: {transcript}");
    }
    output.status.success()
        && transcript.contains("test result: ok. 1 passed; 0 failed;")
        && (platform::refusal::Refusal::find(&transcript)
            .is_some_and(|refusal| refusal.rule == rule)
            || evidence_incident(incident_evidence.path(), rule))
}

/// Records one discharge after its evidence holds, replacing that incident's previous disposition.
///
/// # Errors
///
/// Refuses mismatched identities and rules, unknown outcomes, changed prevention evidence, and tests that fail to reproduce the incident.
/// An exemption must name its reason and reconsideration condition.
pub fn record_discharge(state: &Path, root: &Path, discharge: &Discharge) -> Result<()> {
    record_discharges(state, root, core::slice::from_ref(discharge))
}

/// Records each incident's disposition and checks shared evidence once per batch.
///
/// # Errors
///
/// Refuses unknown incident identities, mismatched rules, uncommitted evidence, and incomplete prevention.
pub fn record_discharges(state: &Path, root: &Path, discharges: &[Discharge]) -> Result<()> {
    let recorded = incidents(state)?;
    let mut verified: BTreeMap<(String, String, String), bool> = BTreeMap::new();
    for discharge in discharges {
        ensure!(discharge.schema == 1, "unknown discharge schema");
        let incident = recorded
            .iter()
            .find(|incident| Some(&incident.id) == discharge.incident.as_ref())
            .context("a discharge must name one recorded incident identity")?;
        ensure!(
            incident.rule == discharge.rule,
            "the discharge rule differs from its incident"
        );
        ensure!(
            !discharge.rule.trim().is_empty(),
            "a discharge names its rule"
        );
        ensure!(
            platform::refusal::Refusal::new(
                &discharge.rule,
                "failure observation",
                "inspect the failure"
            )
            .evidence("native event")
            .is_complete(),
            "a discharge names a valid dotted rule"
        );
        let outcome = match discharge.outcome.as_str() {
            "mechanized" => incident_rules::Outcome::Mechanized {
                evidence_present: !discharge.evidence.is_empty()
                    && discharge
                        .evidence
                        .iter()
                        .all(|evidence| root.join(evidence_path(evidence)).is_file()),
                reproduces: !discharge.evidence.is_empty()
                    && discharge.evidence.iter().all(|evidence| {
                        let Some(commit) = discharge.commit.as_ref() else {
                            return false;
                        };
                        let key = (commit.clone(), evidence.clone(), discharge.rule.clone());
                        *verified.entry(key).or_insert_with(|| {
                            evidence_at_commit(root, evidence, commit)
                                && evidence_reproduces(root, evidence, &discharge.rule)
                        })
                    }),
            },
            "exempt" => incident_rules::Outcome::Exempt {
                reason: discharge
                    .reason
                    .as_deref()
                    .and_then(incident_rules::Reason::from_key),
                reconsideration: discharge
                    .reconsideration
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty()),
            },
            other => anyhow::bail!("unknown discharge outcome `{other}`"),
        };
        ensure!(
            incident_rules::complete(outcome),
            "the discharge of `{}` holds no complete prevention: mechanized evidence names an existing gate or test that reproduces the incident, and an exemption names a closed-set reason with a reconsideration condition",
            discharge.rule
        );
        write_json(
            &state
                .join("discharges")
                .join(format!("{}.json", incident.id)),
            discharge,
            true,
        )?;
    }
    Ok(())
}

/// The rules whose recorded incidents still lack a complete disposition, ranked by repetition first.
///
/// # Errors
///
/// Returns an error when reading the incidents or the discharges fails.
pub fn open_incident_rules(state: &Path) -> Result<Vec<(String, u64, incident_rules::Rank)>> {
    let discharged: BTreeSet<_> = discharges(state)?
        .into_iter()
        .filter_map(|discharge| {
            discharge
                .incident
                .map(|incident| (discharge.rule, incident))
        })
        .collect();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for incident in incidents(state)? {
        if !discharged.contains(&(incident.rule.clone(), incident.id)) {
            *counts.entry(incident.rule).or_default() += 1;
        }
    }
    let mut open: Vec<_> = counts
        .into_iter()
        .map(|(rule, repetitions)| {
            let rank = incident_rules::rank(repetitions);
            (rule, repetitions, rank)
        })
        .collect();
    open.sort_by(|(_, _, first), (_, _, second)| second.cmp(first));
    Ok(open)
}
