//! Source-bound compatibility assessment before replacing the native mutation campaign.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[path = "njutest_producer.rs"]
mod producer;

pub const UPSTREAM_REVISION: &str = "46d5b5f4446503162ea18189756249231abb6fc7";
const TOOL_VERSION: &str = "0.1.0";
const SEALED_TARGET: &str = "wasm32-wasip1";
const SEALED_POINTER_WIDTH: u8 = 32;
const CANONICAL_ENVIRONMENT: &str = "home/.chezmoitemplates/partials/njutest-mise.toml";
const GENERATED_ENVIRONMENT: &str = "mise.njutest.toml";
const PACKAGE: &str = "rules";
const INVALID_RUN_EXIT_CODE: i64 = 2;
const INTERRUPTED_RUN_EXIT_CODE: i64 = 130;
const COLD_MEASUREMENT_CASES: usize = 2;
const WARM_MEASUREMENT_CASES: usize = 4;
const ASSURANCE_SCHEMA: &str = include_str!("../schema/njutest/njutest-assurance-report-v1.json");
const CATALOG_SCHEMA: &str = include_str!("../schema/njutest/rust-mutants-catalog-v1.json");
const ENGINE_SCHEMA: &str = include_str!("../schema/njutest/rust-mutants-run-report-v1.json");

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Document {
    Catalog,
    Engine,
    Assurance,
}

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Temperature {
    Cold,
    Warm,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Prepare the optional managed environment through the guarded installer.
    Prepare,
    /// Decode an imported document without admitting execution or native parity.
    Inspect {
        #[arg(value_enum)]
        document: Document,
        file: PathBuf,
    },
    /// Ask the pinned runner what the complete rules-package run would measure.
    Plan,
    /// Measure an owned committed snapshot and report every result as an assessment.
    Run {
        #[arg(long, value_enum)]
        temperature: Temperature,
    },
}

#[derive(Debug, Args)]
pub struct Request {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Origin {
    Imported,
    Executed,
}

#[derive(Debug, Serialize)]
pub struct Assessment {
    upstream_revision: &'static str,
    origin: Origin,
    native_parity: &'static str,
    native_target: String,
    sealed_target: &'static str,
    sealed_pointer_width: u8,
    cataloged: usize,
    killed: usize,
    survived: usize,
    unproven: usize,
    compile_rejected: usize,
    other_outcomes: usize,
    zero_survivors: bool,
    limitations: Vec<String>,
}

impl Assessment {
    fn imported(target: String, cataloged: usize) -> Self {
        Self {
            upstream_revision: UPSTREAM_REVISION,
            origin: Origin::Imported,
            native_parity: "unmeasured",
            native_target: target,
            sealed_target: SEALED_TARGET,
            sealed_pointer_width: SEALED_POINTER_WIDTH,
            cataloged,
            killed: 0,
            survived: 0,
            unproven: 0,
            compile_rejected: 0,
            other_outcomes: 0,
            zero_survivors: false,
            limitations: vec![
                "Imported documents grant no producer or native execution credit".to_owned(),
            ],
        }
    }
}

fn rows<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value[key]
        .as_array()
        .map(Vec::as_slice)
        .with_context(|| format!("missing {key} rows"))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("missing {key} string"))
}

fn count(value: &Value, key: &str) -> Result<u64> {
    value[key]
        .as_u64()
        .with_context(|| format!("missing {key} count"))
}

fn schema(document: Document) -> &'static str {
    match document {
        Document::Catalog => CATALOG_SCHEMA,
        Document::Engine => ENGINE_SCHEMA,
        Document::Assurance => ASSURANCE_SCHEMA,
    }
}

fn closed_document(document: Document, bytes: &[u8]) -> Result<Value> {
    let expected = match document {
        Document::Assurance => "32dffb64af03f035efa7ff6b64732d888ee1ed0983f9b35988312b0ea6a514a7",
        Document::Catalog => "dc93b3834401a52e4dfa377d5bb7e548350b0bab18ec5183ca3562f3c147d70b",
        Document::Engine => "8f253a6c4b80e71ac89d730ef4ebabd96565fad3399f72f4e689b303ed51ea4a",
    };
    ensure!(
        digest(schema(document).as_bytes()) == expected,
        "pinned original schema bytes changed"
    );
    let original: Value = serde_json::from_str(schema(document))?;
    let validator = jsonschema::options()
        .build(&original)
        .map_err(|error| anyhow::anyhow!("invalid pinned schema: {error}"))?;
    let value: Value = serde_json::from_slice(bytes)?;
    if let Err(errors) = validator.validate(&value) {
        let problems: Vec<_> = errors
            .map(|error| format!("{}: {error}", error.instance_path))
            .collect();
        anyhow::bail!(
            "incompatible pinned {document:?} document: {}",
            problems.join("; ")
        );
    }
    Ok(value)
}

