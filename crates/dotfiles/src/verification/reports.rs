use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::plan::{Check, Method, Report, Scope};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResultSet {
    pub subjects: Vec<String>,
    pub passed: bool,
}

fn libtest(check: &Check, exit: i32, text: &str, red: bool) -> Result<ResultSet> {
    let mut results = BTreeMap::new();
    for line in text.lines() {
        let Some((name, outcome)) = line
            .strip_prefix("test ")
            .and_then(|value| value.rsplit_once(" ... "))
        else {
            continue;
        };
        if matches!(outcome, "ok" | "FAILED" | "ignored") {
            ensure!(
                results.insert(name, outcome).is_none(),
                "ambiguous repeated test identity {name}"
            );
        }
    }
    ensure!(
        !results.is_empty(),
        "no completed behavioral tests; compiler and infrastructure errors do not establish red"
    );
    let expected: BTreeSet<_> = check.expected.iter().map(String::as_str).collect();
    ensure!(
        expected.is_subset(&results.keys().copied().collect::<BTreeSet<_>>()),
        "completed test inventory differs from the declared contract"
    );
    ensure!(
        !results.values().any(|value| *value == "ignored"),
        "an applicable test was ignored"
    );
    let failed = results.values().any(|value| *value == "FAILED");
    ensure!(
        if red {
            exit != 0 && failed
        } else {
            exit == 0 && !failed
        },
        "test outcome does not establish the requested red or green phase"
    );
    Ok(ResultSet {
        subjects: check.expected.clone(),
        passed: !red,
    })
}

