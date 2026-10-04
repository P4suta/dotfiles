use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const HARNESSES: [&str; 6] = [
    "adapter_activation_requires_policy_and_installed_runtime",
    "composition_respects_sample_and_ratio_without_overflow",
    "dispositions_require_evidence_and_defined_follow_up",
    "incomplete_completion_is_refused_and_automatic_continuation_is_bounded",
    "immutable_report_identity_requires_exact_digest_shape",
    "stale_or_unprocessed_maintenance_never_completes",
];

pub const PR_HARNESSES: [&str; 6] = [
    "api_quota_requires_a_nonzero_reserve_and_sufficient_remaining_requests",
    "coderabbit_exclusion_does_not_authorize_unfinished_generation",
    "local_creation_always_starts_as_draft",
    "personal_issue_prerequisites_cannot_be_skipped_by_any_generation_mode",
    "ready_requires_a_validated_draft_transition",
    "rejected_documents_never_produce_a_pr_mutation",
];

pub const PROFILE_HARNESSES: [&str; 7] = [
    "only_available_profiles_are_selected",
    "application_requires_owned_destination_or_native_authorization",
    "concurrent_source_and_local_edits_are_never_overwritten",
    "secrets_require_present_nonempty_resolved_values",
    "windows_path_translation_preserves_inline_public_keys",
    "remote_or_installed_context7_never_requires_a_mutation",
    "forge_import_requires_only_pinned_keys_including_the_current_one",
];

pub const REAPER_HARNESSES: [&str; 3] = [
    "process_age_never_underflows_or_divides_by_zero",
    "running_or_reused_processes_are_never_killable",
    "tick_conversion_rejects_zero_frequency",
];

pub const REVIEW_HARNESSES: [&str; 5] = [
    "cooldown_refuses_rollback_and_every_incomplete_interval",
    "layered_capacity_is_floored_once_without_overflow",
    "owner_pause_allows_only_local_guard_status",
    "review_consumption_respects_the_layered_capacity_budget",
    "rolling_attempts_cannot_exceed_the_layered_local_budget",
];

pub fn validate_results(value: &Value, expected: &[&str], counterexample: bool) -> Result<()> {
    let summary = &value["verification_results"]["summary"];
    ensure!(
        summary["status"] == "completed"
            && summary["executed"].as_u64() == Some(expected.len() as u64)
            && !expected.is_empty(),
        "proof execution is incomplete or empty"
    );
    let results = value["verification_results"]["results"]
        .as_array()
        .context("missing proof results")?;
    let names: BTreeSet<_> = results
        .iter()
        .map(|result| {
            result["harness_id"]
                .as_str()
                .context("missing harness identity")
        })
        .collect::<Result<_>>()?;
    ensure!(
        results.len() == expected.len() && names == expected.iter().copied().collect(),
        "unexpected, missing, or duplicate proof harness"
    );
    if counterexample {
        ensure!(
            summary["failed"] == expected.len(),
            "counterexample was not rejected"
        );
        ensure!(
            results.iter().all(
                |result| result["checks"].as_array().is_some_and(|checks| checks
                    .iter()
                    .any(|check| check["category"] == "assertion" && check["status"] == "Failure"))
            ),
            "expected assertion counterexample is missing"
        );
    } else {
        ensure!(
            summary["failed"] == 0 && summary["successful"] == expected.len(),
            "required proof failed"
        );
        for result in results {
            ensure!(
                result["status"] == "Success",
                "proof was unsupported, skipped, or failed"
            );
            let checks = result["checks"]
                .as_array()
                .context("missing proof properties")?;
            ensure!(
                checks.iter().any(|check| check["category"] == "assertion"),
                "proof has no assertion"
            );
            ensure!(
                checks
                    .iter()
                    .filter(|check| check["category"] == "cover" && check["status"] == "Satisfied")
                    .count()
                    >= 2,
                "proof lacks reachable acceptance and refusal witnesses"
            );
            ensure!(
                checks.iter().all(|check| if check["category"] == "cover" {
                    check["status"] == "Satisfied"
                } else {
                    check["status"] == "Success"
                }),
                "proof property is unverified"
            );
        }
    }
    Ok(())
}

fn verify_file(
    source: &Path,
    directory: &Path,
    expected: &[&str],
    counterexample: bool,
) -> Result<()> {
    let output = directory.join("results.json");
    let mut command = Tool::Kani.command();
    command
        .current_dir(directory)
        .arg(source)
        .args([
            "--output-format",
            "terse",
            "-Z",
            "unstable-options",
            "-Z",
            "stubbing",
            "--harness-timeout",
            "60s",
            "--export-json",
        ])
        .arg(&output);
    if counterexample {
        command.args(["--harness", expected[0], "--exact"]);
    }
    let status = command.status().context("start pinned Kani verifier")?;
    ensure!(
        status.success() != counterexample,
        "Kani returned an unexpected outcome: {status}"
    );
    let result: Value = crate::skill_ops::read_json(&output)?;
    validate_results(&result, expected, counterexample)
}