fn identities<'a>(
    mutants: &'a [Value],
    rejections: &'a [Value],
    index: &str,
) -> Result<BTreeSet<&'a str>> {
    ensure!(!mutants.is_empty(), "empty mutation inventory");
    let mut ids = BTreeSet::new();
    let mut indices = BTreeSet::new();
    for row in mutants.iter().chain(rejections) {
        ensure!(
            ids.insert(text(row, "id")?) && indices.insert(count(row, index)?),
            "duplicate mutation identity or catalog position"
        );
    }
    ensure!(
        indices.iter().copied().eq(0..indices.len() as u64),
        "missing or non-dense catalog positions"
    );
    Ok(ids)
}

fn counters(actual: &BTreeMap<&str, u64>, accounting: &Value) -> Result<()> {
    for (name, expected) in actual {
        ensure!(
            count(accounting, name)? == *expected,
            "{name} counter contradicts terminal rows"
        );
    }
    Ok(())
}

fn catalog(value: &Value) -> Result<Assessment> {
    ensure!(
        value["tool_version"] == TOOL_VERSION,
        "unexpected mutation engine version"
    );
    let mutants = rows(value, "mutants")?;
    let rejections = rows(value, "rejections")?;
    identities(mutants, rejections, "index")?;
    for row in mutants {
        let start = count(row, "start_byte")?;
        let end = count(row, "end_byte")?;
        ensure!(
            end >= start && end - start == text(row, "original")?.len() as u64,
            "source span contradicts the original mutation bytes"
        );
    }
    let mut assessment = Assessment::imported(
        text(&value["workspace"]["platform"], "target")?.to_owned(),
        mutants.len(),
    );
    assessment.compile_rejected = rejections
        .iter()
        .filter(|row| row["reason"] == "compiler-refused")
        .count();
    assessment
        .limitations
        .push("Catalog rows establish candidates, not terminal mutation outcomes".to_owned());
    Ok(assessment)
}

fn engine_counts(value: &Value) -> Result<BTreeMap<&'static str, u64>> {
    let mutants = rows(value, "mutants")?;
    let refused = rows(value, "rejections")?
        .iter()
        .filter(|row| row["reason"] == "compiler-refused")
        .count() as u64;
    let skipped = rows(value, "skips")?.iter().try_fold(0u64, |sum, row| {
        sum.checked_add(count(row, "count")?)
            .context("engine skip count overflow")
    })?;
    let mut counts = BTreeMap::from([
        ("cataloged", mutants.len() as u64),
        ("refused", refused),
        ("skipped", skipped),
        ("executed", 0),
        ("killed", 0),
        ("survived", 0),
        ("unproven", 0),
        ("step_limit_reached", 0),
        ("waited", 0),
        ("inconclusive", 0),
        ("errored", 0),
        ("not_run", 0),
        ("unreached", 0),
        ("declined", 0),
        ("expected", 0),
        ("unproven_killed", 0),
        ("unproven_survived", 0),
        ("unproven_unreached", 0),
        ("unproven_discharged", 0),
    ]);
    for row in mutants {
        count_engine_mutation(row, &mut counts)?;
    }
    counts.insert("executed", mutants.len() as u64 - counts["not_run"]);
    Ok(counts)
}

