#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(kani), derive(clap::ValueEnum))]
pub enum Generation {
    Local,
    Coderabbit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operation {
    Create,
    Edit,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    CreateDraft,
    CreateReady,
    Edit,
    Ready,
}

/// The issue prerequisite established before a PR operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IssueGate {
    NotRequired,
    Linked,
    Missing,
}

/// Apply personal issue rules only to owned repositories that are not forks.
pub fn issue_gate(personal_owner: bool, fork: bool, checked_issue: bool) -> IssueGate {
    if !personal_owner || fork {
        IssueGate::NotRequired
    } else if checked_issue {
        IssueGate::Linked
    } else {
        IssueGate::Missing
    }
}

/// Keep a nonzero reserve before issuing further GitHub operations.
pub fn api_quota_available(remaining: u64, reserve: u64) -> bool {
    reserve > 0 && remaining >= reserve
}

pub fn coderabbit_body_marker_allowed(request: bool, summary: bool, ignore: bool) -> bool {
    ignore || request && summary
}

/// The head commit's reported checks; an empty report is not a pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Checks {
    Passing,
    Incomplete,
}

pub fn checks_state(total: usize, unfinished: usize, failed: usize) -> Checks {
    if total > 0 && unfinished == 0 && failed == 0 {
        Checks::Passing
    } else {
        Checks::Incomplete
    }
}

/// Why a PR body excludes CodeRabbit's automatic review.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Exclusion {
    None,
    /// A standing exclusion chosen for the PR.
    Owner,
    /// An exclusion that lasts only while the owner pauses PR reviews.
    Pause,
}

pub fn exclusion(ignore: bool, pause_marker: bool) -> Exclusion {
    match (ignore, pause_marker) {
        (false, _) => Exclusion::None,
        (true, false) => Exclusion::Owner,
        (true, true) => Exclusion::Pause,
    }
}

/// Whether an operation asks CodeRabbit for a PR review or generation.
/// An edit requests a review when it removes the exclusion of a ready PR.
pub fn requests_review(
    operation: Operation,
    generation: Generation,
    draft: bool,
    was_excluded: bool,
    excluded: bool,
) -> bool {
    generation == Generation::Coderabbit
        || match operation {
            Operation::Create => false,
            Operation::Edit => !draft && was_excluded && !excluded,
            Operation::Ready => !excluded,
        }
}

/// Whether an operation during a PR pause keeps the review owed after resumption.
/// A PR becomes ready, or stays ready, only with the exclusion made for the pause.
pub fn keeps_pause_exclusion(
    operation: Operation,
    draft: bool,
    was: Exclusion,
    now: Exclusion,
) -> bool {
    match operation {
        Operation::Create => true,
        Operation::Edit => draft || was != Exclusion::Pause || now == Exclusion::Pause,
        Operation::Ready => now == Exclusion::Pause,
    }
}

/// Refuse every review request while the owner pauses CodeRabbit PR reviews.
pub fn review_request_allowed(paused: bool, request: bool) -> bool {
    !paused || !request
}

/// The CodeRabbit PR review a published PR still owes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReviewRequirement {
    Required,
    /// Required, but the body still carries the exclusion added for the pause.
    Resumed,
    Excluded,
    Paused,
}

pub fn review_requirement(paused: bool, exclusion: Exclusion) -> ReviewRequirement {
    match (paused, exclusion) {
        (true, _) => ReviewRequirement::Paused,
        (false, Exclusion::None) => ReviewRequirement::Required,
        (false, Exclusion::Pause) => ReviewRequirement::Resumed,
        (false, Exclusion::Owner) => ReviewRequirement::Excluded,
    }
}

pub fn plan(
    operation: Operation,
    generation: Generation,
    validated: bool,
    draft: bool,
    issue: IssueGate,
    checks: Checks,
) -> Option<Effect> {
    if !validated || issue == IssueGate::Missing {
        return None;
    }
    match operation {
        Operation::Create => Some(if generation == Generation::Local || draft {
            Effect::CreateDraft
        } else {
            Effect::CreateReady
        }),
        Operation::Edit => Some(Effect::Edit),
        Operation::Ready => (draft && checks == Checks::Passing).then_some(Effect::Ready),
    }
}

