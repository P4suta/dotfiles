use crate::proof_budget_rules;
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

/// Compares a Kani results document with the expected harnesses and the proof budget.
///
/// # Errors
///
/// Returns an error when the run stops early or holds no harness, the harness set differs from `expected`, a harness lacks a measurement or exceeds the budget, or a result contradicts `counterexample`.
/// A normal run must pass every proof with an assertion, two satisfied cover witnesses, and every property verified, and a counterexample run must fail every harness on an assertion.
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
    let slow: Vec<String> = results
        .iter()
        .filter_map(|result| {
            let duration = result["duration_ms"].as_u64();
            (proof_budget_rules::verdict(duration) != proof_budget_rules::Verdict::Within).then(
                || {
                    format!(
                        "{} took {} ms",
                        result["harness_id"].as_str().unwrap_or_default(),
                        duration
                            .map_or_else(|| "unmeasured".to_owned(), |millis| millis.to_string())
                    )
                },
            )
        })
        .collect();
    ensure!(
        slow.is_empty(),
        "each proof harness must finish within {} ms; shrink its bounds or split it without weakening its assertions:\n{}",
        proof_budget_rules::BUDGET_MILLIS,
        slow.join("\n")
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
        ])
        .arg(format!("{}s", proof_budget_rules::TIMEOUT_SECONDS))
        .arg("--export-json")
        .arg(&output);
    if counterexample {
        command.args(["--harness", expected[0], "--exact"]);
    }
    let status = command.status().context("start pinned Kani verifier")?;
    ensure!(
        status.success() != counterexample,
        "Kani returned an unexpected outcome: {status}"
    );
    let result: Value = skill_ops::read_json(&output)?;
    validate_results(&result, expected, counterexample)
}

/// The harnesses that `kani list` finds in `source`, which every run must then pass.
/// The inventory follows the source, so a harness added beside its rule joins the gate without a second list to edit.
fn verify_inventory(source: &Path, directory: &Path) -> Result<BTreeSet<String>> {
    let status = Tool::Kani
        .command()
        .current_dir(directory)
        .args(["list", "--format", "json", "-Z", "stubbing"])
        .arg(source)
        .status()?;
    ensure!(status.success(), "cannot list required proof harnesses");
    let listed: Value = skill_ops::read_json(&directory.join("kani-list.json"))?;
    ensure!(
        listed["kani-version"] == KANI_VERSION,
        "unexpected proof inventory version"
    );
    let declarations = listed["standard-harnesses"]
        .as_object()
        .context("missing standard harness inventory")?;
    let names: BTreeSet<String> = declarations
        .values()
        .flat_map(|value| value.as_array().into_iter().flatten())
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("invalid proof inventory")
        })
        .collect::<Result<_>>()?;
    ensure!(
        !names.is_empty(),
        "{} declares no proof harness",
        source.display()
    );
    Ok(names)
}

fn short_digest(bytes: &[u8]) -> String {
    crate::build_inputs::hex(&Sha256::digest(bytes)[..8])
}

#[must_use]
pub fn container_image(definition: &[u8]) -> String {
    format!("dotfiles-proofs:{}", short_digest(definition))
}

#[must_use]
pub fn container_target_volume(root: &Path) -> String {
    format!(
        "dotfiles-proofs-target-{}",
        short_digest(root.as_os_str().as_encoded_bytes())
    )
}

/// Kani writes temporary files beside the source it compiles, so the gate runs on a container-local copy of the read-only checkout.
const CONTAINER_GATE: &str = "set -o pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -; exec cargo run --locked --manifest-path xtask/Cargo.toml -- proofs";

pub fn container_run_arguments(root: &Path, image: &str, target_volume: &str) -> Vec<OsString> {
    // Docker reads `--mount` as one CSV record, so quoting keeps a comma in the path inside the field, and Windows forbids the quote character in paths.
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

/// Kani has no native build here, so the unchanged proof gate runs on a pinned Linux verification host that mounts this checkout read-only.
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

/// Runs every registered Kani proof natively on macOS and Linux, and in a Linux container elsewhere.
///
/// # Errors
///
/// Returns an error when the pinned Kani version or Docker engine can't run, Docker can't build the verification image, or any proof or counterexample check fails.
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
    let mut verified = 0;
    for rules in &RULES {
        verified += verify_rules(root, rules)?;
    }
    println!(
        "Verified all {verified} production policy harnesses, reachable outcomes, and the rejecting counterexamples"
    );
    Ok(())
}

