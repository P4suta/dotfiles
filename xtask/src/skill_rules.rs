#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(kani), derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(not(kani), serde(rename_all = "kebab-case"))]
pub enum Outcome {
    Keep,
    Implemented,
    Rejected,
    Deferred,
}

pub fn decision_complete(outcome: Outcome, reason: bool, evidence: bool, revisit: bool) -> bool {
    reason && evidence && (outcome != Outcome::Deferred || revisit)
}

pub fn immutable_identity_valid(length: usize, hexadecimal: bool) -> bool {
    length == 64 && hexadecimal
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstallationStage {
    Policy,
    Runtime,
    Adapter,
}

pub fn installation_stage(policy_applied: bool, runtime_installed: bool) -> InstallationStage {
    if !policy_applied {
        InstallationStage::Policy
    } else if !runtime_installed {
        InstallationStage::Runtime
    } else {
        InstallationStage::Adapter
    }
}

pub fn composition_candidate(
    shared: usize,
    first: usize,
    second: usize,
    minimum: usize,
    percent: usize,
) -> bool {
    shared <= first
        && shared <= second
        && minimum >= 2
        && (1..=100).contains(&percent)
        && shared >= minimum
        && (shared as u128) * 100 >= (first.min(second) as u128) * (percent as u128)
}

#[derive(Clone, Copy)]
pub struct Maintenance {
    pub catalog_current: bool,
    pub policy_current: bool,
    pub observations_preserved: bool,
    pub notes_current: bool,
    pub review_complete: bool,
    pub new_observations: usize,
    pub threshold: usize,
}

pub fn maintenance_current(value: Maintenance) -> bool {
    value.catalog_current
        && value.policy_current
        && value.observations_preserved
        && value.notes_current
        && value.review_complete
        && value.threshold > 0
        && value.new_observations < value.threshold
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Completion {
    Allow,
    RequestMaintenance,
    EndIncomplete,
}

pub fn completion(current: bool, already_continued: bool) -> Completion {
    if current {
        Completion::Allow
    } else if already_continued {
        Completion::EndIncomplete
    } else {
        Completion::RequestMaintenance
    }
}

#[cfg(kani)]
#[kani::proof]
fn adapter_activation_requires_policy_and_installed_runtime() {
    let policy: bool = kani::any();
    let runtime: bool = kani::any();
    let stage = installation_stage(policy, runtime);
    assert!(stage != InstallationStage::Adapter || (policy && runtime));
    assert!(stage != InstallationStage::Runtime || (policy && !runtime));
    assert!(stage != InstallationStage::Policy || !policy);
    kani::cover!(stage == InstallationStage::Policy);
    kani::cover!(stage == InstallationStage::Runtime);
    kani::cover!(stage == InstallationStage::Adapter);
}

#[cfg(kani)]
#[kani::proof]
fn immutable_report_identity_requires_exact_digest_shape() {
    let length: usize = kani::any();
    let hexadecimal: bool = kani::any();
    let accepted = immutable_identity_valid(length, hexadecimal);
    assert!(!accepted || (length == 64 && hexadecimal));
    assert!(length != 64 || !hexadecimal || accepted);
    kani::cover!(accepted);
    kani::cover!(!accepted);
}

#[cfg(kani)]
#[kani::proof]
fn incomplete_completion_is_refused_and_automatic_continuation_is_bounded() {
    let current: bool = kani::any();
    let continued: bool = kani::any();
    let action = completion(current, continued);
    assert_eq!(action == Completion::Allow, current);
    assert!(!continued || action != Completion::RequestMaintenance);
    if action == Completion::RequestMaintenance {
        assert_eq!(completion(false, true), Completion::EndIncomplete);
    }
    kani::cover!(action == Completion::Allow);
    kani::cover!(action == Completion::RequestMaintenance);
    kani::cover!(action == Completion::EndIncomplete);
}

#[cfg(kani)]
#[kani::proof]
fn dispositions_require_evidence_and_defined_follow_up() {
    let choice: u8 = kani::any();
    kani::assume(choice < 4);
    let outcome = match choice {
        0 => Outcome::Keep,
        1 => Outcome::Implemented,
        2 => Outcome::Rejected,
        _ => Outcome::Deferred,
    };
    let reason: bool = kani::any();
    let evidence: bool = kani::any();
    let revisit: bool = kani::any();
    let result = decision_complete(outcome, reason, evidence, revisit);
    assert!(!result || (reason && evidence));
    assert!(outcome != Outcome::Deferred || !result || revisit);
    assert!(!(reason && evidence && (outcome != Outcome::Deferred || revisit)) || result);
    kani::cover!(result);
    kani::cover!(!result);
}

#[cfg(kani)]
#[kani::proof]
fn composition_respects_sample_and_ratio_without_overflow() {
    let shared: usize = kani::any();
    let first: usize = kani::any();
    let second: usize = kani::any();
    let minimum: usize = kani::any();
    let percent: usize = kani::any();
    let result = composition_candidate(shared, first, second, minimum, percent);
    assert!(
        !result
            || (shared <= first
                && shared <= second
                && shared >= minimum
                && minimum >= 2
                && (1..=100).contains(&percent))
    );
    if result {
        assert!((shared as u128) * 100 >= (first.min(second) as u128) * (percent as u128));
    }
    kani::cover!(result);
    kani::cover!(!result);
}

#[cfg(kani)]
#[kani::proof]
fn stale_or_unprocessed_maintenance_never_completes() {
    let value = Maintenance {
        catalog_current: kani::any(),
        policy_current: kani::any(),
        observations_preserved: kani::any(),
        notes_current: kani::any(),
        review_complete: kani::any(),
        new_observations: kani::any(),
        threshold: kani::any(),
    };
    let result = maintenance_current(value);
    assert!(
        !result
            || (value.catalog_current
                && value.policy_current
                && value.observations_preserved
                && value.notes_current
                && value.review_complete)
    );
    assert!(!result || (value.threshold > 0 && value.new_observations < value.threshold));
    kani::cover!(result);
    kani::cover!(!result);
}