fn count_engine_mutation(row: &Value, counts: &mut BTreeMap<&'static str, u64>) -> Result<()> {
    let outcome = text(row, "outcome")?;
    let reason = row["not_run_reason"].as_str();
    let native = row["evidence"]["kind"] == "unproven";
    let unproven = match outcome {
        "killed" | "survived" => native,
        "not_run" => reason == Some("discharged") || (native && reason == Some("unreached")),
        _ => false,
    };
    let name = if unproven { "unproven" } else { outcome };
    *counts.get_mut(name).context("unknown mutation outcome")? += 1;
    let subset = match (outcome, reason, unproven) {
        ("killed", _, true) => Some("unproven_killed"),
        ("survived", _, true) => Some("unproven_survived"),
        ("not_run", Some("unreached"), true) => Some("unproven_unreached"),
        ("not_run", Some("discharged"), true) => Some("unproven_discharged"),
        ("not_run", Some("unreached"), false) => Some("unreached"),
        ("not_run", Some("declined"), false) => Some("declined"),
        _ => None,
    };
    if let Some(subset) = subset {
        *counts.get_mut(subset).context("missing engine subset")? += 1;
    }
    if row["expected"] == true {
        *counts
            .get_mut("expected")
            .context("missing expected count")? += 1;
    }
    Ok(())
}

fn engine(value: &Value) -> Result<Assessment> {
    ensure!(
        value["tool_version"] == TOOL_VERSION,
        "unexpected mutation engine version"
    );
    let run = &value["run"];
    ensure!(
        run["interrupted"] == false
            && run["shard"].is_null()
            && !matches!(
                run["exit_code"].as_i64(),
                Some(INVALID_RUN_EXIT_CODE | INTERRUPTED_RUN_EXIT_CODE)
            ),
        "failed, interrupted or partial mutation run"
    );
    let mutants = rows(value, "mutants")?;
    let rejections = rows(value, "rejections")?;
    identities(mutants, rejections, "index")?;
    let targets = rows(value, "targets")?;
    let mut target_ids = BTreeSet::new();
    ensure!(!targets.is_empty(), "missing baseline target inventory");
    for target in targets {
        ensure!(
            target_ids.insert(text(target, "id")?),
            "duplicate baseline target"
        );
        ensure!(target["tests"].is_u64(), "baseline target did not complete");
    }
    let mut assessment = Assessment::imported(
        text(&value["workspace"]["platform"], "target")?.to_owned(),
        mutants.len(),
    );
    for row in mutants {
        ensure!(
            row["expected"] == false,
            "accepted outcomes do not establish zero survivors"
        );
        let outcome = text(row, "outcome")?;
        let unproven = row["evidence"]["kind"] == "unproven";
        if unproven {
            assessment.unproven += 1;
        } else {
            match outcome {
                "killed" => assessment.killed += 1,
                "survived" => assessment.survived += 1,
                _ => assessment.other_outcomes += 1,
            }
            validate_execution(row, outcome, &target_ids)?;
        }
    }
    counters(&engine_counts(value)?, &value["accounting"])?;
    assessment.compile_rejected = rejections
        .iter()
        .filter(|row| row["reason"] == "compiler-refused")
        .count();
    ensure!(
        count(&value["accounting"], "refused")? == assessment.compile_rejected as u64,
        "compiler refusal count contradicts rows"
    );
    let clean =
        assessment.survived == 0 && assessment.unproven == 0 && assessment.other_outcomes == 0;
    ensure!(
        run["exit_code"] == i32::from(!clean),
        "engine exit code contradicts terminal outcomes"
    );
    assessment.zero_survivors = clean;
    assessment.limitations.push("Sealed guards answer wasm32-wasip1 with 32-bit usize; native leads do not establish that answer".to_owned());
    Ok(assessment)
}

fn validate_execution(row: &Value, outcome: &str, targets: &BTreeSet<&str>) -> Result<()> {
    let evidence = &row["evidence"];
    if matches!(outcome, "killed" | "survived") {
        ensure!(
            matches!(evidence["kind"].as_str(), Some("sealed" | "unproven")),
            "mutation verdict has no execution evidence"
        );
    }
    if evidence["kind"] == "sealed" {
        let executions = rows(evidence, "executions")?;
        if matches!(outcome, "killed" | "survived") {
            ensure!(
                !executions.is_empty(),
                "sealed verdict has no actual executions"
            );
        }
        for execution in executions {
            ensure!(
                targets.contains(text(execution, "target")?),
                "execution names an absent baseline target"
            );
        }
        if outcome == "killed" {
            ensure!(
                executions.iter().any(|row| matches!(
                    row["came_to"].as_str(),
                    Some("panicked" | "failed" | "trapped")
                )),
                "killed mutation has no detecting sealed execution"
            );
        }
        if outcome == "survived" {
            ensure!(
                executions.iter().all(|row| row["came_to"] == "passed"),
                "survivor contradicts sealed execution outcomes"
            );
        }
    }
    Ok(())
}