fn verify_inventory(source: &Path, directory: &Path, expected: &[&str]) -> Result<()> {
    let status = Tool::Kani
        .command()
        .current_dir(directory)
        .args(["list", "--format", "json", "-Z", "stubbing"])
        .arg(source)
        .status()?;
    ensure!(status.success(), "cannot list required proof harnesses");
    let listed: Value = crate::skill_ops::read_json(&directory.join("kani-list.json"))?;
    ensure!(
        listed["kani-version"] == "0.68.0",
        "unexpected proof inventory version"
    );
    let declarations = listed["standard-harnesses"]
        .as_object()
        .context("missing standard harness inventory")?;
    let names: BTreeSet<_> = declarations
        .values()
        .flat_map(|value| value.as_array().into_iter().flatten())
        .map(|value| value.as_str().context("invalid proof inventory"))
        .collect::<Result<_>>()?;
    ensure!(
        names == expected.iter().copied().collect(),
        "required proof inventory drifted"
    );
    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    ensure!(
        cfg!(any(target_os = "macos", target_os = "linux")),
        "Kani is required on a supported Mac or Linux verification host; this native host does not provide that verifier"
    );
    let version = Tool::Kani.command().arg("--version").output()?;
    ensure!(
        version.status.success()
            && String::from_utf8(version.stdout)?.contains("Kani Rust Verifier 0.68.0"),
        "the pinned Kani version is unavailable"
    );
    let source = crate::canonical(&root.join("xtask/src/skill_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &HARNESSES)?;
    verify_file(&source, directory.path(), &HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_completion_without_evidence() {{\n    assert!(production::decision_complete(production::Outcome::Implemented, true, false, false));\n}}\n"
        ),
    )?;
    verify_file(
        &file,
        probe.path(),
        &["reject_completion_without_evidence"],
        true,
    )?;
    ensure!(
        fs::read(source)? == original,
        "production proof source changed during verification"
    );
    let source = crate::canonical(&root.join("xtask/src/pr_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &PR_HARNESSES)?;
    verify_file(&source, directory.path(), &PR_HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_ready_local_creation() {{\n    assert_eq!(production::plan(production::Operation::Create, production::Generation::Local, true, false, production::IssueGate::Linked), Some(production::Effect::CreateReady));\n}}\n#[kani::proof]\nfn reject_creation_without_a_personal_issue() {{\n    assert!(production::plan(production::Operation::Create, production::Generation::Coderabbit, true, false, production::issue_gate(true, false, false)).is_some());\n}}\n#[kani::proof]\nfn reject_unfinished_summary_as_review_exclusion() {{\n    assert!(production::coderabbit_body_marker_allowed(false, true, false));\n}}\n"
        ),
    )?;
    for name in [
        "reject_ready_local_creation",
        "reject_creation_without_a_personal_issue",
        "reject_unfinished_summary_as_review_exclusion",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
    ensure!(
        fs::read(source)? == original,
        "production PR proof source changed during verification"
    );
    let source = crate::canonical(&root.join("xtask/src/profile_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &PROFILE_HARNESSES)?;
    verify_file(&source, directory.path(), &PROFILE_HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_unapproved_application() {{\n    assert!(production::permitted(production::Mode::Full, true, true, false));\n}}\n"
        ),
    )?;
    verify_file(
        &file,
        probe.path(),
        &["reject_unapproved_application"],
        true,
    )?;
    ensure!(
        fs::read(source)? == original,
        "production profile proof source changed during verification"
    );
    let source = crate::canonical(&root.join("guard/src/reaper_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &REAPER_HARNESSES)?;
    verify_file(&source, directory.path(), &REAPER_HARNESSES, false)?;
    ensure!(
        fs::read(source)? == original,
        "production reaper proof source changed during verification"
    );
    let source = crate::canonical(&root.join("xtask/src/review_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &REVIEW_HARNESSES)?;
    verify_file(&source, directory.path(), &REVIEW_HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_eighth_review_of_ten() {{\n    assert!(production::included_allowance(10, 3));\n}}\n#[kani::proof]\nfn reject_rounding_the_layered_budget_up() {{\n    assert_eq!(production::capacity_budget(49), 36);\n}}\n#[kani::proof]\nfn reject_eighth_local_attempt() {{\n    assert!(production::rolling_allowance(7, 7));\n}}\n#[kani::proof]\nfn reject_early_review() {{\n    assert!(production::cooldown_complete(100, 101, production::PROBE_GAP_MS));\n}}\n#[kani::proof]\nfn reject_owner_pause_bypass() {{\n    assert!(production::service_access(true, false));\n}}\n"
        ),
    )?;
    for name in [
        "reject_eighth_review_of_ten",
        "reject_rounding_the_layered_budget_up",
        "reject_eighth_local_attempt",
        "reject_early_review",
        "reject_owner_pause_bypass",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
    ensure!(
        fs::read(source)? == original,
        "production review proof source changed during verification"
    );
    println!(
        "Verified all {} production policy harnesses, reachable outcomes, and the rejecting counterexamples",
        HARNESSES.len()
            + PR_HARNESSES.len()
            + PROFILE_HARNESSES.len()
            + REAPER_HARNESSES.len()
            + REVIEW_HARNESSES.len()
    );
    Ok(())
}
