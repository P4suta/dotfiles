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

pub const PR_HARNESSES: [&str; 16] = [
    "a_behind_stacked_pr_is_restacked_and_an_independent_one_updated_by_merge",
    "a_branch_sharing_work_with_another_open_pr_opens_only_on_top_of_it",
    "a_draft_becomes_ready_during_the_pause_only_with_the_pause_lines",
    "a_paused_edit_never_removes_the_exclusion_of_a_ready_pr",
    "a_pr_merges_only_ready_current_passing_clean_and_without_an_owed_review",
    "a_pr_ready_during_the_pause_keeps_its_pause_exclusion",
    "a_refusal_names_the_first_unmet_condition",
    "api_quota_requires_a_nonzero_reserve_and_sufficient_remaining_requests",
    "coderabbit_exclusion_does_not_authorize_unfinished_generation",
    "local_creation_always_starts_as_draft",
    "paused_pr_reviews_are_never_requested",
    "personal_issue_prerequisites_cannot_be_skipped_by_any_generation_mode",
    "pr_review_requirement_returns_after_resumption",
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

pub const GATE_HARNESSES: [&str; 7] = [
    "a_commit_retry_keeps_what_its_message_source_implied",
    "a_long_option_is_its_exact_spelling_or_its_only_completion",
    "a_history_refusal_suggests_the_step_for_its_worst_update",
    "a_published_rewrite_runs_only_through_the_leased_stack_tooling_or_a_waiver",
    "a_rebase_is_refused_whenever_it_may_rewrite_a_published_commit",
    "a_rebase_signs_one_linear_branch_and_an_amend_only_the_checked_out_tip",
    "a_suggestion_is_rebuilt_only_from_a_line_read_in_full",
];

pub const REVIEW_HARNESSES: [&str; 6] = [
    "cooldown_refuses_rollback_and_every_incomplete_interval",
    "layered_capacity_is_floored_once_without_overflow",
    "owner_pause_allows_only_local_guard_status",
    "pause_scope_is_the_union_of_its_markers",
    "review_consumption_respects_the_layered_capacity_budget",
    "rolling_attempts_cannot_exceed_the_layered_local_budget",
];

pub const INSTRUCTION_HARNESSES: [&str; 3] = [
    "every_line_needs_the_class_of_its_shape",
    "follow_ups_are_defined_once_and_referenced",
    "removed_lines_name_what_holds_them",
];

pub const NEXT_ACTION_HARNESSES: [&str; 5] = [
    "executed_steps_run_only_on_their_own_preconditions",
    "host_prerequisites_precede_every_repository_step",
    "owner_pauses_are_never_bypassed",
    "publication_requires_committed_decided_current_work",
    "upstream_commits_the_branch_never_held_are_never_overwritten",
];

pub const CHANGE_HARNESSES: [&str; 2] = [
    "ci_tooling_lock_files_and_the_classifier_select_everything",
    "every_path_selects_a_gate_and_unknown_paths_select_all",
];

pub const LINE_ENDING_HARNESSES: [&str; 3] = [
    "auto_detection_follows_git_byte_classes",
    "conversion_removes_only_carriage_returns_before_line_feeds",
    "declared_line_endings_are_never_rewritten",
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
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_ready_local_creation() {{\n    assert_eq!(production::plan(production::Operation::Create, production::Generation::Local, true, false, production::IssueGate::Linked, production::Checks::Passing), Ok(production::Effect::CreateReady));\n}}\n#[kani::proof]\nfn reject_creation_without_a_personal_issue() {{\n    assert!(production::plan(production::Operation::Create, production::Generation::Coderabbit, true, false, production::issue_gate(true, false, false), production::Checks::Passing).is_ok());\n}}\n#[kani::proof]\nfn reject_unfinished_summary_as_review_exclusion() {{\n    assert!(production::coderabbit_body_marker_allowed(false, true, false));\n}}\n#[kani::proof]\nfn reject_ready_with_unfinished_checks() {{\n    assert_eq!(production::plan(production::Operation::Ready, production::Generation::Local, true, true, production::IssueGate::Linked, production::checks_state(3, 1, 0)), Ok(production::Effect::Ready));\n}}\n#[kani::proof]\nfn reject_blaming_checks_for_a_published_pr() {{\n    assert_eq!(production::plan(production::Operation::Ready, production::Generation::Local, true, false, production::IssueGate::Linked, production::Checks::Incomplete), Err(production::Blocker::UnfinishedChecks));\n}}\n#[kani::proof]\nfn reject_paused_review_request() {{\n    assert!(production::review_request_allowed(true, production::requests_review(production::Operation::Ready, production::Generation::Local, true, false, false)));\n}}\n#[kani::proof]\nfn reject_paused_exclusion_removal() {{\n    assert!(production::review_request_allowed(true, production::requests_review(production::Operation::Edit, production::Generation::Local, false, true, false)));\n}}\n#[kani::proof]\nfn reject_pause_exclusion_as_standing() {{\n    assert_eq!(production::review_requirement(false, production::exclusion(true, true)), production::ReviewRequirement::Excluded);\n}}\n#[kani::proof]\nfn reject_standing_exclusion_when_ready_during_the_pause() {{\n    assert!(production::keeps_pause_exclusion(production::Operation::Ready, true, production::Exclusion::None, production::exclusion(true, false)));\n}}\nfn state(lifecycle: production::Lifecycle, behind: bool, checks: production::HeadChecks, exclusion: production::Exclusion) -> production::MergeState {{\n    production::MergeState {{ lifecycle, conflict: false, behind, checks, clean: true, exclusion }}\n}}\n#[kani::proof]\nfn reject_merging_a_behind_pr() {{\n    assert!(production::merge_step(state(production::Lifecycle::Ready, true, production::HeadChecks::Passing, production::Exclusion::Pause), false, true) == production::Step::Merge);\n}}\n#[kani::proof]\nfn reject_updating_a_stacked_pr_by_merge() {{\n    assert!(production::merge_step(state(production::Lifecycle::Ready, true, production::HeadChecks::Passing, production::Exclusion::Pause), true, true) == production::Step::Update);\n}}\n#[kani::proof]\nfn reject_merging_with_a_failed_check() {{\n    assert!(production::merge_step(state(production::Lifecycle::Ready, false, production::HeadChecks::Failed, production::Exclusion::Pause), false, true) == production::Step::Merge);\n}}\n#[kani::proof]\nfn reject_readying_without_the_pause_lines() {{\n    assert!(production::merge_step(state(production::Lifecycle::Draft, false, production::HeadChecks::Passing, production::Exclusion::None), false, true) == production::Step::Ready);\n}}\n#[kani::proof]\nfn reject_opening_a_dependent_branch_independently() {{\n    assert!(production::dependency(true, false, true).is_none());\n}}\n"
        ),
    )?;
    for name in [
        "reject_ready_local_creation",
        "reject_creation_without_a_personal_issue",
        "reject_unfinished_summary_as_review_exclusion",
        "reject_ready_with_unfinished_checks",
        "reject_blaming_checks_for_a_published_pr",
        "reject_paused_review_request",
        "reject_paused_exclusion_removal",
        "reject_pause_exclusion_as_standing",
        "reject_standing_exclusion_when_ready_during_the_pause",
        "reject_merging_a_behind_pr",
        "reject_updating_a_stacked_pr_by_merge",
        "reject_merging_with_a_failed_check",
        "reject_readying_without_the_pause_lines",
        "reject_opening_a_dependent_branch_independently",
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
    let source = crate::canonical(&root.join("guard/src/gate_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &GATE_HARNESSES)?;
    verify_file(&source, directory.path(), &GATE_HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_rebasing_across_several_refs() {{\n    assert!(production::signing(2, production::Reference::Branch, false, false) == production::Signing::Rebase);\n}}\n#[kani::proof]\nfn reject_rebasing_a_range_with_merges() {{\n    assert!(production::signing(1, production::Reference::Branch, true, false) == production::Signing::Rebase);\n}}\n#[kani::proof]\nfn reject_amending_a_ref_that_is_not_checked_out() {{\n    assert!(production::signing(1, production::Reference::Other, false, true) == production::Signing::Amend);\n}}\n#[kani::proof]\nfn reject_deleting_through_the_forge_before_fetching() {{\n    assert!(production::history(true, false) == production::History::ForgeDelete);\n}}\n#[kani::proof]\nfn reject_completing_a_prefix_of_two_options() {{\n    assert!(production::spelling(b\"a\", &[&b\"ab\"[..], &b\"ac\"[..]]) == production::Spelling::Is(0));\n}}\n#[kani::proof]\nfn reject_completing_over_an_exact_spelling() {{\n    assert!(production::spelling(b\"ab\", &[&b\"abc\"[..], &b\"ab\"[..]]) == production::Spelling::Is(0));\n}}\n#[kani::proof]\nfn reject_ignoring_a_unique_abbreviation() {{\n    assert!(production::spelling(b\"re\", &[&b\"repo\"[..], &b\"force\"[..]]) == production::Spelling::Unknown);\n}}\n#[kani::proof]\nfn reject_suggesting_from_an_unread_line() {{\n    assert!(production::verdict(true, false) == production::Verdict::Suggest);\n}}\n#[kani::proof]\nfn reject_retrying_a_reword_without_what_it_implied() {{\n    assert!(production::commit_retry(production::Source {{ fixup: production::Fixup::Reword, reuse: false, renew: false, amend: false }}, Some(false), false) == Ok(production::Retry {{ allow_empty: false, only: false, authorship: false, reset_author: true }}));\n}}\n#[kani::proof]\nfn reject_retrying_without_the_reused_authorship() {{\n    assert!(production::commit_retry(production::Source {{ fixup: production::Fixup::Absent, reuse: true, renew: false, amend: false }}, Some(false), false).is_ok());\n}}\n#[kani::proof]\nfn reject_keeping_a_reset_author_without_its_reuse() {{\n    assert!(production::commit_retry(production::Source {{ fixup: production::Fixup::Absent, reuse: true, renew: true, amend: false }}, Some(false), true).is_ok_and(|retry| retry.reset_author));\n}}\n#[kani::proof]\nfn reject_the_reused_authorship_during_a_pick() {{\n    assert!(production::commit_retry(production::Source {{ fixup: production::Fixup::Absent, reuse: true, renew: false, amend: false }}, Some(true), true).is_ok_and(|retry| retry.authorship));\n}}\n#[kani::proof]\nfn reject_retrying_a_reuse_without_knowing_of_a_pick() {{\n    assert!(production::commit_retry(production::Source {{ fixup: production::Fixup::Absent, reuse: true, renew: true, amend: false }}, None, true).is_ok());\n}}\n#[kani::proof]\nfn reject_a_manual_rebase_of_a_published_branch() {{\n    assert!(!production::rebase_refused(false, Some(true)));\n}}\n#[kani::proof]\nfn reject_an_unleased_stack_marker() {{\n    assert!(production::stack_tooling(Some(b\"t\"), None));\n}}\n#[kani::proof]\nfn reject_the_lease_admitting_a_plain_force_push() {{\n    assert!(production::admission(false, true, false) == production::Admission::Tooling);\n}}\n"
        ),
    )?;
    for name in [
        "reject_rebasing_across_several_refs",
        "reject_rebasing_a_range_with_merges",
        "reject_amending_a_ref_that_is_not_checked_out",
        "reject_deleting_through_the_forge_before_fetching",
        "reject_completing_a_prefix_of_two_options",
        "reject_completing_over_an_exact_spelling",
        "reject_ignoring_a_unique_abbreviation",
        "reject_suggesting_from_an_unread_line",
        "reject_retrying_a_reword_without_what_it_implied",
        "reject_retrying_without_the_reused_authorship",
        "reject_keeping_a_reset_author_without_its_reuse",
        "reject_the_reused_authorship_during_a_pick",
        "reject_retrying_a_reuse_without_knowing_of_a_pick",
        "reject_a_manual_rebase_of_a_published_branch",
        "reject_an_unleased_stack_marker",
        "reject_the_lease_admitting_a_plain_force_push",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
    ensure!(
        fs::read(source)? == original,
        "production gate proof source changed during verification"
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
            "#[path = {source:?}]\nmod production;\n#[kani::proof]\nfn reject_eighth_review_of_ten() {{\n    assert!(production::included_allowance(10, 3));\n}}\n#[kani::proof]\nfn reject_rounding_the_layered_budget_up() {{\n    assert_eq!(production::capacity_budget(49), 36);\n}}\n#[kani::proof]\nfn reject_eighth_local_attempt() {{\n    assert!(production::rolling_allowance(7, 7));\n}}\n#[kani::proof]\nfn reject_early_review() {{\n    assert!(production::cooldown_complete(100, 101, production::PROBE_GAP_MS));\n}}\n#[kani::proof]\nfn reject_owner_pause_bypass() {{\n    assert!(production::service_access(production::pause_scope(false, false, true), false));\n}}\n"
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
    let source = crate::canonical(&root.join("xtask/src/next_action_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &NEXT_ACTION_HARNESSES)?;
    verify_file(&source, directory.path(), &NEXT_ACTION_HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]\nmod production;\nuse production::*;\n#[kani::proof]\nfn reject_executing_a_push() {{\n    assert!(executable(Step::Push));\n}}\n#[kani::proof]\nfn reject_pushing_while_paused() {{\n    assert!(decide(State {{ host: Host::Ready, head: Head::Feature, dirty: false, undecided: false, incomplete: false, behind_base: false, ahead_base: true, upstream: Upstream::Ahead, push_paused: true, pr: Pr::Draft, checks: Checks::Passing, workflows: true, review_paused: false }}) == Step::Push);\n}}\n#[kani::proof]\nfn reject_pushing_to_a_ready_pr_while_review_is_paused() {{\n    assert!(decide(State {{ host: Host::Ready, head: Head::Feature, dirty: false, undecided: false, incomplete: false, behind_base: false, ahead_base: true, upstream: Upstream::Ahead, push_paused: false, pr: Pr::Ready, checks: Checks::Passing, workflows: true, review_paused: true }}) == Step::Push);\n}}\n#[kani::proof]\nfn reject_force_pushing_over_commits_the_branch_never_held() {{\n    assert!(decide(State {{ host: Host::Ready, head: Head::Feature, dirty: false, undecided: false, incomplete: false, behind_base: false, ahead_base: true, upstream: Upstream::Diverged, push_paused: false, pr: Pr::Draft, checks: Checks::Passing, workflows: true, review_paused: false }}) == Step::ForcePush);\n}}\n"
        ),
    )?;
    for name in [
        "reject_executing_a_push",
        "reject_pushing_while_paused",
        "reject_pushing_to_a_ready_pr_while_review_is_paused",
        "reject_force_pushing_over_commits_the_branch_never_held",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
    ensure!(
        fs::read(source)? == original,
        "production next-action proof source changed during verification"
    );
    let source = crate::canonical(&root.join("xtask/src/eol_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &LINE_ENDING_HARNESSES)?;
    verify_file(&source, directory.path(), &LINE_ENDING_HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]\nmod production;\nuse production::{{Action, Attributes, Eol, Index, Staging, Text, action, content}};\n#[kani::proof]\n#[kani::unwind(5)]\nfn reject_rewriting_declared_crlf() {{\n    let attributes = Attributes {{ text: Text::Auto, eol: Eol::Crlf, binary: false }};\n    assert!(action(attributes, Staging::Add, Index::Other, content(b\"a\\r\\n\")) == Action::Normalize);\n}}\n#[kani::proof]\n#[kani::unwind(5)]\nfn reject_rewriting_control_heavy_content() {{\n    let attributes = Attributes {{ text: Text::Auto, eol: Eol::Lf, binary: false }};\n    assert!(action(attributes, Staging::Add, Index::Other, content(b\"\\x01\\x02\\r\\n\")) == Action::Normalize);\n}}\n#[kani::proof]\n#[kani::unwind(5)]\nfn reject_rewriting_crlf_the_index_keeps() {{\n    let attributes = Attributes {{ text: Text::Auto, eol: Eol::Lf, binary: false }};\n    assert!(action(attributes, Staging::Add, Index::CrlfText, content(b\"a\\r\\n\")) == Action::Normalize);\n}}\n"
        ),
    )?;
    for name in [
        "reject_rewriting_declared_crlf",
        "reject_rewriting_control_heavy_content",
        "reject_rewriting_crlf_the_index_keeps",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
    ensure!(
        fs::read(source)? == original,
        "production line-ending proof source changed during verification"
    );
    let source = crate::canonical(&root.join("xtask/src/instruction_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &INSTRUCTION_HARNESSES)?;
    verify_file(&source, directory.path(), &INSTRUCTION_HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]
mod production;
#[kani::proof]
fn reject_a_directive_classified_as_judgment() {{
    assert!(production::admitted(production::Line::Directive, production::Class::Judgment, true));
}}
#[kani::proof]
fn reject_an_undefined_follow_up() {{
    assert!(production::admitted(production::Line::Text, production::Class::FollowUp, false));
}}
#[kani::proof]
fn reject_a_duplicate_follow_up() {{
    assert!(production::follow_up(2, 1) == production::Definition::Current);
}}
#[kani::proof]
fn reject_a_gate_without_its_source() {{
    assert!(production::removed_row_held(Some(production::Holder::Gate), false, true, true));
}}
#[kani::proof]
fn reject_a_directive_that_renders_output() {{
    assert!(production::admitted(production::Line::Output, production::Class::Directive, true));
}}
#[kani::proof]
fn reject_a_merge_without_its_retained_line() {{
    assert!(production::removed_row_held(Some(production::Holder::Merged), true, true, false));
}}
#[kani::proof]
fn reject_a_contradiction_without_what_it_contradicted() {{
    assert!(production::removed_row_held(Some(production::Holder::Contradicted), true, false, false));
}}
"
        ),
    )?;
    for name in [
        "reject_a_directive_classified_as_judgment",
        "reject_an_undefined_follow_up",
        "reject_a_duplicate_follow_up",
        "reject_a_gate_without_its_source",
        "reject_a_directive_that_renders_output",
        "reject_a_merge_without_its_retained_line",
        "reject_a_contradiction_without_what_it_contradicted",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
    ensure!(
        fs::read(source)? == original,
        "production instruction proof source changed during verification"
    );
    let source = crate::canonical(&root.join("xtask/src/change_rules.rs"))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    verify_inventory(&source, directory.path(), &CHANGE_HARNESSES)?;
    verify_file(&source, directory.path(), &CHANGE_HARNESSES, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {source:?}]\nmod production;\nuse production::{{Class, Gates, classify, gates, matched}};\n#[kani::proof]\n#[kani::unwind(40)]\nfn reject_an_unknown_path_selecting_less_than_everything() {{\n    let bytes: [u8; 24] = kani::any();\n    let path = &bytes[..];\n    kani::assume(matched(path).is_none());
    assert!(gates(classify(path)) != Gates::ALL);\n}}\n#[kani::proof]\n#[kani::unwind(40)]\nfn reject_a_lock_file_selecting_a_single_class() {{\n    let mut bytes: [u8; 24] = kani::any();\n    bytes[19..].copy_from_slice(b\".lock\");\n    assert!(classify(&bytes) != Class::Everything);\n}}\n"
        ),
    )?;
    for name in [
        "reject_an_unknown_path_selecting_less_than_everything",
        "reject_a_lock_file_selecting_a_single_class",
    ] {
        verify_file(&file, probe.path(), &[name], true)?;
    }
    ensure!(
        fs::read(source)? == original,
        "production change-classifier proof source changed during verification"
    );
    println!(
        "Verified all {} production policy harnesses, reachable outcomes, and the rejecting counterexamples",
        HARNESSES.len()
            + PR_HARNESSES.len()
            + PROFILE_HARNESSES.len()
            + REAPER_HARNESSES.len()
            + GATE_HARNESSES.len()
            + REVIEW_HARNESSES.len()
            + INSTRUCTION_HARNESSES.len()
            + NEXT_ACTION_HARNESSES.len()
            + LINE_ENDING_HARNESSES.len()
            + CHANGE_HARNESSES.len()
    );
    Ok(())
}