fn kani(check: &Check, exit: i32, text: &str, root: &Path) -> Result<ResultSet> {
    ensure!(exit == 0, "proof command failed or stopped inconclusively");
    let references = super::proof::references(text)?;
    if !references.is_empty() {
        super::proof::validate(check, &references, root)?;
        return Ok(ResultSet {
            subjects: check.expected.clone(),
            passed: true,
        });
    }
    let expected = format!(
        "Verified all {} production policy harnesses with their reachable outcomes, and rejected all {} counterexamples",
        check.expected.len(),
        check.rejecting.len()
    );
    ensure!(
        text.lines().any(|line| line.trim() == expected),
        "source-bound Kani runner did not complete its expected proof and rejecting inventories"
    );
    Ok(ResultSet {
        subjects: check.expected.clone(),
        passed: true,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Structured {
    version: u8,
    source: String,
    plan: String,
    method: Method,
    scope: Scope,
    tool_version: String,
    target: String,
    bounds: String,
    results: Vec<Subject>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    name: String,
    status: Status,
    executed: u64,
    failed: u64,
    skipped: u64,
    inconclusive: u64,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Passed,
    Failed,
    Skipped,
    Inconclusive,
    Unsupported,
}

fn structured(
    check: &Check,
    exit: i32,
    text: &str,
    source: &str,
    plan: &str,
    red: bool,
) -> Result<ResultSet> {
    let report: Structured =
        serde_json::from_str(text).context("parse verification producer report")?;
    ensure!(
        (if red { exit != 0 } else { exit == 0 })
            && report.version == super::VERSION
            && report.source == source
            && report.plan == plan
            && report.method == check.method
            && report.scope == check.scope,
        "verification producer identity or result differs"
    );
    ensure!(
        !report.tool_version.trim().is_empty()
            && !report.target.trim().is_empty()
            && !report.bounds.trim().is_empty(),
        "verification producer omits its version, target, or bounds"
    );
    ensure!(
        report.results.iter().all(|item| item.executed > 0
            && item.failed <= item.executed
            && item.skipped == 0
            && item.inconclusive == 0
            && if red && item.failed > 0 {
                item.status == Status::Failed
            } else {
                item.status == Status::Passed && item.failed == 0
            }),
        "verification producer has an incomplete, unsupported, or failing result"
    );
    ensure!(
        !red || report.results.iter().any(|item| item.failed > 0),
        "structured red has no observed behavioral failure"
    );
    let subjects: BTreeSet<_> = report
        .results
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    ensure!(
        subjects.len() == report.results.len()
            && subjects == check.expected.iter().map(String::as_str).collect(),
        "verification producer inventory is empty, incomplete, duplicated, or unexpected"
    );
    Ok(ResultSet {
        subjects: check.expected.clone(),
        passed: !red,
    })
}

pub(super) fn validate(
    check: &Check,
    exit: i32,
    stdout: &str,
    source: &str,
    plan: &str,
    root: &Path,
    red: bool,
) -> Result<ResultSet> {
    ensure!(
        !red || matches!(check.report, Report::Libtest | Report::Structured),
        "TDD red requires completed behavioral test failures"
    );
    match check.report {
        Report::Libtest => libtest(check, exit, stdout, red),
        Report::Kani => kani(check, exit, stdout, root),
        Report::Structured => structured(check, exit, stdout, source, plan, red),
        Report::Mutants => {
            let path = root.join(
                check
                    .report_path
                    .as_deref()
                    .context("missing mutation report path")?,
            );
            super::mutation::validate(
                &path,
                exit,
                &check.expected,
                check
                    .verifier_version
                    .as_deref()
                    .context("missing mutation verifier version")?,
            )?;
            Ok(ResultSet {
                subjects: check.expected.clone(),
                passed: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn proof_fixture() -> Result<(tempfile::TempDir, Check, String)> {
        let root = tempfile::tempdir()?;
        let mut check = check();
        check.method = Method::Proof;
        check.report = Report::Kani;
        check.expected = vec!["keeps_bounds".into()];
        check.rejecting = vec!["rejects_false".into()];
        let mut output = String::new();
        let directory = root.path().join("verification/results");
        std::fs::create_dir_all(&directory)?;
        for (name, hash, exit, stdout) in [
            (
                "positive-proof",
                "a".repeat(64),
                0,
                "Checking harness proofs::keeps_bounds...\nVERIFICATION RESULT:\n ** 0 of 4 failed (1 unreachable)\nVERIFICATION:- SUCCESSFUL\nComplete - 1 successfully verified harnesses, 0 failures, 1 total.\n",
            ),
            (
                "negative-control",
                "b".repeat(64),
                1,
                "Checking harness rejects_false...\nVERIFICATION RESULT:\n ** 1 of 1 failed\nFailed Checks: assertion failed: false\nVERIFICATION:- FAILED\nVerification failed for - rejects_false\nComplete - 0 successfully verified harnesses, 1 failures, 1 total.\n",
            ),
        ] {
            writeln!(
                output,
                "phase=verification name={name} args=[\"kani\"] source_sha256={hash}"
            )?;
            let receipt = serde_json::json!({"version":3,"command":["cargo","kani"],"source_sha256":hash,"artifact_sha256":null,"container_image":format!("sha256:{}", "c".repeat(64)),"exit":exit,"stdout":stdout,"stderr":""});
            std::fs::write(
                directory.join(format!("{name}.json")),
                serde_json::to_vec(&receipt)?,
            )?;
        }
        Ok((root, check, output))
    }

    #[test]
    fn verification_proof_receipts_cover_native_harnesses() -> Result<()> {
        let (root, check, output) = proof_fixture()?;
        validate(&check, 0, &output, "source", "plan", root.path(), false)?;
        Ok(())
    }

    #[test]
    fn verification_proof_receipts_reject_missing_changed_failed_and_unrelated_results()
    -> Result<()> {
        let (root, check, output) = proof_fixture()?;
        validate(&check, 0, &output, "source", "plan", root.path(), false)?;
        let path = root.path().join("verification/results/positive-proof.json");
        let original: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        for (field, replacement) in [
            ("version", 2.into()),
            ("source_sha256", "d".repeat(64).into()),
            ("container_image", "moving-tag:latest".into()),
            ("artifact_sha256", "e".repeat(64).into()),
            (
                "command",
                serde_json::json!(["cargo", "kani", "--harness", "other"]),
            ),
            ("exit", 1.into()),
            ("exit", serde_json::Value::Null),
        ] {
            let mut changed = original.clone();
            changed[field] = replacement;
            std::fs::write(&path, serde_json::to_vec(&changed)?)?;
            assert!(
                validate(&check, 0, &output, "source", "plan", root.path(), false).is_err(),
                "{field}"
            );
        }
        let original_stdout = original["stdout"].as_str().unwrap();
        for (from, to) in [
            ("proofs::keeps_bounds", "proofs::unrelated"),
            ("0 of 4 failed (1 unreachable)", "0 of 0 failed"),
            (
                "0 of 4 failed (1 unreachable)",
                "0 of 4 failed (4 unreachable)",
            ),
            (
                "0 of 4 failed (1 unreachable)",
                "1 of 4 failed (1 unreachable)",
            ),
            (" ** 0 of 4 failed (1 unreachable)\n", ""),
            ("1 total.", "2 total."),
            ("Complete - 1", "Complete - 18446744073709551615"),
            ("VERIFICATION:- SUCCESSFUL", "VERIFICATION:- FAILED"),
        ] {
            let mut changed = original.clone();
            changed["stdout"] = original_stdout.replace(from, to).into();
            std::fs::write(&path, serde_json::to_vec(&changed)?)?;
            assert!(
                validate(&check, 0, &output, "source", "plan", root.path(), false).is_err(),
                "{to}"
            );
        }
        std::fs::write(&path, serde_json::to_vec(&original)?)?;
        for changed in [
            output.replace("name=positive-proof", "name=../positive-proof"),
            format!("{output}{output}"),
            output.replace("args=[\"kani\"]", "args=[execute()]"),
        ] {
            assert!(validate(&check, 0, &changed, "source", "plan", root.path(), false).is_err());
        }
        std::fs::remove_file(&path)?;
        assert!(validate(&check, 0, &output, "source", "plan", root.path(), false).is_err());
        Ok(())
    }

    fn check() -> Check {
        Check {
            id: "tests".into(),
            method: Method::Test,
            tool: super::super::plan::Tool::Cargo,
            arguments: vec!["test".into()],
            report: Report::Libtest,
            expected: vec!["contract".into()],
            rejecting: vec![],
            report_path: None,
            verifier_version: None,
            environment: vec![],
            scope: Scope::Affected,
        }
    }

    #[test]
    fn compilation_errors_skips_and_empty_runs_do_not_establish_behavior() {
        let check = check();
        assert!(libtest(&check, 101, "error[E0308]: mismatched types", true).is_err());
        assert!(libtest(&check, 0, "running 0 tests", false).is_err());
        assert!(libtest(&check, 0, "test contract ... ignored", false).is_err());
        assert!(libtest(&check, 0, "test contract ... ok", true).is_err());
        assert!(libtest(&check, 101, "test contract ... FAILED", true).is_ok());
        assert!(libtest(&check, 0, "test contract ... ok", false).is_ok());
    }

    #[test]
    fn structured_results_need_current_identity_and_nonempty_completed_work() -> Result<()> {
        let mut report = serde_json::json!({"version": 1, "source": "fixture", "plan": "fixture", "method": "test", "scope": "affected", "tool_version": "fixture", "target": "fixture", "bounds": "one test", "results": [{"name": "contract", "status": "passed", "executed": 1, "failed": 0, "skipped": 0, "inconclusive": 0}]});
        let inspect = |value: &serde_json::Value, red| -> Result<()> {
            super::super::inspect_producer_report(&serde_json::to_string(value)?, red)
        };
        inspect(&report, false)?;
        assert!(inspect(&report, true).is_err());
        for field in ["failed", "skipped", "inconclusive"] {
            report["results"][0][field] = 1.into();
            assert!(inspect(&report, false).is_err());
            report["results"][0][field] = 0.into();
        }
        report["results"][0]["executed"] = 0.into();
        assert!(inspect(&report, false).is_err());
        report["results"][0]["executed"] = 1.into();
        report["results"][0]["failed"] = 1.into();
        report["results"][0]["status"] = "failed".into();
        inspect(&report, true)?;
        report["source"] = "another-source".into();
        assert!(inspect(&report, true).is_err());
        Ok(())
    }
}
