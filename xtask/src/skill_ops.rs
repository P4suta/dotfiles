use crate::skill_rules::{Maintenance, composition_candidate, maintenance_current};
pub use crate::skill_rules::{Outcome, decision_complete};
use anyhow::{Context, Result, ensure};
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
    pub also: Vec<String>,
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
    pub outcome: Outcome,
    pub reason: String,
    pub evidence: Vec<String>,
    pub revisit: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub report_hash: String,
    pub decisions: Vec<Decision>,
}

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
                .insert(&requirement.skill, &requirement.also)
                .is_none(),
            "duplicate dependency declaration"
        );
        let mut unique = BTreeSet::new();
        for name in &requirement.also {
            ensure!(
                available.contains(name) && unique.insert(name),
                "unknown or duplicate dependency: {name}"
            );
        }
    }
    fn visit(
        name: &str,
        available: &BTreeSet<String>,
        graph: &BTreeMap<&String, &Vec<String>>,
        visiting: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
        plan: &mut Vec<String>,
    ) -> Result<()> {
        ensure!(available.contains(name), "unknown skill: {name}");
        if done.contains(name) {
            return Ok(());
        }
        ensure!(
            visiting.insert(name.into()),
            "cyclic skill dependency at {name}"
        );
        if let Some(dependencies) = graph.get(&name.to_string()) {
            for dependency in *dependencies {
                visit(dependency, available, graph, visiting, done, plan)?;
            }
        }
        visiting.remove(name);
        done.insert(name.into());
        plan.push(name.into());
        Ok(())
    }
    let mut plan = Vec::new();
    let mut visiting = BTreeSet::new();
    let mut done = BTreeSet::new();
    for name in requested {
        visit(name, available, &graph, &mut visiting, &mut done, &mut plan)?;
    }
    Ok(plan)
}

pub fn check_review(
    report_hash: &str,
    findings: &[Finding],
    review: &Review,
    evidence_root: &Path,
) -> Result<()> {
    ensure!(
        report_hash == review.report_hash,
        "stale skill analysis; analyze the current catalog and evidence"
    );
    let expected: BTreeSet<_> = findings.iter().map(|finding| &finding.id).collect();
    ensure!(
        expected.len() == findings.len(),
        "duplicate analysis finding"
    );
    let mut found = BTreeSet::new();
    let root = evidence_root.canonicalize()?;
    for decision in &review.decisions {
        ensure!(
            expected.contains(&decision.id) && found.insert(&decision.id),
            "unknown or duplicate skill decision: {}",
            decision.id
        );
        ensure!(
            decision_complete(
                decision.outcome,
                !decision.reason.trim().is_empty(),
                !decision.evidence.is_empty(),
                decision
                    .revisit
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
            ),
            "incomplete decision: {}",
            decision.id
        );
        for evidence in &decision.evidence {
            let path = Path::new(evidence);
            ensure!(
                !evidence.is_empty()
                    && path
                        .components()
                        .all(|part| matches!(part, Component::Normal(_))),
                "evidence must be a relative local path"
            );
            let resolved = root
                .join(path)
                .canonicalize()
                .context("missing decision evidence")?;
            ensure!(
                resolved.starts_with(&root) && resolved.is_file(),
                "decision evidence escapes its root or is not a file"
            );
        }
    }
    ensure!(
        found == expected,
        "unprocessed skill recommendations; record every disposition with evidence"
    );
    Ok(())
}

pub fn hash(value: &impl Serialize) -> Result<String> {
    Ok(digest(&serde_json::to_vec(value)?))
}

pub fn evidence_hashes(review: &Review, root: &Path) -> Result<BTreeMap<String, String>> {
    let root = root.canonicalize()?;
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
        let resolved = root.join(path).canonicalize()?;
        ensure!(
            resolved.starts_with(&root) && resolved.is_file(),
            "evidence escapes its root"
        );
        result.insert(name.clone(), digest(&fs::read(resolved)?));
    }
    Ok(result)
}

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