fn assurance_counts(mutants: &[Value]) -> Result<BTreeMap<&'static str, u64>> {
    let mut counts = BTreeMap::from([
        ("cataloged", mutants.len() as u64),
        ("rejected", 0),
        ("executed", 0),
        ("killed", 0),
        ("survived", 0),
        ("step_limit_reached", 0),
        ("waited", 0),
        ("unreached", 0),
        ("equivalent", 0),
        ("accepted", 0),
        ("model_noticed", 0),
        ("model_proved", 0),
        ("reused_killed", 0),
        ("reused_survived", 0),
    ]);
    for row in mutants {
        let outcome = text(&row["decision"], "outcome")?;
        let column = match outcome {
            "compile-rejected" => "rejected",
            "step-limit-reached" => "step_limit_reached",
            "model-noticed" => "model_noticed",
            "model-proved" => "model_proved",
            other => other,
        };
        if let Some(count) = counts.get_mut(column) {
            *count += 1;
        }
        if !matches!(outcome, "compile-rejected" | "unreached" | "equivalent") {
            *counts
                .get_mut("executed")
                .context("missing executed count")? += 1;
        }
        if row["accepted"] == true {
            *counts
                .get_mut("accepted")
                .context("missing accepted count")? += 1;
        }
        if row["reuse"]["reused"] == true {
            let column = match outcome {
                "killed" => Some("reused_killed"),
                "survived" => Some("reused_survived"),
                _ => None,
            };
            if let Some(column) = column {
                *counts.get_mut(column).context("missing reuse count")? += 1;
            }
        }
    }
    Ok(counts)
}

fn observer_counts(mutants: &[Value]) -> Result<BTreeMap<&'static str, u64>> {
    let mut counts = BTreeMap::from([
        ("types", 0),
        ("tests", 0),
        ("model_noticed", 0),
        ("model_proved", 0),
        ("step_limit_reached", 0),
        ("proved", 0),
        ("unnoticed", 0),
        ("unreached", 0),
        ("unproven", 0),
        ("waited", 0),
        ("errored", 0),
    ]);
    for row in mutants {
        let outcome = text(&row["decision"], "outcome")?;
        let native_lead = row["evidence"]["kind"] == "unproven"
            && matches!(outcome, "killed" | "survived" | "unreached" | "equivalent");
        let observer = if native_lead {
            "unproven"
        } else {
            match outcome {
                "compile-rejected" => "types",
                "killed" => "tests",
                "model-noticed" => "model_noticed",
                "model-proved" => "model_proved",
                "step-limit-reached" => "step_limit_reached",
                "waited" => "waited",
                "survived" => "unnoticed",
                "unreached" => "unreached",
                "equivalent" => "proved",
                "unconfirmed" | "errored" | "declined" => "errored",
                _ => anyhow::bail!("unknown observer outcome"),
            }
        };
        *counts.get_mut(observer).context("missing observer count")? += 1;
    }
    Ok(counts)
}

fn assurance_baseline(part: &Value) -> Result<BTreeSet<&str>> {
    let targets = rows(part, "targets")?;
    let mut target_names = BTreeSet::new();
    let mut target_ids = BTreeSet::new();
    ensure!(!targets.is_empty(), "missing baseline targets");
    for target in targets {
        ensure!(
            target["status"] == "passed"
                && target_names.insert(text(target, "name")?)
                && target_ids.insert(text(target, "id")?),
            "baseline failed, missing, skipped or duplicated"
        );
    }
    counters(
        &BTreeMap::from([
            ("selected", targets.len() as u64),
            ("passed", targets.len() as u64),
            ("failed", 0),
            ("skipped", 0),
            ("missing", 0),
        ]),
        &part["accounting"]["targets"],
    )?;
    let drift_rows = rows(part, "drift")?;
    let drift: BTreeSet<_> = drift_rows
        .iter()
        .map(|row| text(row, "target"))
        .collect::<Result<_>>()?;
    ensure!(
        drift == target_names
            && drift_rows.len() == target_names.len()
            && drift_rows.iter().all(|row| row["state"] == "held"),
        "missing, duplicate or changed original-code reach control"
    );
    Ok(target_names)
}

