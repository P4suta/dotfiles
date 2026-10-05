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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IssueGate {
    NotRequired,
    Linked,
    Missing,
}

/// Apply personal issue rules only to owned non-fork repositories.
pub fn issue_gate(personal_owner: bool, fork: bool, checked_issue: bool) -> IssueGate {
    if !personal_owner || fork {
        IssueGate::NotRequired
    } else if checked_issue {
        IssueGate::Linked
    } else {
        IssueGate::Missing
    }
}

pub fn api_quota_available(remaining: u64, reserve: u64) -> bool {
    reserve > 0 && remaining >= reserve
}

pub fn coderabbit_body_marker_allowed(request: bool, summary: bool, ignore: bool) -> bool {
    ignore || request && summary
}

/// The head commit's reported checks, where an empty report never passes.
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

/// The first unmet condition that refuses an operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blocker {
    Unvalidated,
    MissingIssue,
    NotDraft,
    UnfinishedChecks,
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

/// The checks reported for the head commit while a merge waits for them.
/// An empty report counts as pending.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeadChecks {
    Passing,
    Pending,
    Failed,
}

pub fn head_checks(total: usize, unfinished: usize, failed: usize) -> HeadChecks {
    if failed > 0 {
        HeadChecks::Failed
    } else if total == 0 || unfinished > 0 {
        HeadChecks::Pending
    } else {
        HeadChecks::Passing
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lifecycle {
    Draft,
    Ready,
    Merged,
    Closed,
}

/// What GitHub reports about one PR of a merge, read in one response so the checks belong to its head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MergeState {
    pub lifecycle: Lifecycle,
    pub conflict: bool,
    /// The base branch has commits the head lacks.
    pub behind: bool,
    pub checks: HeadChecks,
    /// GitHub reports that the PR meets all merge rules of the base.
    pub clean: bool,
    pub exclusion: Exclusion,
}

/// Why a merge stops before it lands a PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    Closed,
    Conflict,
    FailedChecks,
    /// The head still needs a CodeRabbit review.
    /// Only coderabbit-review can establish that the review's findings have a resolution.
    Review,
}

/// The next step of a merge for one PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Done,
    /// Merge the base into an independent PR's head with GitHub's branch update, which GitHub signs.
    Update,
    /// Rebase a stacked PR onto its parent with the stack tooling.
    /// A stack needs a linear history.
    Restack,
    Wait,
    /// Add the CodeRabbit pause lines before a draft becomes ready during the owner's pause.
    Exclude,
    Ready,
    /// Merge the head whose checks the tool read.
    Merge,
    Stop(Stop),
}

pub fn merge_step(state: MergeState, stacked: bool, paused: bool) -> Step {
    match state.lifecycle {
        Lifecycle::Merged => return Step::Done,
        Lifecycle::Closed => return Step::Stop(Stop::Closed),
        Lifecycle::Draft | Lifecycle::Ready => {}
    }
    if state.conflict {
        return Step::Stop(Stop::Conflict);
    }
    if state.behind {
        return if stacked { Step::Restack } else { Step::Update };
    }
    match state.checks {
        HeadChecks::Failed => return Step::Stop(Stop::FailedChecks),
        HeadChecks::Pending => return Step::Wait,
        HeadChecks::Passing => {}
    }
    if state.lifecycle == Lifecycle::Draft {
        return if paused && state.exclusion != Exclusion::Pause {
            Step::Exclude
        } else {
            Step::Ready
        };
    }
    match review_requirement(paused, state.exclusion) {
        ReviewRequirement::Required | ReviewRequirement::Resumed => Step::Stop(Stop::Review),
        ReviewRequirement::Paused | ReviewRequirement::Excluded if !state.clean => Step::Wait,
        ReviewRequirement::Paused | ReviewRequirement::Excluded => Step::Merge,
    }
}

/// What a new branch shares with another open PR's branch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dependency {
    Commits,
    Paths,
}

