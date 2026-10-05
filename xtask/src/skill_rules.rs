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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tracked {
    Current,
    Missing,
    Stale,
    Incomplete,
}

pub fn tracked_decision(present: bool, bound: bool, complete: bool) -> Tracked {
    if !present {
        Tracked::Missing
    } else if !bound {
        Tracked::Stale
    } else if !complete {
        Tracked::Incomplete
    } else {
        Tracked::Current
    }
}

pub fn adopted(bound: bool, outcome: Outcome) -> bool {
    bound && outcome != Outcome::Deferred
}

pub fn triage_complete(unadopted: usize, review_verified: bool) -> bool {
    unadopted == 0 || review_verified
}

/// Settles maintenance by the recorded analysis or by a current analysis with nothing to triage.
pub fn gate_complete(recorded_current: bool, open_findings: usize) -> bool {
    recorded_current || open_findings == 0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Within,
    Settled,
    Missing,
    Incomplete,
    Unexpected,
}

pub fn size_disposition(over_limit: bool, recorded: bool, complete: bool) -> Sizing {
    match (over_limit, recorded, complete) {
        (false, false, _) => Sizing::Within,
        (false, true, _) => Sizing::Unexpected,
        (true, false, _) => Sizing::Missing,
        (true, true, false) => Sizing::Incomplete,
        (true, true, true) => Sizing::Settled,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiffBase {
    Whole,
    Assessed,
    Unavailable,
}

/// Shows a diff only from a commit that holds the content the previous decision assessed.
pub fn diff_base(previous: bool, assessed_commit: bool) -> DiffBase {
    match (previous, assessed_commit) {
        (false, _) => DiffBase::Whole,
        (true, true) => DiffBase::Assessed,
        (true, false) => DiffBase::Unavailable,
    }
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
    let outcome = any_outcome();
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

#[cfg(kani)]
fn any_outcome() -> Outcome {
    let choice: u8 = kani::any();
    kani::assume(choice < 4);
    match choice {
        0 => Outcome::Keep,
        1 => Outcome::Implemented,
        2 => Outcome::Rejected,
        _ => Outcome::Deferred,
    }
}

#[cfg(kani)]
#[kani::proof]
fn tracked_decisions_pass_only_when_present_bound_and_complete() {
    let present: bool = kani::any();
    let bound: bool = kani::any();
    let complete: bool = kani::any();
    let state = tracked_decision(present, bound, complete);
    assert_eq!(state == Tracked::Current, present && bound && complete);
    assert!(state != Tracked::Missing || !present);
    assert!(state != Tracked::Stale || (present && !bound));
    assert!(state != Tracked::Incomplete || (present && bound && !complete));
    kani::cover!(state == Tracked::Current);
    kani::cover!(state == Tracked::Missing);
    kani::cover!(state == Tracked::Stale);
    kani::cover!(state == Tracked::Incomplete);
}

#[cfg(kani)]
#[kani::proof]
fn only_current_settled_decisions_are_adopted() {
    let bound: bool = kani::any();
    let outcome = any_outcome();
    let result = adopted(bound, outcome);
    assert!(!result || bound);
    assert!(!result || outcome != Outcome::Deferred);
    assert!(!bound || outcome == Outcome::Deferred || result);
    kani::cover!(result);
    kani::cover!(!result);
}

#[cfg(kani)]
#[kani::proof]
fn triage_is_skipped_only_when_every_finding_is_adopted() {
    let unadopted: usize = kani::any();
    let verified: bool = kani::any();
    let result = triage_complete(unadopted, verified);
    assert!(!result || unadopted == 0 || verified);
    assert!(unadopted == 0 || verified || !result);
    kani::cover!(result && !verified);
    kani::cover!(!result);
}

#[cfg(kani)]
#[kani::proof]
fn oversized_skills_need_a_complete_size_disposition() {
    let over_limit: bool = kani::any();
    let recorded: bool = kani::any();
    let complete: bool = kani::any();
    let state = size_disposition(over_limit, recorded, complete);
    let accepted = matches!(state, Sizing::Within | Sizing::Settled);
    assert_eq!(accepted, recorded == over_limit && (!recorded || complete));
    assert!(state != Sizing::Missing || (over_limit && !recorded));
    assert!(state != Sizing::Unexpected || (!over_limit && recorded));
    assert!(state != Sizing::Incomplete || (over_limit && recorded && !complete));
    kani::cover!(state == Sizing::Within);
    kani::cover!(state == Sizing::Settled);
    kani::cover!(state == Sizing::Missing);
    kani::cover!(state == Sizing::Incomplete);
    kani::cover!(state == Sizing::Unexpected);
}

#[cfg(kani)]
#[kani::proof]
fn the_gate_completes_only_when_nothing_remains_to_triage() {
    let recorded: bool = kani::any();
    let open: usize = kani::any();
    let result = gate_complete(recorded, open);
    assert!(!result || recorded || open == 0);
    assert!(result || (!recorded && open > 0));
    kani::cover!(result && !recorded);
    kani::cover!(!result);
}

#[cfg(kani)]
#[kani::proof]
fn the_decision_diff_starts_only_from_the_assessed_content() {
    let previous: bool = kani::any();
    let assessed: bool = kani::any();
    let base = diff_base(previous, assessed);
    assert!(base != DiffBase::Assessed || (previous && assessed));
    assert!(base != DiffBase::Unavailable || (previous && !assessed));
    assert_eq!(base == DiffBase::Whole, !previous);
    kani::cover!(base == DiffBase::Whole);
    kani::cover!(base == DiffBase::Assessed);
    kani::cover!(base == DiffBase::Unavailable);
}
