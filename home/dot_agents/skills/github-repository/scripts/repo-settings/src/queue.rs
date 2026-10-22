use crate::merge_queue_rules::{Decision, decide};
use crate::{Change, Finding, Operation, State, Workflow, number};
use anyhow::{Context, Result, bail, ensure};
use glob::{MatchOptions, Pattern};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use yaml_rust2::{Yaml, YamlLoader};

const MAX_ENTRIES_TO_BUILD: u64 = 1;
const MIN_ENTRIES_TO_MERGE: u64 = 1;
const MAX_ENTRIES_TO_MERGE: u64 = 1;
const MIN_ENTRIES_TO_MERGE_WAIT_MINUTES: u64 = 0;
const CHECK_RESPONSE_TIMEOUT_MINUTES: u64 = 60;
const MATRIX_JOB_LIMIT: usize = 256;

fn matches_ref(pattern: &str, branch: &str) -> Result<bool> {
    if matches!(pattern, "~ALL" | "~DEFAULT_BRANCH") {
        return Ok(true);
    }
    Ok(Pattern::new(pattern)?.matches_with(
        &format!("refs/heads/{branch}"),
        MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        },
    ))
}

fn default_branch_ruleset(ruleset: &Value, branch: &str) -> Result<bool> {
    if ruleset["target"] != "branch" || ruleset["enforcement"] != "active" {
        return Ok(false);
    }
    let refs = &ruleset["conditions"]["ref_name"];
    let mut included = false;
    for pattern in refs["include"]
        .as_array()
        .context("missing ruleset ref includes")?
    {
        included |= matches_ref(pattern.as_str().context("invalid ref include")?, branch)?;
    }
    for pattern in refs["exclude"]
        .as_array()
        .context("missing ruleset ref excludes")?
    {
        if matches_ref(pattern.as_str().context("invalid ref exclusion")?, branch)? {
            return Ok(false);
        }
    }
    Ok(included)
}

fn rules(ruleset: &Value) -> Result<&Vec<Value>> {
    ruleset["rules"].as_array().context("missing ruleset rules")
}

fn check_contexts(ruleset: &Value) -> Result<BTreeSet<String>> {
    let mut contexts = BTreeSet::new();
    for rule in rules(ruleset)? {
        if rule["type"] == "required_status_checks" {
            for check in rule["parameters"]["required_status_checks"]
                .as_array()
                .context("missing required checks")?
            {
                contexts.insert(
                    check["context"]
                        .as_str()
                        .context("missing required check context")?
                        .to_owned(),
                );
            }
        }
    }
    Ok(contexts)
}

