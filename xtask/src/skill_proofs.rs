use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Stdio;

pub const KANI_VERSION: &str = "0.68.0";

pub const HARNESSES: [&str; 12] = [
    "adapter_activation_requires_policy_and_installed_runtime",
    "composition_respects_sample_and_ratio_without_overflow",
    "dispositions_require_evidence_and_defined_follow_up",
    "incomplete_completion_is_refused_and_automatic_continuation_is_bounded",
    "immutable_report_identity_requires_exact_digest_shape",
    "only_current_settled_decisions_are_adopted",
    "oversized_skills_need_a_complete_size_disposition",
    "stale_or_unprocessed_maintenance_never_completes",
    "the_decision_diff_starts_only_from_the_assessed_content",
    "the_gate_completes_only_when_nothing_remains_to_triage",
    "tracked_decisions_pass_only_when_present_bound_and_complete",
    "triage_is_skipped_only_when_every_finding_is_adopted",
];

pub const PR_HARNESSES: [&str; 7] = [
    "api_quota_requires_a_nonzero_reserve_and_sufficient_remaining_requests",
    "coderabbit_exclusion_does_not_authorize_unfinished_generation",
    "local_creation_always_starts_as_draft",
    "personal_issue_prerequisites_cannot_be_skipped_by_any_generation_mode",
    "ready_requires_a_validated_draft_transition",
    "ready_requires_every_reported_check_to_pass",
    "rejected_documents_never_produce_a_pr_mutation",
];

pub const PROFILE_HARNESSES: [&str; 9] = [
    "only_available_profiles_are_selected",
    "application_requires_owned_destination_or_native_authorization",
    "concurrent_source_and_local_edits_are_never_overwritten",
    "secrets_require_present_nonempty_resolved_values",
    "windows_path_translation_preserves_inline_public_keys",
    "remote_or_installed_context7_never_requires_a_mutation",
    "forge_import_requires_only_pinned_keys_including_the_current_one",
    "memory_is_off_only_when_every_switch_is_explicitly_disabled",
    "host_edits_are_overwritten_only_by_a_merge_that_keeps_every_host_key",
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
        listed["kani-version"] == KANI_VERSION,
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

fn short_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn container_image(definition: &[u8]) -> String {
    format!("dotfiles-proofs:{}", short_digest(definition))
}

pub fn container_target_volume(root: &Path) -> String {
    format!(
        "dotfiles-proofs-target-{}",
        short_digest(root.as_os_str().as_encoded_bytes())
    )
}

/// Kani writes temporary files beside the source it compiles, so the gate runs on a container-local copy of the read-only checkout.
const CONTAINER_GATE: &str = "set -o pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -; exec cargo run --locked --manifest-path xtask/Cargo.toml -- proofs";

pub fn container_run_arguments(root: &Path, image: &str, target_volume: &str) -> Vec<OsString> {
    // Docker reads `--mount` as one CSV record; quoting keeps a comma in the path inside the field, and Windows paths cannot contain the quote itself.
    let mut mount = OsString::from("type=bind,\"source=");
    mount.push(root);
    mount.push("\",target=/source,readonly");
    let mut arguments: Vec<OsString> = ["run", "--rm", "--mount"].map(OsString::from).into();
    arguments.push(mount);
    arguments.extend(
        [
            "--volume",
            &format!("{target_volume}:/cache"),
            "--volume",
            "dotfiles-proofs-registry:/usr/local/cargo/registry",
            "--env",
            "CARGO_TARGET_DIR=/cache/target",
            "--workdir",
            "/work",
            image,
            "bash",
            "-ec",
            CONTAINER_GATE,
        ]
        .map(OsString::from),
    );
    arguments
}

/// Kani has no native build here, so the unchanged proof gate runs on a pinned Linux verification host that sees this checkout read-only.
fn verify_in_container(root: &Path) -> Result<()> {
    let root = crate::canonical(root)?;
    let engine = Tool::Docker
        .command()
        .args(["version", "--format", "{{.Server.Os}}"])
        .stderr(Stdio::null())
        .output()
        .context("Kani proofs on this host run in a Linux container; install Docker")?;
    ensure!(
        engine.status.success() && String::from_utf8(engine.stdout)?.trim() == "linux",
        "Kani proofs on this host need a running Docker engine with Linux containers"
    );
    let definition = root.join("xtask/proofs/Dockerfile");
    let image = container_image(&fs::read(&definition)?);
    let present = Tool::Docker
        .command()
        .args(["image", "inspect"])
        .arg(&image)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !present.success() {
        let status = Tool::Docker
            .command()
            .args(["build", "--tag"])
            .arg(&image)
            .arg("--file")
            .arg(&definition)
            .arg(root.join("xtask/proofs"))
            .status()
            .context("build the pinned Kani verification image")?;
        ensure!(
            status.success(),
            "cannot build the pinned Kani verification image: {status}"
        );
    }
    let status = Tool::Docker
        .command()
        .args(container_run_arguments(
            &root,
            &image,
            &container_target_volume(&root),
        ))
        .status()
        .context("start the Kani verification container")?;
    ensure!(
        status.success(),
        "the containerized proof gate failed: {status}"
    );
    Ok(())
}

pub fn verify(root: &Path) -> Result<()> {
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        verify_native(root)
    } else {
        verify_in_container(root)
    }
}

