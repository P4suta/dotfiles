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

pub fn plan(
    operation: Operation,
    generation: Generation,
    validated: bool,
    draft: bool,
    issue: IssueGate,
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
        Operation::Ready => draft.then_some(Effect::Ready),
    }
}

#[cfg(kani)]
fn inputs() -> (Operation, Generation, bool, bool, IssueGate) {
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
    )
}

#[cfg(kani)]
#[kani::proof]
fn rejected_documents_never_produce_a_pr_mutation() {
    let (operation, generation, validated, draft, issue) = inputs();
    let result = plan(operation, generation, validated, draft, issue);
    assert!(validated || result.is_none());
    assert_eq!(
        result.is_some(),
        validated && issue != IssueGate::Missing && (operation != Operation::Ready || draft)
    );
    kani::cover!(result.is_some());
    kani::cover!(result.is_none());
}

#[cfg(kani)]
#[kani::proof]
fn local_creation_always_starts_as_draft() {
    let (operation, generation, validated, draft, issue) = inputs();
    let result = plan(operation, generation, validated, draft, issue);
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
    let (operation, generation, validated, draft, issue) = inputs();
    let result = plan(operation, generation, validated, draft, issue);
    assert_eq!(
        result == Some(Effect::Ready),
        operation == Operation::Ready && validated && draft && issue != IssueGate::Missing
    );
    kani::cover!(result == Some(Effect::Ready));
    kani::cover!(result.is_none());
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
    let (operation, generation, validated, draft, _) = inputs();
    let result = plan(operation, generation, validated, draft, gate);
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