fn assurance(value: &Value) -> Result<Assessment> {
    ensure!(
        value["document_type"] == "complete",
        "shard is not a complete assessment"
    );
    let report = &value["report"];
    ensure!(
        report["tool"]["njutest"] == TOOL_VERSION && report["tool"]["rust_mutants"] == TOOL_VERSION,
        "unexpected producer version"
    );
    ensure!(
        report["scope"]["shard"].is_null() && report["composition"]["kind"] == "direct",
        "partial or merged document cannot establish this complete assessment"
    );
    let builds = rows(report, "builds")?;
    let configured = rows(&report["scope"], "configured_builds")?;
    ensure!(
        builds.len() == configured.len()
            && builds
                .iter()
                .zip(configured)
                .all(|(build, name)| build["name"] == *name),
        "missing or reordered configured build"
    );
    let mut assessment = Assessment::imported(String::new(), 0);
    for build in builds {
        let parts = rows(build, "parts")?;
        ensure!(
            parts.len() == 1 && parts[0]["part"]["kind"] == "whole",
            "build does not contain one complete whole-catalog source"
        );
        let part = &parts[0];
        let target = text(&part["toolchain"], "target")?;
        if assessment.native_target.is_empty() {
            target.clone_into(&mut assessment.native_target);
        } else {
            ensure!(
                assessment.native_target == target,
                "assessment combines different native targets"
            );
        }
        let mutants = rows(part, "mutants")?;
        identities(mutants, &[], "catalog_index")?;
        let target_names = assurance_baseline(part)?;
        for row in mutants {
            ensure!(
                row["accepted"] == false,
                "accepted mutation is not a zero-survivor result"
            );
            let outcome = text(&row["decision"], "outcome")?;
            if row["evidence"]["kind"] == "unproven" {
                assessment.unproven += 1;
            } else {
                match outcome {
                    "killed" => assessment.killed += 1,
                    "survived" => assessment.survived += 1,
                    "compile-rejected" => assessment.compile_rejected += 1,
                    _ => assessment.other_outcomes += 1,
                }
            }
            validate_execution(row, outcome, &target_names)?;
        }
        counters(&assurance_counts(mutants)?, &part["accounting"]["mutants"])?;
        counters(
            &observer_counts(mutants)?,
            &part["accounting"]["mutants"]["observers"],
        )?;
        assessment.cataloged += mutants.len();
        for limitation in rows(part, "limitations")? {
            assessment
                .limitations
                .push(text(limitation, "name")?.to_owned());
        }
    }
    assessment.zero_survivors = assessment.survived == 0
        && assessment.unproven == 0
        && assessment.other_outcomes == 0
        && assessment.killed != 0;
    assessment.limitations.push("The sealed 32-bit target does not establish parity with the native 64-bit mutation campaign".to_owned());
    Ok(assessment)
}

fn decode(document: Document, bytes: &[u8]) -> Result<Assessment> {
    let value = closed_document(document, bytes)?;
    match document {
        Document::Catalog => catalog(&value),
        Document::Engine => engine(&value),
        Document::Assurance => assurance(&value),
    }
}

fn digest(bytes: &[u8]) -> String {
    platform::build_inputs::hex(&Sha256::digest(bytes))
}

fn relative(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "source path escapes the captured workspace"
    );
    Ok(path)
}

fn source_rows(root: &Path, sources: &[Value]) -> Result<()> {
    let mut paths = BTreeSet::new();
    ensure!(!sources.is_empty(), "missing mutation source inventory");
    for source in sources {
        let name = text(source, "path")?;
        ensure!(paths.insert(name), "duplicate source digest");
        let path = root.join(relative(name)?);
        let canonical = platform::paths::canonical(&path)?;
        ensure!(
            canonical.starts_with(root) && fs::symlink_metadata(&path)?.file_type().is_file(),
            "source digest refers outside the captured Source"
        );
        ensure!(
            digest(&fs::read(path)?) == text(source, "digest")?,
            "stale mutation source bytes"
        );
    }
    Ok(())
}