fn verify_native(root: &Path) -> Result<()> {
    let version = Tool::Kani.command().arg("--version").output()?;
    ensure!(
        version.status.success()
            && String::from_utf8(version.stdout)?
                .contains(&format!("Kani Rust Verifier {KANI_VERSION}")),
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
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_completion_without_evidence() {{\n    assert!(production::decision_complete(production::Outcome::Implemented, true, false, false));\n}}\n#[kani::proof]\nfn reject_adopting_a_changed_skill() {{\n    assert!(production::adopted(false, production::Outcome::Keep));\n}}\n#[kani::proof]\nfn reject_a_stale_tracked_decision() {{\n    assert!(production::tracked_decision(true, false, true) == production::Tracked::Current);\n}}\n#[kani::proof]\nfn reject_skipping_triage_with_open_findings() {{\n    assert!(production::triage_complete(1, false));\n}}\n#[kani::proof]\nfn reject_an_unrecorded_oversized_skill() {{\n    assert!(production::size_disposition(true, false, false) == production::Sizing::Settled);\n}}\n#[kani::proof]\nfn reject_completing_the_gate_with_open_findings() {{\n    assert!(production::gate_complete(false, 1));\n}}\n#[kani::proof]\nfn reject_a_diff_from_unassessed_content() {{\n    assert!(production::diff_base(true, false) == production::DiffBase::Assessed);\n}}\n"
        ),
    )?;
    for name in [
        "reject_completion_without_evidence",
        "reject_adopting_a_changed_skill",
        "reject_a_stale_tracked_decision",
        "reject_skipping_triage_with_open_findings",
        "reject_an_unrecorded_oversized_skill",
        "reject_completing_the_gate_with_open_findings",
        "reject_a_diff_from_unassessed_content",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
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
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_ready_local_creation() {{\n    assert_eq!(production::plan(production::Operation::Create, production::Generation::Local, true, false, production::IssueGate::Linked, production::Checks::Passing), Some(production::Effect::CreateReady));\n}}\n#[kani::proof]\nfn reject_creation_without_a_personal_issue() {{\n    assert!(production::plan(production::Operation::Create, production::Generation::Coderabbit, true, false, production::issue_gate(true, false, false), production::Checks::Passing).is_some());\n}}\n#[kani::proof]\nfn reject_unfinished_summary_as_review_exclusion() {{\n    assert!(production::coderabbit_body_marker_allowed(false, true, false));\n}}\n#[kani::proof]\nfn reject_ready_with_unfinished_checks() {{\n    assert_eq!(production::plan(production::Operation::Ready, production::Generation::Local, true, true, production::IssueGate::Linked, production::checks_state(3, 1, 0)), Some(production::Effect::Ready));\n}}\n"
        ),
    )?;
    for name in [
        "reject_ready_local_creation",
        "reject_creation_without_a_personal_issue",
        "reject_unfinished_summary_as_review_exclusion",
        "reject_ready_with_unfinished_checks",
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
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_unapproved_application() {{\n    assert!(production::permitted(production::Mode::Full, true, true, false));\n}}\n#[kani::proof]\nfn reject_an_unset_memory_switch() {{\n    assert!(production::memory_off(&[production::Memory::Disabled, production::Memory::Unset]));\n}}\n#[kani::proof]\nfn reject_overwriting_a_merge_that_drops_a_host_key() {{\n    assert!(!production::host_edit_refused(true, true, false));\n}}\n"
        ),
    )?;
    for name in [
        "reject_unapproved_application",
        "reject_an_unset_memory_switch",
        "reject_overwriting_a_merge_that_drops_a_host_key",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
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