#[cfg(kani)]
fn inputs() -> (Operation, Generation, bool, bool, IssueGate, Checks) {
    let choice: u8 = kani::any();
    kani::assume(choice < 3);
    (
        match choice {
            0 => Operation::Create,
            1 => Operation::Edit,
            _ => Operation::Ready,
        },
        if kani::any() {
            Generation::Local
        } else {
            Generation::Coderabbit
        },
        kani::any(),
        kani::any(),
        issue_gate(kani::any(), kani::any(), kani::any()),
        checks_state(kani::any(), kani::any(), kani::any()),
    )
}

#[cfg(kani)]
#[kani::proof]
fn rejected_documents_never_produce_a_pr_mutation() {
    let (operation, generation, validated, draft, issue, checks) = inputs();
    let result = plan(operation, generation, validated, draft, issue, checks);
    assert!(validated || result.is_none());
    assert_eq!(
        result.is_some(),
        validated
            && issue != IssueGate::Missing
            && (operation != Operation::Ready || draft && checks == Checks::Passing)
    );
    kani::cover!(result.is_some());
    kani::cover!(result.is_none());
}

#[cfg(kani)]
#[kani::proof]
fn local_creation_always_starts_as_draft() {
    let (operation, generation, validated, draft, issue, checks) = inputs();
    let result = plan(operation, generation, validated, draft, issue, checks);
    if operation == Operation::Create && validated && issue != IssueGate::Missing {
        assert_eq!(
            result == Some(Effect::CreateDraft),
            generation == Generation::Local || draft
        );
        assert_eq!(
            result == Some(Effect::CreateReady),
            generation == Generation::Coderabbit && !draft
        );
    }
    kani::cover!(result == Some(Effect::CreateDraft));
    kani::cover!(result == Some(Effect::CreateReady));
}

#[cfg(kani)]
#[kani::proof]
fn ready_requires_a_validated_draft_transition() {
    let (operation, generation, validated, draft, issue, checks) = inputs();
    let result = plan(operation, generation, validated, draft, issue, checks);
    assert_eq!(
        result == Some(Effect::Ready),
        operation == Operation::Ready
            && validated
            && draft
            && issue != IssueGate::Missing
            && checks == Checks::Passing
    );
    kani::cover!(result == Some(Effect::Ready));
    kani::cover!(result.is_none());
    kani::cover!(
        result.is_none()
            && operation == Operation::Ready
            && validated
            && draft
            && issue != IssueGate::Missing
    );
}

#[cfg(kani)]
#[kani::proof]
fn ready_requires_every_reported_check_to_pass() {
    let total: usize = kani::any();
    let unfinished: usize = kani::any();
    let failed: usize = kani::any();
    let state = checks_state(total, unfinished, failed);
    assert_eq!(
        state == Checks::Passing,
        total != 0 && unfinished == 0 && failed == 0
    );
    kani::cover!(state == Checks::Passing);
    kani::cover!(state == Checks::Incomplete && total == 0);
    kani::cover!(state == Checks::Incomplete && unfinished != 0);
    kani::cover!(state == Checks::Incomplete && failed != 0);
}

#[cfg(kani)]
#[kani::proof]
fn personal_issue_prerequisites_cannot_be_skipped_by_any_generation_mode() {
    let personal_owner: bool = kani::any();
    let fork: bool = kani::any();
    let checked_issue: bool = kani::any();
    let gate = issue_gate(personal_owner, fork, checked_issue);
    assert_eq!(
        gate == IssueGate::Missing,
        personal_owner && !fork && !checked_issue
    );
    assert_eq!(gate == IssueGate::NotRequired, !personal_owner || fork);
    let (operation, generation, validated, draft, _, checks) = inputs();
    let result = plan(operation, generation, validated, draft, gate, checks);
    assert!(gate != IssueGate::Missing || result.is_none());
    kani::cover!(result.is_some() && gate == IssueGate::Linked);
    kani::cover!(result.is_some() && gate == IssueGate::NotRequired);
    kani::cover!(result.is_none() && gate == IssueGate::Missing);
}

#[cfg(kani)]
#[kani::proof]
fn coderabbit_exclusion_does_not_authorize_unfinished_generation() {
    let request: bool = kani::any();
    let summary: bool = kani::any();
    let ignore: bool = kani::any();
    let accepted = coderabbit_body_marker_allowed(request, summary, ignore);
    assert_eq!(accepted, ignore || request && summary);
    assert!(request || ignore || !accepted);
    assert!(summary || ignore || !accepted);
    kani::cover!(accepted && !request && ignore);
    kani::cover!(!accepted && !request && summary && !ignore);
    kani::cover!(!accepted && !summary && !ignore);
}