/// One pure rule module.
/// Every harness that `kani list` finds in it must pass.
/// Each counterexample, a harness asserting a rejected rule, must fail when it runs on that module.
#[derive(Debug)]
pub struct Rules {
    pub source: &'static str,
    /// Harness definitions placed after `mod production;`, which names the module under proof.
    pub counterexamples: &'static str,
    pub rejected: &'static [&'static str],
}

/// Every rule module registered with the shared verification, in source order.
pub const RULES: [Rules; 29] = [
    Rules {
        source: "crates/skill-ops/src/skill_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_completion_without_evidence() {
    assert!(production::decision_complete(production::Outcome::Implemented, true, false, false));
}
#[kani::proof]
fn reject_adopting_a_changed_skill() {
    assert!(production::adopted(false, production::Outcome::Keep));
}
#[kani::proof]
fn reject_a_stale_tracked_decision() {
    assert!(production::tracked_decision(true, false, true) == production::Tracked::Current);
}
#[kani::proof]
fn reject_skipping_triage_with_open_findings() {
    assert!(production::triage_complete(1, false));
}
#[kani::proof]
fn reject_an_unrecorded_oversized_skill() {
    assert!(production::size_disposition(true, false, false) == production::Sizing::Settled);
}
#[kani::proof]
fn reject_completing_the_gate_with_open_findings() {
    assert!(production::gate_complete(false, 1));
}
#[kani::proof]
fn reject_a_diff_from_unassessed_content() {
    assert!(production::diff_base(true, false) == production::DiffBase::Assessed);
}
",
        rejected: &[
            "reject_completion_without_evidence",
            "reject_adopting_a_changed_skill",
            "reject_a_stale_tracked_decision",
            "reject_skipping_triage_with_open_findings",
            "reject_an_unrecorded_oversized_skill",
            "reject_completing_the_gate_with_open_findings",
            "reject_a_diff_from_unassessed_content",
        ],
    },
    Rules {
        source: "crates/pr-workflow/src/pr_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_ready_local_creation() {
    assert_eq!(production::plan(production::Operation::Create, production::Generation::Local, true, false, production::IssueGate::Linked, production::Checks::Passing), Ok(production::Effect::CreateReady));
}
#[kani::proof]
fn reject_creation_without_a_personal_issue() {
    assert!(production::plan(production::Operation::Create, production::Generation::Coderabbit, true, false, production::issue_gate(true, false, false), production::Checks::Passing).is_ok());
}
#[kani::proof]
fn reject_unfinished_summary_as_review_exclusion() {
    assert!(production::coderabbit_body_marker_allowed(false, true, false));
}
#[kani::proof]
fn reject_ready_with_unfinished_checks() {
    assert_eq!(production::plan(production::Operation::Ready, production::Generation::Local, true, true, production::IssueGate::Linked, production::checks_state(3, 1, 0)), Ok(production::Effect::Ready));
}
#[kani::proof]
fn reject_blaming_checks_for_a_published_pr() {
    assert_eq!(production::plan(production::Operation::Ready, production::Generation::Local, true, false, production::IssueGate::Linked, production::Checks::Incomplete), Err(production::Blocker::UnfinishedChecks));
}
#[kani::proof]
fn reject_paused_review_request() {
    assert!(production::review_request_allowed(true, production::requests_review(production::Operation::Ready, production::Generation::Local, true, false, false)));
}
#[kani::proof]
fn reject_paused_exclusion_removal() {
    assert!(production::review_request_allowed(true, production::requests_review(production::Operation::Edit, production::Generation::Local, false, true, false)));
}
#[kani::proof]
fn reject_pause_exclusion_as_standing() {
    assert_eq!(production::review_requirement(false, production::exclusion(true, true)), production::ReviewRequirement::Excluded);
}
#[kani::proof]
fn reject_standing_exclusion_when_ready_during_the_pause() {
    assert!(production::keeps_pause_exclusion(production::Operation::Ready, true, production::Exclusion::None, production::exclusion(true, false)));
}
fn state(lifecycle: production::Lifecycle, behind: bool, checks: production::HeadChecks, exclusion: production::Exclusion) -> production::MergeState {
    production::MergeState { lifecycle, conflict: false, behind, checks, clean: true, exclusion }
}
#[kani::proof]
fn reject_merging_a_behind_pr() {
    assert!(production::merge_step(state(production::Lifecycle::Ready, true, production::HeadChecks::Passing, production::Exclusion::Pause), false, true) == production::Step::Merge);
}
#[kani::proof]
fn reject_updating_a_stacked_pr_by_merge() {
    assert!(production::merge_step(state(production::Lifecycle::Ready, true, production::HeadChecks::Passing, production::Exclusion::Pause), true, true) == production::Step::Update);
}
#[kani::proof]
fn reject_merging_with_a_failed_check() {
    assert!(production::merge_step(state(production::Lifecycle::Ready, false, production::HeadChecks::Failed, production::Exclusion::Pause), false, true) == production::Step::Merge);
}
#[kani::proof]
fn reject_readying_without_the_pause_lines() {
    assert!(production::merge_step(state(production::Lifecycle::Draft, false, production::HeadChecks::Passing, production::Exclusion::None), false, true) == production::Step::Ready);
}
#[kani::proof]
fn reject_opening_a_dependent_branch_independently() {
    assert!(production::dependency(true, false, true).is_none());
}
",
        rejected: &[
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
        ],
    },
    Rules {
        source: "crates/model/src/profile_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_unapproved_application() {
    assert!(production::permitted(production::Mode::Full, true, true, false));
}
#[kani::proof]
fn reject_an_unset_memory_switch() {
    assert!(production::memory_off(&[production::Memory::Disabled, production::Memory::Unset]));
}
#[kani::proof]
fn reject_overwriting_a_merge_that_drops_a_host_key() {
    assert!(!production::host_edit_refused(true, true, false));
}
#[kani::proof]
fn reject_detecting_wsl_on_a_plain_linux_kernel() {
    assert!(production::detected(production::System::Linux, false) == production::Profile::Wsl);
}
",
        rejected: &[
            "reject_unapproved_application",
            "reject_an_unset_memory_switch",
            "reject_overwriting_a_merge_that_drops_a_host_key",
            "reject_detecting_wsl_on_a_plain_linux_kernel",
        ],
    },
    Rules {
        source: "crates/dotguard/src/reaper_rules.rs",
        counterexamples: "",
        rejected: &[],
    },
    Rules {
        source: "crates/dotguard/src/gate_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_rebasing_across_several_refs() {
    assert!(production::signing(2, production::Reference::Branch, false, false) == production::Signing::Rebase);
}
#[kani::proof]
fn reject_rebasing_a_range_with_merges() {
    assert!(production::signing(1, production::Reference::Branch, true, false) == production::Signing::Rebase);
}
#[kani::proof]
fn reject_amending_a_ref_that_is_not_checked_out() {
    assert!(production::signing(1, production::Reference::Other, false, true) == production::Signing::Amend);
}
#[kani::proof]
fn reject_deleting_through_the_forge_before_fetching() {
    assert!(production::history(true, false) == production::History::ForgeDelete);
}
#[kani::proof]
fn reject_completing_a_prefix_of_two_options() {
    assert!(production::spelling(b\"a\", &[&b\"ab\"[..], &b\"ac\"[..]]) == production::Spelling::Is(0));
}
#[kani::proof]
fn reject_completing_over_an_exact_spelling() {
    assert!(production::spelling(b\"ab\", &[&b\"abc\"[..], &b\"ab\"[..]]) == production::Spelling::Is(0));
}
#[kani::proof]
fn reject_ignoring_a_unique_abbreviation() {
    assert!(production::spelling(b\"re\", &[&b\"repo\"[..], &b\"force\"[..]]) == production::Spelling::Unknown);
}
#[kani::proof]
fn reject_suggesting_from_an_unread_line() {
    assert!(production::verdict(true, false) == production::Verdict::Suggest);
}
#[kani::proof]
fn reject_retrying_a_reword_without_what_it_implied() {
    assert!(production::commit_retry(production::Source { fixup: production::Fixup::Reword, reuse: false, renew: false, amend: false }, Some(false), false) == Ok(production::Retry { allow_empty: false, only: false, authorship: false, reset_author: true }));
}
#[kani::proof]
fn reject_retrying_without_the_reused_authorship() {
    assert!(production::commit_retry(production::Source { fixup: production::Fixup::Absent, reuse: true, renew: false, amend: false }, Some(false), false).is_ok());
}
#[kani::proof]
fn reject_keeping_a_reset_author_without_its_reuse() {
    assert!(production::commit_retry(production::Source { fixup: production::Fixup::Absent, reuse: true, renew: true, amend: false }, Some(false), true).is_ok_and(|retry| retry.reset_author));
}
#[kani::proof]
fn reject_the_reused_authorship_during_a_pick() {
    assert!(production::commit_retry(production::Source { fixup: production::Fixup::Absent, reuse: true, renew: false, amend: false }, Some(true), true).is_ok_and(|retry| retry.authorship));
}
#[kani::proof]
fn reject_retrying_a_reuse_without_knowing_of_a_pick() {
    assert!(production::commit_retry(production::Source { fixup: production::Fixup::Absent, reuse: true, renew: true, amend: false }, None, true).is_ok());
}
#[kani::proof]
fn reject_a_manual_rebase_of_a_published_branch() {
    assert!(!production::rebase_refused(false, Some(true)));
}
#[kani::proof]
fn reject_an_unleased_stack_marker() {
    assert!(production::stack_tooling(Some(b\"t\"), None));
}
#[kani::proof]
fn reject_the_lease_admitting_a_plain_force_push() {
    assert!(production::admission(false, true, false) == production::Admission::Tooling);
}
",
        rejected: &[
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
        ],
    },
    Rules {
        source: "crates/coderabbit-guard/src/review_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_eighth_review_of_ten() {
    assert!(production::included_allowance(10, 3));
}
#[kani::proof]
fn reject_rounding_the_layered_budget_up() {
    assert_eq!(production::capacity_budget(49), 36);
}
#[kani::proof]
fn reject_eighth_local_attempt() {
    assert!(production::rolling_allowance(7, 7));
}
#[kani::proof]
fn reject_early_review() {
    assert!(production::cooldown_complete(100, 101, production::PROBE_GAP_MS));
}
#[kani::proof]
fn reject_owner_pause_bypass() {
    assert!(production::service_access(production::pause_scope(false, false, true), false));
}
",
        rejected: &[
            "reject_eighth_review_of_ten",
            "reject_rounding_the_layered_budget_up",
            "reject_eighth_local_attempt",
            "reject_early_review",
            "reject_owner_pause_bypass",
        ],
    },
    Rules {
        source: "crates/prose/src/prose_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_allowing_a_failing_reply() {
    assert!(production::reply(false, false) == production::Reply::Allow);
}
#[kani::proof]
fn reject_ending_after_another_hooks_continuation() {
    assert!(production::reply(false, production::rewritten_by_this_hook(true, Some(false))) == production::Reply::EndUnchecked);
}
#[kani::proof]
fn reject_the_standard_on_a_fork() {
    assert!(production::standard_applies(true, true));
}
#[kani::proof]
fn reject_skipping_an_unresolved_origin_silently() {
    assert!(production::repository_standard(None, Some(production::Owner::Unresolved), false, false) == production::Standard::Skips);
}
#[kani::proof]
fn reject_skipping_a_repository_without_origin_silently() {
    assert!(production::repository_standard(None, None, false, false) == production::Standard::Skips);
}
#[kani::proof]
fn reject_ignoring_a_personal_remote_without_origin() {
    assert!(production::repository_standard(None, None, true, false) == production::Standard::Skips);
}
#[kani::proof]
fn reject_overriding_a_declared_opt_out() {
    assert!(production::repository_standard(Some(false), Some(production::Owner::Personal), false, false) == production::Standard::Applies);
}
",
        rejected: &[
            "reject_allowing_a_failing_reply",
            "reject_ending_after_another_hooks_continuation",
            "reject_the_standard_on_a_fork",
            "reject_skipping_an_unresolved_origin_silently",
            "reject_skipping_a_repository_without_origin_silently",
            "reject_ignoring_a_personal_remote_without_origin",
            "reject_overriding_a_declared_opt_out",
        ],
    },
    Rules {
        source: "xtask/src/next_action_rules.rs",
        counterexamples: "use production::*;
#[kani::proof]
fn reject_executing_a_push() {
    assert!(executable(Step::Push));
}
#[kani::proof]
fn reject_pushing_while_paused() {
    assert!(decide(State { host: Host::Ready, head: Head::Feature, dirty: false, undecided: false, incomplete: false, behind_base: false, ahead_base: true, upstream: Upstream::Ahead, push_paused: true, pr: Pr::Draft, checks: Checks::Passing, workflows: true, review_paused: false }) == Step::Push);
}
#[kani::proof]
fn reject_pushing_to_a_ready_pr_while_review_is_paused() {
    assert!(decide(State { host: Host::Ready, head: Head::Feature, dirty: false, undecided: false, incomplete: false, behind_base: false, ahead_base: true, upstream: Upstream::Ahead, push_paused: false, pr: Pr::Ready, checks: Checks::Passing, workflows: true, review_paused: true }) == Step::Push);
}
#[kani::proof]
fn reject_force_pushing_over_commits_the_branch_never_held() {
    assert!(decide(State { host: Host::Ready, head: Head::Feature, dirty: false, undecided: false, incomplete: false, behind_base: false, ahead_base: true, upstream: Upstream::Diverged, push_paused: false, pr: Pr::Draft, checks: Checks::Passing, workflows: true, review_paused: false }) == Step::ForcePush);
}
",
        rejected: &[
            "reject_executing_a_push",
            "reject_pushing_while_paused",
            "reject_pushing_to_a_ready_pr_while_review_is_paused",
            "reject_force_pushing_over_commits_the_branch_never_held",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/eol_rules.rs",
        counterexamples: "use production::{Action, Attributes, Eol, Index, Staging, Text, action, content};
#[kani::proof]
#[kani::unwind(5)]
fn reject_rewriting_declared_crlf() {
    let attributes = Attributes { text: Text::Auto, eol: Eol::Crlf, binary: false };
    assert!(action(attributes, Staging::Add, Index::Other, content(b\"a\\r\\n\")) == Action::Normalize);
}
#[kani::proof]
#[kani::unwind(5)]
fn reject_rewriting_control_heavy_content() {
    let attributes = Attributes { text: Text::Auto, eol: Eol::Lf, binary: false };
    assert!(action(attributes, Staging::Add, Index::Other, content(b\"\\x01\\x02\\r\\n\")) == Action::Normalize);
}
#[kani::proof]
#[kani::unwind(5)]
fn reject_rewriting_crlf_the_index_keeps() {
    let attributes = Attributes { text: Text::Auto, eol: Eol::Lf, binary: false };
    assert!(action(attributes, Staging::Add, Index::CrlfText, content(b\"a\\r\\n\")) == Action::Normalize);
}
",
        rejected: &[
            "reject_rewriting_declared_crlf",
            "reject_rewriting_control_heavy_content",
            "reject_rewriting_crlf_the_index_keeps",
        ],
    },
    Rules {
        source: "xtask/src/instruction_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_a_directive_classified_as_judgment() {
    assert!(production::admitted(production::Line::Directive, production::Class::Judgment, true));
}
#[kani::proof]
fn reject_an_undefined_follow_up() {
    assert!(production::admitted(production::Line::Text, production::Class::FollowUp, false));
}
#[kani::proof]
fn reject_a_duplicate_follow_up() {
    assert!(production::follow_up(2, 1) == production::Definition::Current);
}
#[kani::proof]
fn reject_a_gate_without_its_source() {
    assert!(production::removed_row_held(Some(production::Holder::Gate), false, true, true));
}
#[kani::proof]
fn reject_a_directive_that_renders_output() {
    assert!(production::admitted(production::Line::Output, production::Class::Directive, true));
}
#[kani::proof]
fn reject_a_merge_without_its_retained_line() {
    assert!(production::removed_row_held(Some(production::Holder::Merged), true, true, false));
}
#[kani::proof]
fn reject_a_contradiction_without_what_it_contradicted() {
    assert!(production::removed_row_held(Some(production::Holder::Contradicted), true, false, false));
}
",
        rejected: &[
            "reject_a_directive_classified_as_judgment",
            "reject_an_undefined_follow_up",
            "reject_a_duplicate_follow_up",
            "reject_a_gate_without_its_source",
            "reject_a_directive_that_renders_output",
            "reject_a_merge_without_its_retained_line",
            "reject_a_contradiction_without_what_it_contradicted",
        ],
    },
    Rules {
        source: "crates/platform/src/target_rules.rs",
        counterexamples: "use production::{Target, target};
#[kani::proof]
fn reject_ignoring_an_absolute_target() {
    assert!(target(true, true) == Target::Development);
}
#[kani::proof]
fn reject_adopting_a_relative_target() {
    assert!(target(false, true) == Target::Configured);
}
#[kani::proof]
fn reject_the_home_target_beside_a_development_volume() {
    assert!(target(false, true) == Target::Home);
}
",
        rejected: &[
            "reject_ignoring_an_absolute_target",
            "reject_adopting_a_relative_target",
            "reject_the_home_target_beside_a_development_volume",
        ],
    },
    Rules {
        source: "crates/platform/src/stale_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_calling_a_matching_build_stale() {
    assert!(production::freshness(true, true) == production::Freshness::Stale);
}
#[kani::proof]
fn reject_running_a_stale_build() {
    assert!(production::runs(production::freshness(true, false)));
}
#[kani::proof]
fn reject_preferring_the_build_checkout_over_the_working_copy() {
    assert!(production::origin(false, true, true) == Some(production::Origin::BuildCheckout));
}
#[kani::proof]
fn reject_installing_an_unrecorded_tool() {
    assert!(production::reinstalls(false, false));
}
",
        rejected: &[
            "reject_calling_a_matching_build_stale",
            "reject_running_a_stale_build",
            "reject_preferring_the_build_checkout_over_the_working_copy",
            "reject_installing_an_unrecorded_tool",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/shell_write_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_admitting_an_inline_interpreter() {
    assert!(production::verdict(true, false) == production::Verdict::Admit);
}
#[kani::proof]
fn reject_admitting_a_file_redirect() {
    assert!(production::verdict(false, true) == production::Verdict::Admit);
}
",
        rejected: &[
            "reject_admitting_an_inline_interpreter",
            "reject_admitting_a_file_redirect",
        ],
    },
    Rules {
        source: "crates/pr-workflow/src/wip_rules.rs",
        counterexamples: "use production::{admits, limit};
#[kani::proof]
fn reject_admitting_a_pr_at_the_limit() {
    assert!(admits(1, limit(1, None)));
}
#[kani::proof]
fn reject_ignoring_a_repository_override() {
    assert!(limit(1, Some(3)) == 1);
}
",
        rejected: &[
            "reject_admitting_a_pr_at_the_limit",
            "reject_ignoring_a_repository_override",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/timeout_rules.rs",
        counterexamples: "use production::{Timeout, percentile, timeout};
#[kani::proof]
fn reject_keeping_a_timeout_below_the_observed_percentile() {
    assert!(timeout(Some(100_000), 0, Some(60_000), 600_000) == Timeout::Keep(60_000));
}
#[kani::proof]
fn reject_a_timeout_above_the_client_limit() {
    assert!(timeout(Some(590_000), 0, None, 600_000).millis() > 600_000);
}
#[kani::proof]
fn reject_the_median_as_the_high_percentile() {
    assert!(percentile(&[1, 2, 3]) == Some(2));
}
",
        rejected: &[
            "reject_keeping_a_timeout_below_the_observed_percentile",
            "reject_a_timeout_above_the_client_limit",
            "reject_the_median_as_the_high_percentile",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/ready_push_rules.rs",
        counterexamples: "use production::{Pr, Verdict, push};
#[kani::proof]
fn reject_admitting_a_push_to_a_ready_pr() {
    assert!(push(true, Pr::Ready) == Verdict::Admit);
}
#[kani::proof]
fn reject_admitting_an_unknown_pr_state() {
    assert!(push(true, Pr::Unknown) == Verdict::Admit);
}
#[kani::proof]
fn reject_refusing_a_push_to_a_draft() {
    assert!(push(true, Pr::Draft) == Verdict::ReturnToDraft);
}
",
        rejected: &[
            "reject_admitting_a_push_to_a_ready_pr",
            "reject_admitting_an_unknown_pr_state",
            "reject_refusing_a_push_to_a_draft",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/disk_scan_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_admitting_a_recursive_size_scan() {
    assert!(production::verdict(true, true) == production::Verdict::Admit);
}
#[kani::proof]
fn reject_refusing_a_walk_without_sizes() {
    assert!(production::verdict(true, false) == production::Verdict::Refuse);
}
",
        rejected: &[
            "reject_admitting_a_recursive_size_scan",
            "reject_refusing_a_walk_without_sizes",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/path_conversion_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_admitting_a_converted_remote_path() {
    assert!(production::verdict(true, true, false) == production::Verdict::Admit);
}
#[kani::proof]
fn reject_refusing_a_command_with_the_conversion_off() {
    assert!(production::verdict(true, true, true) == production::Verdict::Refuse);
}
",
        rejected: &[
            "reject_admitting_a_converted_remote_path",
            "reject_refusing_a_command_with_the_conversion_off",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/body_rules.rs",
        counterexamples: "#[kani::proof]
fn reject_admitting_an_attributed_body() {
    assert!(production::verdict(true) == production::Verdict::Admit);
}
#[kani::proof]
fn reject_an_empty_deleting_next_action() {
    assert!(production::next_action(production::Reason::Attribution).is_empty());
}
",
        rejected: &[
            "reject_admitting_an_attributed_body",
            "reject_an_empty_deleting_next_action",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/hosts_rules.rs",
        counterexamples: "use production::*;
#[kani::proof]
fn reject_admitting_a_failed_family() {
    assert!(admit([true, true, true], [Evidence::Passed, Evidence::Failed, Evidence::Passed]) == Verdict::Admit);
}
#[kani::proof]
fn reject_a_record_for_another_tree() {
    assert!(record(Evidence::Missing, false, true) == Evidence::Passed);
}
#[kani::proof]
fn reject_silently_accepting_a_changed_key() {
    assert!(pin(Key::Changed, false));
}
#[kani::proof]
fn reject_checking_a_dirty_tree() {
    assert!(start(false, [true, true, true], [true, true, true]) == Start::Run);
}
",
        rejected: &[
            "reject_admitting_a_failed_family",
            "reject_a_record_for_another_tree",
            "reject_silently_accepting_a_changed_key",
            "reject_checking_a_dirty_tree",
        ],
    },
    Rules {
        source: "xtask/src/change_rules.rs",
        counterexamples: "use production::{Class, Gates, classify, gates, matched};
#[kani::proof]
#[kani::unwind(40)]
fn reject_an_unknown_path_selecting_less_than_everything() {
    let bytes: [u8; 24] = kani::any();
    let path = &bytes[..];
    kani::assume(matched(path).is_none());
    assert!(gates(classify(path)) != Gates::ALL);
}
#[kani::proof]
#[kani::unwind(40)]
fn reject_a_lock_file_selecting_a_single_class() {
    let mut bytes: [u8; 24] = kani::any();
    bytes[19..].copy_from_slice(b\".lock\");
    assert!(classify(&bytes) != Class::Everything);
}
",
        rejected: &[
            "reject_an_unknown_path_selecting_less_than_everything",
            "reject_a_lock_file_selecting_a_single_class",
        ],
    },
    Rules {
        source: "crates/prose/src/source_name_rules.rs",
        counterexamples: "use production::{Entry, target};
#[kani::proof]
fn reject_keeping_the_empty_attribute_in_a_target_name() {
    assert!(target(Entry::File, b\"empty_vocab.txt\").is_some_and(|found| found.name == b\"empty_vocab.txt\"));
}
#[kani::proof]
fn reject_a_template_as_a_verbatim_copy() {
    assert!(target(Entry::File, b\"policy.json.tmpl\").is_some());
}
",
        rejected: &[
            "reject_keeping_the_empty_attribute_in_a_target_name",
            "reject_a_template_as_a_verbatim_copy",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/start_health_rules.rs",
        counterexamples: "use production::{Events, Start, Verdict, verdict};
#[kani::proof]
fn reject_starting_work_on_a_start_that_never_finished() {
    assert!(verdict(Start::TimedOut, Some(Events::NONE)) == Verdict::Healthy);
}
#[kani::proof]
fn reject_refusing_a_fast_start_for_past_events() {
    let events = Events { engine_terminated: 1, protection_failed: 0, service_timeout: 0 };
    assert!(verdict(Start::Completed { millis: 40 }, Some(events)) == Verdict::Stalled);
}
",
        rejected: &[
            "reject_starting_work_on_a_start_that_never_finished",
            "reject_refusing_a_fast_start_for_past_events",
        ],
    },
    Rules {
        source: "xtask/src/ignore_rules.rs",
        counterexamples: "use production::{Verdict, matches, verdict};
#[kani::proof]
#[kani::unwind(8)]
fn reject_a_star_that_crosses_a_segment() {
    assert!(matches(b\"*\", b\"a/b\"));
}
#[kani::proof]
fn reject_keeping_a_repeated_pattern() {
    assert!(verdict(true, 1) == Verdict::Kept);
}
",
        rejected: &[
            "reject_a_star_that_crosses_a_segment",
            "reject_keeping_a_repeated_pattern",
        ],
    },
    Rules {
        source: "xtask/src/proof_budget_rules.rs",
        counterexamples: "use production::{BUDGET_MILLIS, Verdict, verdict};
#[kani::proof]
fn reject_admitting_an_overrun() {
    assert!(verdict(Some(BUDGET_MILLIS + 1)) == Verdict::Within);
}
#[kani::proof]
fn reject_admitting_an_unmeasured_harness() {
    assert!(verdict(None) == Verdict::Within);
}
",
        rejected: &[
            "reject_admitting_an_overrun",
            "reject_admitting_an_unmeasured_harness",
        ],
    },
    Rules {
        source: "xtask/src/input_rules.rs",
        counterexamples: "use production::{Term, Verdict, verdict};
#[kani::proof]
fn reject_hashing_a_chezmoi_variable() {
    assert!(verdict(&[Term::Data, Term::Chezmoi]) == Verdict::Admit);
}
#[kani::proof]
fn reject_hashing_the_whole_context() {
    assert!(verdict(&[Term::Data, Term::Context]) == Verdict::Admit);
}
",
        rejected: &[
            "reject_hashing_a_chezmoi_variable",
            "reject_hashing_the_whole_context",
        ],
    },
    Rules {
        source: "crates/dotfiles/src/windows/setup/ocomment_rules.rs",
        counterexamples: "use production::{Action, action};
#[kani::proof]
fn reject_skipping_a_moved_checkout() {
    assert!(action(Some(&b\"b\"[..]), true, Some(&b\"a\"[..])) == Action::Current);
}
#[kani::proof]
fn reject_skipping_a_missing_binary() {
    assert!(action(Some(&b\"a\"[..]), false, Some(&b\"a\"[..])) == Action::Current);
}
",
        rejected: &[
            "reject_skipping_a_moved_checkout",
            "reject_skipping_a_missing_binary",
        ],
    },
    Rules {
        source: "crates/platform/src/aside_rules.rs",
        counterexamples: "use production::is_aside;
#[kani::proof]
fn reject_sweeping_the_installed_binary() {
    assert!(is_aside(b\"a\", b\"a\"));
}
#[kani::proof]
fn reject_sweeping_another_binary() {
    assert!(is_aside(b\"a\", b\"b.1.old\"));
}
",
        rejected: &[
            "reject_sweeping_the_installed_binary",
            "reject_sweeping_another_binary",
        ],
    },
    Rules {
        source: "xtask/src/adr_rules.rs",
        counterexamples: "use production::{Status, Verdict, verdict};
#[kani::proof]
fn reject_a_superseded_record_without_its_successor() {
    assert!(verdict(Status::Superseded, 0) == Verdict::Consistent);
}
#[kani::proof]
fn reject_an_accepted_record_that_later_records_changed() {
    assert!(verdict(Status::Accepted, 1) == Verdict::Consistent);
}
",
        rejected: &[
            "reject_a_superseded_record_without_its_successor",
            "reject_an_accepted_record_that_later_records_changed",
        ],
    },
];

/// Verifies one rule module and its counterexamples, and returns how many harnesses it proved.
fn verify_rules(root: &Path, rules: &Rules) -> Result<usize> {
    let source = crate::canonical(&root.join(rules.source))?;
    let original = fs::read(&source)?;
    let directory = tempfile::tempdir()?;
    let harnesses = verify_inventory(&source, directory.path())?;
    let names: Vec<&str> = harnesses.iter().map(String::as_str).collect();
    verify_file(&source, directory.path(), &names, false)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {:?}]\nmod production;\n{}",
            source.to_string_lossy(),
            rules.counterexamples
        ),
    )?;
    for name in rules.rejected {
        verify_file(&file, probe.path(), &[name], true)?;
    }
    ensure!(
        fs::read(&source)? == original,
        "production proof source {} changed during verification",
        rules.source
    );
    Ok(names.len())
}