pub fn engine_hash() -> Result<String> {
    hash(&(
        include_str!("skill_ops.rs"),
        include_str!("skill_rules.rs"),
        include_str!("skill_install.rs"),
        include_str!("skill_proofs.rs"),
        include_str!("bin/skill-ops.rs"),
        include_str!("../../dot_config/opencode/plugins/skill-ops.ts"),
        include_str!("lib.rs"),
        include_str!("main.rs"),
        include_str!("../Cargo.toml"),
        include_str!("../Cargo.lock"),
        include_str!("../../package.json"),
        include_str!("../../bun.lock"),
        include_str!("../../tsconfig.json"),
        (
            include_str!("../../mise.toml"),
            include_str!("../../.chezmoiignore"),
            include_str!("../../lefthook.yml"),
            include_str!("../../.github/workflows/required.yml"),
            include_str!("pr_rules.rs"),
            include_str!("pr_workflow.rs"),
            include_str!("bin/pr-workflow.rs"),
            include_str!("review_guard.rs"),
            include_str!("review_rules.rs"),
        ),
    ))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Approved {
    pub catalog: Catalog,
    pub catalog_hash: String,
    pub policy_hash: String,
    pub engine_hash: String,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

pub fn catalog(tree: &Path) -> Result<Catalog> {
    capture_catalog(tree, None)
}

pub fn catalog_owned(tree: &Path, scope: &Catalog) -> Result<Catalog> {
    capture_catalog(tree, Some(scope))
}

fn capture_catalog(tree: &Path, scope: Option<&Catalog>) -> Result<Catalog> {
    let tree = tree.canonicalize()?;
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

pub fn validate_policy(policy: &Policy, catalog: &Catalog) -> Result<()> {
    ensure!(
        policy.schema == 1
            && policy.review_every > 0
            && policy.minimum_pair_sessions >= 2
            && (1..=100).contains(&policy.pair_percent)
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
                && let Ok(resolved) = Path::new(path).canonicalize()
            {
                let root = owned_tree.canonicalize()?;
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

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?
        .sync_all()
        .context("persist skill operations directory")?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub fn write_json(path: &Path, value: &impl Serialize, replace: bool) -> Result<()> {
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
    serde_json::to_writer_pretty(&mut temporary, value)?;
    writeln!(temporary)?;
    temporary.as_file().sync_all()?;
    if replace {
        temporary.persist(path)?;
    } else {
        temporary.persist_noclobber(path)?;
    }
    sync_directory(directory)?;
    Ok(())
}

pub fn record(state: &Path, observation: &Observation) -> Result<bool> {
    ensure!(
        observation.schema == 1
            && observation.id.len() == 64
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

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    ensure!(
        fs::metadata(path)?.len() <= 16_777_216,
        "skill operations input exceeds its size bound"
    );
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("invalid skill operations data: {}", path.display()))
}

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
        for also in &requirement.also {
            let missing = primary.difference(&report.usage[also].sessions).count();
            if missing > 0 {
                report.findings.push(Finding { id: format!("dependency:{}:{also}", requirement.skill), kind: "dependency-observation-gap".into(), skills: vec![requirement.skill.clone(), also.clone()], detail: format!("{missing} sessions lack an observed dependency load; inspect collection coverage and use the dependency-aware loader.") });
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

pub fn check_catalog(root: &Path) -> Result<()> {
    crate::validate_tree(root)?;
    let catalog = catalog(&root.join("dot_agents/skills"))?;
    let policy: Policy = read_json(&root.join("dot_config/skill-ops/policy.json"))?;
    let report = analyze(&catalog, &policy, &[], &[])?;
    let review: Review = read_json(&root.join("docs/skills/review.json"))?;
    check_review(&hash(&report)?, &report.findings, &review, root)
}

#[derive(Clone, Copy)]
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
    if let Some(report) = report {
        ensure!(
            report.schema == 1 && report.event_ids.is_subset(&current),
            "previously observed events disappeared; inspect local state"
        );
        ensure!(
            report.note_ids.is_subset(&note_ids),
            "previous improvement observations disappeared"
        );
        let review_complete = match review {
            Some(review) => {
                check_review(&hash(report)?, &report.findings, review, root)?;
                true
            }
            None => false,
        };
        Ok(!maintenance_current(Maintenance {
            catalog_current: report.catalog_hash == hash(catalog)?
                && report.engine_hash == engine_hash()?,
            policy_current: report.policy_hash == hash(policy)?,
            observations_preserved: report.event_ids.is_subset(&current),
            notes_current: report.note_ids == note_ids,
            review_complete,
            new_observations: current.difference(&report.event_ids).count(),
            threshold: policy.review_every,
        }))
    } else {
        Ok(hash(catalog)? != approved.catalog_hash
            || hash(policy)? != approved.policy_hash
            || engine_hash()? != approved.engine_hash
            || current.len() >= policy.review_every
            || !notes.is_empty())
    }
}

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