/// Uses the normal producer for commands and keeps imported documents outside its execution origin.
///
/// # Errors
///
/// Rejects incompatible documents, incomplete evidence, and unavailable pinned tools.
pub fn run(root: &Path, request: Request) -> Result<()> {
    match request.action {
        Action::Prepare => {
            crate::phase_evidence::run(root, "njutest environment preparation", 1, || {
                producer::prepare(root)
            })?;
        }
        Action::Inspect { document, file } => println!(
            "{}",
            serde_json::to_string_pretty(&decode(document, &fs::read(file)?)?)?
        ),
        Action::Plan => {
            crate::phase_evidence::run(root, "njutest assessment plan", 1, || {
                producer::plan(root)
            })?;
        }
        Action::Run { temperature } => measured(root, temperature)?,
    }
    Ok(())
}

fn measured(root: &Path, temperature: Temperature) -> Result<()> {
    let workload = match temperature {
        Temperature::Cold => COLD_MEASUREMENT_CASES,
        Temperature::Warm => WARM_MEASUREMENT_CASES,
    };
    crate::phase_evidence::run(root, "njutest compatibility assessment", workload, || {
        producer::measure(root, temperature)
    })
}

/// Generates the repository environment from the same pin the managed global environment renders.
///
/// # Errors
///
/// Rejects a changed upstream boundary, missing canonical data, or stale generated configuration.
pub fn generate(root: &Path, check: bool) -> Result<()> {
    let canonical = fs::read(root.join(CANONICAL_ENVIRONMENT))?;
    let destination = root.join(GENERATED_ENVIRONMENT);
    let configured: toml::Value = toml::from_str(std::str::from_utf8(&canonical)?)?;
    let platform::tool::Provision::PinnedEnvironment { key, .. } = Tool::Njutest.provision() else {
        anyhow::bail!("njutest has no named managed environment")
    };
    let expected = format!("rev:{UPSTREAM_REVISION}");
    ensure!(
        configured["tools"][key]["version"].as_str() == Some(expected.as_str()),
        "canonical pin crosses the reviewed schema boundary"
    );
    if check {
        ensure!(
            fs::read(destination)? == canonical,
            "generated njutest environment is stale"
        );
    } else if fs::read(&destination).ok().as_deref() != Some(canonical.as_slice()) {
        fs::write(destination, canonical)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &[u8] = include_bytes!("../tests/fixtures/njutest/catalog.json");
    const ENGINE: &[u8] = include_bytes!("../tests/fixtures/njutest/engine.json");
    const ASSURANCE: &[u8] = include_bytes!("../tests/fixtures/njutest/assurance.json");

    #[test]
    fn repository_and_global_environments_share_the_same_canonical_pin() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        generate(root, true)?;
        assert!(
            fs::read_to_string(root.join("home/dot_config/mise/config.njutest.toml.tmpl"))?
                .contains("partials/njutest-mise.toml")
        );
        Ok(())
    }

    #[test]
    fn original_documents_match_the_verified_vendor_identities() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let manifest: Value =
            serde_json::from_slice(&fs::read(root.join("xtask/schema/njutest/manifest.json"))?)?;
        assert_eq!(manifest["revision"], UPSTREAM_REVISION);
        for document in rows(&manifest, "documents")? {
            let bytes = fs::read(root.join(text(document, "path")?))?;
            assert_eq!(digest(&bytes), text(document, "sha256")?);
            assert_eq!(bytes.len() as u64, count(document, "bytes")?);
            assert_eq!(text(document, "git_blob")?.len(), 40);
        }
        Ok(())
    }

    #[test]
    fn upstream_documents_remain_imported_and_never_establish_native_parity() {
        for (document, bytes) in [
            (Document::Catalog, CATALOG),
            (Document::Engine, ENGINE),
            (Document::Assurance, ASSURANCE),
        ] {
            let report = decode(document, bytes).unwrap();
            assert!(matches!(report.origin, Origin::Imported));
            assert_eq!(report.native_parity, "unmeasured");
            assert_eq!(report.sealed_target, SEALED_TARGET);
            assert_eq!(report.sealed_pointer_width, 32);
            assert!(!report.zero_survivors);
        }
    }

    #[test]
    fn complete_readers_reject_missing_duplicate_and_contradictory_terminal_rows() {
        for (document, fixture, pointer, change) in [
            (Document::Catalog, CATALOG, "/mutants/1/id", Value::Null),
            (
                Document::Catalog,
                CATALOG,
                "/mutants/1/index",
                serde_json::json!(0),
            ),
            (
                Document::Engine,
                ENGINE,
                "/run/interrupted",
                serde_json::json!(true),
            ),
            (
                Document::Engine,
                ENGINE,
                "/run/exit_code",
                serde_json::json!(0),
            ),
            (
                Document::Engine,
                ENGINE,
                "/accounting/killed",
                serde_json::json!(0),
            ),
            (
                Document::Engine,
                ENGINE,
                "/accounting/executed",
                serde_json::json!(0),
            ),
            (
                Document::Engine,
                ENGINE,
                "/mutants/0/outcome",
                serde_json::json!("future-success"),
            ),
            (
                Document::Assurance,
                ASSURANCE,
                "/report/builds/0/parts/0/targets/0/status",
                serde_json::json!("failed"),
            ),
            (
                Document::Assurance,
                ASSURANCE,
                "/report/builds/0/parts/0/drift",
                serde_json::json!([]),
            ),
            (
                Document::Assurance,
                ASSURANCE,
                "/report/builds/0/parts/0/accounting/mutants/killed",
                serde_json::json!(0),
            ),
            (
                Document::Assurance,
                ASSURANCE,
                "/report/builds/0/parts/0/accounting/mutants/executed",
                serde_json::json!(0),
            ),
            (
                Document::Assurance,
                ASSURANCE,
                "/report/builds",
                serde_json::json!([]),
            ),
        ] {
            let mut value: Value = serde_json::from_slice(fixture).unwrap();
            *value.pointer_mut(pointer).unwrap() = change;
            assert!(
                decode(document, &serde_json::to_vec(&value).unwrap()).is_err(),
                "{document:?} {pointer}"
            );
        }
    }

    #[test]
    fn unknown_fields_and_historical_filenames_do_not_select_a_looser_schema() {
        for (document, fixture) in [
            (Document::Catalog, CATALOG),
            (Document::Engine, ENGINE),
            (Document::Assurance, ASSURANCE),
        ] {
            let mut value: Value = serde_json::from_slice(fixture).unwrap();
            value["invented_completion"] = Value::Bool(true);
            assert!(decode(document, &serde_json::to_vec(&value).unwrap()).is_err());
        }
        assert!(decode(Document::Catalog, ENGINE).is_err());
        assert!(decode(Document::Engine, CATALOG).is_err());
    }

    #[test]
    fn every_original_accounting_counter_is_rederived_from_complete_rows() {
        for (document, fixture, pointer) in [
            (Document::Engine, ENGINE, "/accounting"),
            (
                Document::Assurance,
                ASSURANCE,
                "/report/builds/0/parts/0/accounting/mutants",
            ),
            (
                Document::Assurance,
                ASSURANCE,
                "/report/builds/0/parts/0/accounting/mutants/observers",
            ),
        ] {
            let value: Value = serde_json::from_slice(fixture).unwrap();
            for (name, original) in value.pointer(pointer).unwrap().as_object().unwrap() {
                let Some(original) = original.as_u64() else {
                    continue;
                };
                let mut forged = value.clone();
                forged.pointer_mut(pointer).unwrap()[name] = serde_json::json!(original + 1);
                let bytes = serde_json::to_vec(&forged).unwrap();
                assert!(
                    closed_document(document, &bytes).is_ok(),
                    "{document:?} {name} must retain a valid original schema shape"
                );
                assert!(
                    decode(document, &bytes).is_err(),
                    "{document:?} {pointer}/{name}"
                );
            }
        }
    }

    #[test]
    fn source_bytes_and_paths_are_checked_without_minting_a_producer_receipt() {
        let scope = tempfile::tempdir().unwrap();
        let root = platform::paths::canonical(scope.path()).unwrap();
        fs::write(root.join("rule.rs"), b"actual captured bytes").unwrap();
        let sources =
            vec![serde_json::json!({"path":"rule.rs","digest":digest(b"actual captured bytes")})];
        source_rows(&root, &sources).unwrap();
        fs::write(root.join("rule.rs"), b"changed bytes").unwrap();
        assert!(source_rows(&root, &sources).is_err());
        assert!(source_rows(&root, &[serde_json::json!({"path":"../outside","digest":digest(b"actual captured bytes")})]).is_err());
        assert!(source_rows(&root, &[]).is_err());
    }
}