pub(crate) fn changes(
    state: &State,
    changes: &mut Vec<Change>,
    findings: &mut Vec<Finding>,
) -> Result<()> {
    let mut applicable = Vec::new();
    let mut contexts = BTreeSet::new();
    let mut present = false;
    for ruleset in &state.rulesets {
        if default_branch_ruleset(ruleset, &state.repository.default_branch)? {
            contexts.extend(check_contexts(ruleset)?);
            present |= rules(ruleset)?
                .iter()
                .any(|rule| rule["type"] == "merge_queue");
            applicable.push(ruleset);
        }
    }
    let mut missing = contexts.clone();
    let mut ready = true;
    if !contexts.is_empty() {
        for workflow in &state.workflows {
            match reporting_checks(workflow, &contexts) {
                Ok((reported, merge_group)) => {
                    for context in &reported {
                        missing.remove(context);
                    }
                    if !reported.is_empty() && !merge_group {
                        ready = false;
                        findings.push(Finding {
                            repository: state.repository.full_name.clone(),
                            message: format!("{} reports required checks but is missing the merge_group trigger.", workflow.path),
                        });
                    }
                }
                Err(error) => {
                    ready = false;
                    findings.push(Finding {
                        repository: state.repository.full_name.clone(),
                        message: format!(
                            "Cannot verify merge_group reporting in {}: {error:#}.",
                            workflow.path
                        ),
                    });
                }
            }
        }
    }
    for context in missing {
        ready = false;
        findings.push(Finding {
            repository: state.repository.full_name.clone(),
            message: format!("No default-branch workflow reports required check {context}; verify its merge_group reporter before enabling a merge queue."),
        });
    }
    match decide(!contexts.is_empty(), ready, present) {
        Decision::NoChecks => findings.push(Finding {
            repository: state.repository.full_name.clone(),
            message: "Bind a real successful CI check before adding a required-status rule; the CLI never invents a check name.".into(),
        }),
        Decision::Blocked | Decision::Present => {}
        Decision::Add => {
            let mut add_queue = true;
            for before in applicable {
                if check_contexts(before)?.is_empty() {
                    continue;
                }
                let after = queued_ruleset(before, add_queue)?;
                add_queue = false;
                if after != *before {
                    changes.push(Change {
                        repository: state.repository.full_name.clone(),
                        operation: Operation::UpdateRuleset {
                            id: number(before, "id")?,
                            before: before.clone(),
                            after,
                        },
                    });
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn queued_ruleset(before: &Value, add_queue: bool) -> Result<Value> {
    ensure!(
        !check_contexts(before)?.is_empty(),
        "queue update requires status checks"
    );
    let mut after = before.clone();
    let rules = after["rules"]
        .as_array_mut()
        .context("missing ruleset rules")?;
    for rule in rules.iter_mut() {
        if rule["type"] == "required_status_checks" {
            rule["parameters"]["strict_required_status_checks_policy"] = json!(false);
        }
    }
    if add_queue {
        rules.push(json!({"type": "merge_queue", "parameters": {
            "merge_method": "SQUASH", "grouping_strategy": "ALLGREEN",
            "max_entries_to_build": MAX_ENTRIES_TO_BUILD,
            "min_entries_to_merge": MIN_ENTRIES_TO_MERGE,
            "max_entries_to_merge": MAX_ENTRIES_TO_MERGE,
            "min_entries_to_merge_wait_minutes": MIN_ENTRIES_TO_MERGE_WAIT_MINUTES,
            "check_response_timeout_minutes": CHECK_RESPONSE_TIMEOUT_MINUTES
        }}));
    }
    Ok(after)
}

pub(crate) fn payload(ruleset: &Value) -> Result<Value> {
    let mut payload = serde_json::Map::new();
    for key in [
        "name",
        "target",
        "enforcement",
        "conditions",
        "bypass_actors",
        "rules",
    ] {
        payload.insert(
            key.to_owned(),
            ruleset
                .get(key)
                .with_context(|| format!("missing ruleset {key}"))?
                .clone(),
        );
    }
    Ok(Value::Object(payload))
}

fn reporting_checks(
    workflow: &Workflow,
    contexts: &BTreeSet<String>,
) -> Result<(BTreeSet<String>, bool)> {
    let documents = YamlLoader::load_from_str(&workflow.yaml)?;
    ensure!(documents.len() == 1, "expected one workflow document");
    let document = &documents[0];
    let trigger = &document["on"];
    let merge_group = match trigger {
        Yaml::String(event) => event == "merge_group",
        Yaml::Array(events) => events
            .iter()
            .any(|event| event.as_str() == Some("merge_group")),
        Yaml::Hash(events) => events.contains_key(&Yaml::String("merge_group".into())),
        _ => false,
    };
    let mut reported = BTreeSet::new();
    for (key, job) in document["jobs"]
        .as_hash()
        .context("missing workflow jobs")?
    {
        let key = key.as_str().context("invalid job key")?;
        let name = job["name"].as_str().unwrap_or(key);
        let mut names = BTreeSet::from([key.to_owned(), name.to_owned()]);
        if !job["strategy"]["matrix"].is_badvalue() {
            for row in matrix_rows(&job["strategy"]["matrix"])? {
                let rendered = render_name(name, &row)?;
                if name.contains("${{") {
                    names.insert(rendered);
                } else {
                    names.insert(format!(
                        "{name} ({})",
                        row.iter()
                            .map(|(_, value)| value.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
        reported.extend(contexts.intersection(&names).cloned());
    }
    Ok((reported, merge_group))
}

fn scalar(value: &Yaml) -> Result<String> {
    match value {
        Yaml::String(value) | Yaml::Real(value) => Ok(value.clone()),
        Yaml::Integer(value) => Ok(value.to_string()),
        Yaml::Boolean(value) => Ok(value.to_string()),
        _ => bail!("matrix values require a static scalar"),
    }
}

type MatrixRow = Vec<(String, String)>;

fn matrix_row(value: &Yaml) -> Result<MatrixRow> {
    value
        .as_hash()
        .context("invalid matrix row")?
        .iter()
        .map(|(key, value)| {
            Ok((
                key.as_str().context("invalid matrix key")?.to_owned(),
                scalar(value)?,
            ))
        })
        .collect()
}

fn matrix_rows(matrix: &Yaml) -> Result<Vec<MatrixRow>> {
    let axes = matrix
        .as_hash()
        .context("matrix must be statically defined")?;
    let mut rows = vec![Vec::new()];
    let mut axis_names = BTreeSet::new();
    for (key, values) in axes {
        let key = key.as_str().context("invalid matrix axis")?;
        if matches!(key, "include" | "exclude") {
            continue;
        }
        axis_names.insert(key.to_owned());
        let values = values
            .as_vec()
            .context("matrix axis must be a static array")?;
        ensure!(
            rows.len().saturating_mul(values.len()) <= MATRIX_JOB_LIMIT,
            "matrix exceeds job limit"
        );
        let mut expanded = Vec::new();
        for row in &rows {
            for value in values {
                let mut row = row.clone();
                row.push((key.to_owned(), scalar(value)?));
                expanded.push(row);
            }
        }
        rows = expanded;
    }
    if let Some(excludes) = matrix["exclude"].as_vec() {
        for exclude in excludes {
            let exclude = matrix_row(exclude)?;
            rows.retain(|row| !exclude.iter().all(|entry| row.contains(entry)));
        }
    }
    if axis_names.is_empty() {
        rows.clear();
    }
    let original = rows.clone();
    if let Some(includes) = matrix["include"].as_vec() {
        for include in includes {
            let include = matrix_row(include)?;
            let mut merged = false;
            for (base, row) in original.iter().zip(&mut rows) {
                if include.iter().all(|(key, value)| {
                    !axis_names.contains(key) || base.contains(&(key.clone(), value.clone()))
                }) {
                    for (key, value) in &include {
                        if let Some((_, previous)) = row.iter_mut().find(|(name, _)| name == key) {
                            *previous = value.clone();
                        } else {
                            row.push((key.clone(), value.clone()));
                        }
                    }
                    merged = true;
                }
            }
            if !merged {
                rows.push(include);
            }
        }
    }
    ensure!(rows.len() <= MATRIX_JOB_LIMIT, "matrix exceeds job limit");
    Ok(rows)
}

fn render_name(template: &str, row: &MatrixRow) -> Result<String> {
    let mut rest = template;
    let mut rendered = String::new();
    while let Some((prefix, expression)) = rest.split_once("${{") {
        rendered.push_str(prefix);
        let (expression, suffix) = expression
            .split_once("}}")
            .context("unclosed job name expression")?;
        let key = expression
            .trim()
            .strip_prefix("matrix.")
            .context("job name requires a static matrix expression")?;
        let (_, value) = row
            .iter()
            .find(|(name, _)| name == key)
            .context("job name references an unknown matrix value")?;
        rendered.push_str(value);
        rest = suffix;
    }
    rendered.push_str(rest);
    Ok(rendered)
}