/// A branch that shares commits with another open PR opens only stacked on that PR.
/// A branch that shares touched paths with another open PR also opens stacked on it, unless the branch carries a declaration of independence.
pub fn dependency(
    shared_commits: bool,
    shared_paths: bool,
    independent: bool,
) -> Option<Dependency> {
    if shared_commits {
        Some(Dependency::Commits)
    } else if shared_paths && !independent {
        Some(Dependency::Paths)
    } else {
        None
    }
}

pub fn plan(
    operation: Operation,
    generation: Generation,
    validated: bool,
    draft: bool,
    issue: IssueGate,
    checks: Checks,
) -> Result<Effect, Blocker> {
    if !validated {
        return Err(Blocker::Unvalidated);
    }
    if issue == IssueGate::Missing {
        return Err(Blocker::MissingIssue);
    }
    match operation {
        Operation::Create => Ok(if generation == Generation::Local || draft {
            Effect::CreateDraft
        } else {
            Effect::CreateReady
        }),
        Operation::Edit => Ok(Effect::Edit),
        Operation::Ready if !draft => Err(Blocker::NotDraft),
        Operation::Ready if checks != Checks::Passing => Err(Blocker::UnfinishedChecks),
        Operation::Ready => Ok(Effect::Ready),
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
    assert!(validated || result.is_err());
    assert_eq!(
        result.is_ok(),
        validated
            && issue != IssueGate::Missing
            && (operation != Operation::Ready || draft && checks == Checks::Passing)
    );
    kani::cover!(result.is_ok());
    kani::cover!(result.is_err());
}

#[cfg(kani)]
#[kani::proof]
fn local_creation_always_starts_as_draft() {
    let (operation, generation, validated, draft, issue, checks) = inputs();
    let result = plan(operation, generation, validated, draft, issue, checks);
    if operation == Operation::Create && validated && issue != IssueGate::Missing {
        assert_eq!(
            result == Ok(Effect::CreateDraft),
            generation == Generation::Local || draft
        );
        assert_eq!(
            result == Ok(Effect::CreateReady),
            generation == Generation::Coderabbit && !draft
        );
    }
    kani::cover!(result == Ok(Effect::CreateDraft));
    kani::cover!(result == Ok(Effect::CreateReady));
}

#[cfg(kani)]
#[kani::proof]
fn ready_requires_a_validated_draft_transition() {
    let (operation, generation, validated, draft, issue, checks) = inputs();
    let result = plan(operation, generation, validated, draft, issue, checks);
    assert_eq!(
        result == Ok(Effect::Ready),
        operation == Operation::Ready
            && validated
            && draft
            && issue != IssueGate::Missing
            && checks == Checks::Passing
    );
    kani::cover!(result == Ok(Effect::Ready));
    kani::cover!(result.is_err());
    kani::cover!(
        result.is_err()
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
    assert!(gate != IssueGate::Missing || result.is_err());
    kani::cover!(result.is_ok() && gate == IssueGate::Linked);
    kani::cover!(result.is_ok() && gate == IssueGate::NotRequired);
    kani::cover!(result.is_err() && gate == IssueGate::Missing);
}

#[cfg(kani)]
#[kani::proof]
fn a_refusal_names_the_first_unmet_condition() {
    let (operation, generation, validated, draft, issue, checks) = inputs();
    let result = plan(operation, generation, validated, draft, issue, checks);
    let expected = if !validated {
        Some(Blocker::Unvalidated)
    } else if issue == IssueGate::Missing {
        Some(Blocker::MissingIssue)
    } else if operation == Operation::Ready && !draft {
        Some(Blocker::NotDraft)
    } else if operation == Operation::Ready && checks != Checks::Passing {
        Some(Blocker::UnfinishedChecks)
    } else {
        None
    };
    assert_eq!(result.err(), expected);
    assert!(
        operation == Operation::Ready
            || !matches!(result, Err(Blocker::NotDraft | Blocker::UnfinishedChecks))
    );
    kani::cover!(result == Err(Blocker::Unvalidated));
    kani::cover!(result == Err(Blocker::MissingIssue));
    kani::cover!(result == Err(Blocker::NotDraft));
    kani::cover!(result == Err(Blocker::UnfinishedChecks));
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

#[cfg(kani)]
fn merge_state() -> MergeState {
    MergeState {
        lifecycle: match kani::any::<u8>() {
            0 => Lifecycle::Draft,
            1 => Lifecycle::Ready,
            2 => Lifecycle::Merged,
            _ => Lifecycle::Closed,
        },
        conflict: kani::any(),
        behind: kani::any(),
        checks: head_checks(kani::any(), kani::any(), kani::any()),
        clean: kani::any(),
        exclusion: exclusion(kani::any(), kani::any()),
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_pr_merges_only_ready_current_passing_clean_and_without_an_owed_review() {
    let state = merge_state();
    let stacked: bool = kani::any();
    let paused: bool = kani::any();
    let step = merge_step(state, stacked, paused);
    assert_eq!(
        step == Step::Merge,
        state.lifecycle == Lifecycle::Ready
            && !state.conflict
            && !state.behind
            && state.checks == HeadChecks::Passing
            && state.clean
            && matches!(
                review_requirement(paused, state.exclusion),
                ReviewRequirement::Paused | ReviewRequirement::Excluded
            )
    );
    assert!(
        state.checks != HeadChecks::Failed
            || state.behind
            || !matches!(step, Step::Merge | Step::Ready | Step::Wait)
    );
    kani::cover!(step == Step::Merge && paused);
    kani::cover!(step == Step::Merge && !paused);
    kani::cover!(step == Step::Stop(Stop::Review));
    kani::cover!(step == Step::Stop(Stop::FailedChecks));
    kani::cover!(step == Step::Stop(Stop::Conflict));
    kani::cover!(step == Step::Wait && state.checks == HeadChecks::Passing);
}

#[cfg(kani)]
#[kani::proof]
fn a_behind_stacked_pr_is_restacked_and_an_independent_one_updated_by_merge() {
    let state = merge_state();
    let stacked: bool = kani::any();
    let step = merge_step(state, stacked, kani::any());
    let open = matches!(state.lifecycle, Lifecycle::Draft | Lifecycle::Ready);
    assert_eq!(
        step == Step::Restack,
        open && !state.conflict && state.behind && stacked
    );
    assert_eq!(
        step == Step::Update,
        open && !state.conflict && state.behind && !stacked
    );
    assert!(!stacked || step != Step::Update);
    kani::cover!(step == Step::Restack);
    kani::cover!(step == Step::Update);
}

#[cfg(kani)]
#[kani::proof]
fn a_draft_becomes_ready_during_the_pause_only_with_the_pause_lines() {
    let state = merge_state();
    let paused: bool = kani::any();
    let step = merge_step(state, kani::any(), paused);
    if step == Step::Ready {
        assert!(state.lifecycle == Lifecycle::Draft && state.checks == HeadChecks::Passing);
        assert!(!paused || state.exclusion == Exclusion::Pause);
        assert!(
            keeps_pause_exclusion(Operation::Ready, true, state.exclusion, state.exclusion)
                || !paused
        );
    }
    assert_eq!(
        step == Step::Exclude,
        state.lifecycle == Lifecycle::Draft
            && !state.conflict
            && !state.behind
            && state.checks == HeadChecks::Passing
            && paused
            && state.exclusion != Exclusion::Pause
    );
    kani::cover!(step == Step::Ready && paused);
    kani::cover!(step == Step::Ready && !paused);
    kani::cover!(step == Step::Exclude);
}

#[cfg(kani)]
#[kani::proof]
fn a_branch_sharing_work_with_another_open_pr_opens_only_on_top_of_it() {
    let shared_commits: bool = kani::any();
    let shared_paths: bool = kani::any();
    let independent: bool = kani::any();
    let found = dependency(shared_commits, shared_paths, independent);
    assert_eq!(
        found.is_some(),
        shared_commits || shared_paths && !independent
    );
    assert_eq!(found == Some(Dependency::Commits), shared_commits);
    assert!(!shared_commits || found.is_some());
    kani::cover!(found == Some(Dependency::Commits) && independent);
    kani::cover!(found == Some(Dependency::Paths));
    kani::cover!(found.is_none() && shared_paths);
    kani::cover!(found.is_none() && !shared_paths);
}

#[cfg(test)]
mod tests {
    use super::{
        Dependency, Exclusion, HeadChecks, Lifecycle, MergeState, Step, Stop, dependency,
        head_checks, merge_step,
    };

    const READY: MergeState = MergeState {
        lifecycle: Lifecycle::Ready,
        conflict: false,
        behind: false,
        checks: HeadChecks::Passing,
        clean: true,
        exclusion: Exclusion::Pause,
    };

    #[test]
    fn an_empty_or_unfinished_check_report_is_pending_and_any_failure_fails() {
        assert_eq!(head_checks(0, 0, 0), HeadChecks::Pending);
        assert_eq!(head_checks(3, 1, 0), HeadChecks::Pending);
        assert_eq!(head_checks(3, 1, 1), HeadChecks::Failed);
        assert_eq!(head_checks(3, 0, 0), HeadChecks::Passing);
    }

    #[test]
    fn each_merge_state_has_one_next_step() {
        let with = |change: fn(&mut MergeState)| {
            let mut state = READY;
            change(&mut state);
            state
        };
        assert_eq!(merge_step(READY, false, true), Step::Merge);
        assert_eq!(merge_step(READY, true, true), Step::Merge);
        let behind = with(|state| state.behind = true);
        assert_eq!(merge_step(behind, false, true), Step::Update);
        assert_eq!(merge_step(behind, true, true), Step::Restack);
        let conflict = with(|state| {
            state.behind = true;
            state.conflict = true;
        });
        assert_eq!(
            merge_step(conflict, false, true),
            Step::Stop(Stop::Conflict)
        );
        let failed = with(|state| state.checks = HeadChecks::Failed);
        assert_eq!(
            merge_step(failed, false, true),
            Step::Stop(Stop::FailedChecks)
        );
        let pending = with(|state| state.checks = HeadChecks::Pending);
        assert_eq!(merge_step(pending, false, true), Step::Wait);
        let blocked = with(|state| state.clean = false);
        assert_eq!(merge_step(blocked, false, true), Step::Wait);
        let draft = with(|state| {
            state.lifecycle = Lifecycle::Draft;
            state.exclusion = Exclusion::None;
        });
        assert_eq!(merge_step(draft, false, true), Step::Exclude);
        assert_eq!(merge_step(draft, false, false), Step::Ready);
        let excluded = with(|state| state.lifecycle = Lifecycle::Draft);
        assert_eq!(merge_step(excluded, false, true), Step::Ready);
        let reviewed = with(|state| state.exclusion = Exclusion::None);
        assert_eq!(merge_step(reviewed, false, false), Step::Stop(Stop::Review));
        assert_eq!(merge_step(READY, false, false), Step::Stop(Stop::Review));
        let standing = with(|state| state.exclusion = Exclusion::Owner);
        assert_eq!(merge_step(standing, false, false), Step::Merge);
        let merged = with(|state| state.lifecycle = Lifecycle::Merged);
        assert_eq!(merge_step(merged, false, true), Step::Done);
        let closed = with(|state| state.lifecycle = Lifecycle::Closed);
        assert_eq!(merge_step(closed, false, true), Step::Stop(Stop::Closed));
    }

    #[test]
    fn shared_commits_or_paths_make_a_branch_dependent() {
        assert_eq!(dependency(true, true, false), Some(Dependency::Commits));
        assert_eq!(dependency(true, false, true), Some(Dependency::Commits));
        assert_eq!(dependency(false, true, false), Some(Dependency::Paths));
        assert_eq!(dependency(false, true, true), None);
        assert_eq!(dependency(false, false, false), None);
    }
}