#[cfg(kani)]
#[kani::proof]
fn api_quota_requires_a_nonzero_reserve_and_sufficient_remaining_requests() {
    let remaining: u64 = kani::any();
    let reserve: u64 = kani::any();
    let accepted = api_quota_available(remaining, reserve);
    assert!(!accepted || reserve > 0 && remaining >= reserve);
    assert_eq!(accepted, reserve != 0 && remaining >= reserve);
    kani::cover!(accepted);
    kani::cover!(!accepted && remaining < reserve);
    kani::cover!(!accepted && reserve == 0);
}

#[cfg(kani)]
#[kani::proof]
fn paused_pr_reviews_are_never_requested() {
    let (operation, generation, _, draft, _, _) = inputs();
    let paused: bool = kani::any();
    let was_excluded: bool = kani::any();
    let excluded: bool = kani::any();
    let request = requests_review(operation, generation, draft, was_excluded, excluded);
    let accepted = review_request_allowed(paused, request);
    assert_eq!(accepted, !paused || !request);
    assert!(!paused || generation == Generation::Local || !accepted);
    assert!(!paused || operation != Operation::Ready || excluded || !accepted);
    assert!(paused || accepted);
    kani::cover!(paused && accepted && operation == Operation::Ready);
    kani::cover!(paused && !accepted && generation == Generation::Coderabbit);
    kani::cover!(paused && !accepted && generation == Generation::Local);
}

#[cfg(kani)]
#[kani::proof]
fn a_paused_edit_never_removes_the_exclusion_of_a_ready_pr() {
    let paused: bool = kani::any();
    let draft: bool = kani::any();
    let was_excluded: bool = kani::any();
    let excluded: bool = kani::any();
    let accepted = review_request_allowed(
        paused,
        requests_review(
            Operation::Edit,
            Generation::Local,
            draft,
            was_excluded,
            excluded,
        ),
    );
    assert_eq!(accepted, !paused || draft || !was_excluded || excluded);
    kani::cover!(paused && !accepted);
    kani::cover!(paused && accepted && draft && was_excluded && !excluded);
    kani::cover!(!paused && accepted && !draft && was_excluded && !excluded);
}

#[cfg(kani)]
#[kani::proof]
fn a_pr_ready_during_the_pause_keeps_its_pause_exclusion() {
    let (operation, _, _, draft, _, _) = inputs();
    let was = exclusion(kani::any(), kani::any());
    let now = exclusion(kani::any(), kani::any());
    let kept = keeps_pause_exclusion(operation, draft, was, now);
    assert!(operation != Operation::Ready || kept == (now == Exclusion::Pause));
    assert!(
        operation != Operation::Edit
            || draft
            || was != Exclusion::Pause
            || kept == (now == Exclusion::Pause)
    );
    assert!(
        !kept
            || operation != Operation::Ready
            || review_requirement(false, now) == ReviewRequirement::Resumed
    );
    kani::cover!(!kept && operation == Operation::Ready && now == Exclusion::Owner);
    kani::cover!(!kept && operation == Operation::Edit && now == Exclusion::Owner);
    kani::cover!(kept && operation == Operation::Edit && draft && now == Exclusion::None);
    kani::cover!(kept && operation == Operation::Ready);
}

#[cfg(kani)]
#[kani::proof]
fn pr_review_requirement_returns_after_resumption() {
    let paused: bool = kani::any();
    let ignore: bool = kani::any();
    let marker: bool = kani::any();
    let kind = exclusion(ignore, marker);
    let requirement = review_requirement(paused, kind);
    assert_eq!(requirement == ReviewRequirement::Paused, paused);
    assert_eq!(
        matches!(
            requirement,
            ReviewRequirement::Required | ReviewRequirement::Resumed
        ),
        !paused && (!ignore || marker)
    );
    assert_eq!(
        requirement == ReviewRequirement::Excluded,
        !paused && ignore && !marker
    );
    assert!(kind != Exclusion::Pause || requirement != ReviewRequirement::Excluded);
    kani::cover!(requirement == ReviewRequirement::Required);
    kani::cover!(requirement == ReviewRequirement::Resumed);
    kani::cover!(requirement == ReviewRequirement::Paused && kind == Exclusion::Pause);
    kani::cover!(requirement == ReviewRequirement::Excluded);
}
