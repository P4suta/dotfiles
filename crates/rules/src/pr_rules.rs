#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Generation {
    Local,
    Coderabbit,
}

impl Generation {
    pub const ALL: [Self; 2] = [Self::Local, Self::Coderabbit];

    /// The name a command line gives.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Coderabbit => "coderabbit",
        }
    }

    /// The generation a command line names, or `None` for a name no generation carries.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|generation| generation.key() == key)
    }
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
#[must_use]
pub fn issue_gate(personal_owner: bool, fork: bool, checked_issue: bool) -> IssueGate {
    if !personal_owner || fork {
        IssueGate::NotRequired
    } else if checked_issue {
        IssueGate::Linked
    } else {
        IssueGate::Missing
    }
}

#[must_use]
pub fn api_quota_available(remaining: u64, reserve: u64) -> bool {
    reserve > 0 && remaining >= reserve
}

#[must_use]
pub fn coderabbit_body_marker_allowed(request: bool, summary: bool, ignore: bool) -> bool {
    ignore || request && summary
}

/// The head commit's reported checks, which `pr_ready_rules::complete` decides from its report.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Checks {
    Success,
    Incomplete,
}

/// The first unmet condition that refuses an operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blocker {
    Unvalidated,
    MissingIssue,
    NotDraft,
    UnfinishedChecks,
}

/// Why a PR body excludes `CodeRabbit`'s automatic review.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Exclusion {
    None,
    /// A standing exclusion chosen for the PR.
    Owner,
    /// An exclusion that lasts only while the owner pauses PR reviews.
    Pause,
}

#[must_use]
pub fn exclusion(ignore: bool, pause_marker: bool) -> Exclusion {
    match (ignore, pause_marker) {
        (false, _) => Exclusion::None,
        (true, false) => Exclusion::Owner,
        (true, true) => Exclusion::Pause,
    }
}

/// Whether an operation asks `CodeRabbit` for a PR review or generation.
/// An edit requests a review when it removes the exclusion of a ready PR.
#[must_use]
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
#[must_use]
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

/// Refuse every review request while the owner pauses `CodeRabbit` PR reviews.
#[must_use]
pub fn review_request_allowed(paused: bool, request: bool) -> bool {
    !paused || !request
}

/// The `CodeRabbit` PR review a published PR still owes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReviewRequirement {
    Required,
    /// Required, but the body still carries the exclusion added for the pause.
    Resumed,
    Excluded,
    Paused,
}

#[must_use]
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
    Success,
    Pending,
    Failed,
}

#[must_use]
pub fn head_checks(total: usize, unfinished: usize, failed: usize) -> HeadChecks {
    if failed > 0 {
        HeadChecks::Failed
    } else if total == 0 || unfinished > 0 {
        HeadChecks::Pending
    } else {
        HeadChecks::Success
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
    /// The head still needs a `CodeRabbit` review.
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
    /// Add the `CodeRabbit` pause lines before a draft becomes ready during the owner's pause.
    Exclude,
    Ready,
    /// Merge the head whose checks the tool read.
    Merge,
    Stop(Stop),
}

#[must_use]
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
        HeadChecks::Success => {}
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
#[must_use]
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

/// The effect a PR operation may take.
///
/// # Errors
///
/// Returns the blocker that refuses the operation.
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
        Operation::Ready if checks != Checks::Success => Err(Blocker::UnfinishedChecks),
        Operation::Ready => Ok(Effect::Ready),
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const OPERATIONS: [Operation; 3] = [Operation::Create, Operation::Edit, Operation::Ready];
    pub const ISSUES: [IssueGate; 3] = [
        IssueGate::NotRequired,
        IssueGate::Linked,
        IssueGate::Missing,
    ];
    pub const EXCLUSIONS: [Exclusion; 3] = [Exclusion::None, Exclusion::Owner, Exclusion::Pause];
    pub const LIFECYCLES: [Lifecycle; 4] = [
        Lifecycle::Draft,
        Lifecycle::Ready,
        Lifecycle::Merged,
        Lifecycle::Closed,
    ];
    pub const HEAD_CHECKS: [HeadChecks; 3] =
        [HeadChecks::Success, HeadChecks::Pending, HeadChecks::Failed];

    /// A rejected document never mutates a PR, local creation starts as a draft, ready needs a validated draft with passing checks, and a refusal names the first unmet condition.
    pub fn spec_plan(
        operation: Operation,
        generation: Generation,
        validated: bool,
        draft: bool,
        issue: IssueGate,
        checks: Checks,
    ) {
        let expected = if !validated {
            Err(Blocker::Unvalidated)
        } else if issue == IssueGate::Missing {
            Err(Blocker::MissingIssue)
        } else {
            match operation {
                Operation::Create if generation == Generation::Local || draft => {
                    Ok(Effect::CreateDraft)
                }
                Operation::Create => Ok(Effect::CreateReady),
                Operation::Edit => Ok(Effect::Edit),
                Operation::Ready if !draft => Err(Blocker::NotDraft),
                Operation::Ready if checks != Checks::Success => Err(Blocker::UnfinishedChecks),
                Operation::Ready => Ok(Effect::Ready),
            }
        };
        assert_eq!(
            plan(operation, generation, validated, draft, issue, checks),
            expected
        );
    }

    /// Any failure fails a merge, and an empty or unfinished report waits.
    pub fn spec_checks(total: usize, unfinished: usize, failed: usize) {
        let head = head_checks(total, unfinished, failed);
        assert_eq!(head == HeadChecks::Failed, failed != 0);
        assert_eq!(
            head == HeadChecks::Pending,
            failed == 0 && (total == 0 || unfinished != 0)
        );
        assert_eq!(
            head == HeadChecks::Success,
            failed == 0 && total != 0 && unfinished == 0
        );
    }

    /// The flag decisions admit exactly what their rules name.
    pub fn spec_flags(first: bool, second: bool, third: bool) {
        let gate = issue_gate(first, second, third);
        assert_eq!(gate == IssueGate::NotRequired, !first || second);
        assert_eq!(gate == IssueGate::Linked, first && !second && third);
        assert_eq!(gate == IssueGate::Missing, first && !second && !third);
        assert_eq!(
            coderabbit_body_marker_allowed(first, second, third),
            third || first && second
        );
        assert_eq!(review_request_allowed(first, second), !first || !second);
        let kind = exclusion(first, second);
        assert_eq!(kind == Exclusion::None, !first);
        assert_eq!(kind == Exclusion::Owner, first && !second);
        assert_eq!(kind == Exclusion::Pause, first && second);
        assert_eq!(
            dependency(first, second, third),
            if first {
                Some(Dependency::Commits)
            } else if second && !third {
                Some(Dependency::Paths)
            } else {
                None
            }
        );
    }

    pub fn spec_quota(remaining: u64, reserve: u64) {
        assert_eq!(
            api_quota_available(remaining, reserve),
            reserve != 0 && remaining >= reserve
        );
    }

    /// No operation requests a paused PR review, and an edit requests one only by removing the exclusion of a ready PR.
    pub fn spec_review(operation: Operation, generation: Generation, flags: [bool; 4]) {
        let [draft, was_excluded, excluded, paused] = flags;
        let request = requests_review(operation, generation, draft, was_excluded, excluded);
        let expected = generation == Generation::Coderabbit
            || (operation == Operation::Edit && !draft && was_excluded && !excluded)
            || (operation == Operation::Ready && !excluded);
        assert_eq!(request, expected);
        let accepted = review_request_allowed(paused, request);
        assert!(!paused || generation == Generation::Local || !accepted);
        assert!(paused || accepted);
    }

    /// A PR ready during the pause keeps its pause exclusion, and a review comes back once the pause ends.
    pub fn spec_pause(
        operation: Operation,
        draft: bool,
        was: Exclusion,
        now: Exclusion,
        paused: bool,
    ) {
        let kept = keeps_pause_exclusion(operation, draft, was, now);
        let expected = match operation {
            Operation::Create => true,
            Operation::Edit => draft || was != Exclusion::Pause || now == Exclusion::Pause,
            Operation::Ready => now == Exclusion::Pause,
        };
        assert_eq!(kept, expected);
        let requirement = review_requirement(paused, now);
        assert_eq!(requirement == ReviewRequirement::Paused, paused);
        assert_eq!(
            requirement == ReviewRequirement::Required,
            !paused && now == Exclusion::None
        );
        assert_eq!(
            requirement == ReviewRequirement::Resumed,
            !paused && now == Exclusion::Pause
        );
        assert_eq!(
            requirement == ReviewRequirement::Excluded,
            !paused && now == Exclusion::Owner
        );
    }

    /// A PR merges only ready, current, passing, clean, and without an owed review, and each other state takes its own next step.
    pub fn spec_merge(state: MergeState, stacked: bool, paused: bool) {
        let owed = !paused && state.exclusion != Exclusion::Owner;
        let expected = match state.lifecycle {
            Lifecycle::Merged => Step::Done,
            Lifecycle::Closed => Step::Stop(Stop::Closed),
            _ if state.conflict => Step::Stop(Stop::Conflict),
            _ if state.behind && stacked => Step::Restack,
            _ if state.behind => Step::Update,
            _ if state.checks == HeadChecks::Failed => Step::Stop(Stop::FailedChecks),
            _ if state.checks == HeadChecks::Pending => Step::Wait,
            Lifecycle::Draft if paused && state.exclusion != Exclusion::Pause => Step::Exclude,
            Lifecycle::Draft => Step::Ready,
            Lifecycle::Ready if owed => Step::Stop(Stop::Review),
            Lifecycle::Ready if !state.clean => Step::Wait,
            Lifecycle::Ready => Step::Merge,
        };
        assert_eq!(merge_step(state, stacked, paused), expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_pr_operation_takes_its_effect_only_after_every_condition_and_names_the_first_unmet_one() {
        let operation = OPERATIONS[usize::from(kani::any::<u8>() % 3)];
        let generation = Generation::ALL[usize::from(kani::any::<u8>() % 2)];
        let issue = ISSUES[usize::from(kani::any::<u8>() % 3)];
        let checks = if kani::any() {
            Checks::Success
        } else {
            Checks::Incomplete
        };
        let validated: bool = kani::any();
        let draft: bool = kani::any();
        spec_plan(operation, generation, validated, draft, issue, checks);
        let result = plan(operation, generation, validated, draft, issue, checks);
        kani::cover!(result == Ok(Effect::CreateReady));
        kani::cover!(result == Ok(Effect::Ready));
        kani::cover!(result == Err(Blocker::UnfinishedChecks));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn checks_flags_and_quotas_decide_exactly_what_their_rules_name() {
        let total: usize = kani::any();
        let unfinished: usize = kani::any();
        let failed: usize = kani::any();
        spec_checks(total, unfinished, failed);
        let flags: [bool; 3] = [kani::any(), kani::any(), kani::any()];
        let [first, second, third] = flags;
        spec_flags(first, second, third);
        let remaining: u64 = kani::any();
        let reserve: u64 = kani::any();
        spec_quota(remaining, reserve);
        kani::cover!(head_checks(total, unfinished, failed) == HeadChecks::Success);
        kani::cover!(head_checks(total, unfinished, failed) == HeadChecks::Pending && failed == 0);
        kani::cover!(!api_quota_available(remaining, reserve) && reserve > 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn paused_reviews_are_never_requested_and_return_after_resumption() {
        let operation = OPERATIONS[usize::from(kani::any::<u8>() % 3)];
        let generation = Generation::ALL[usize::from(kani::any::<u8>() % 2)];
        let flags: [bool; 4] = [kani::any(), kani::any(), kani::any(), kani::any()];
        spec_review(operation, generation, flags);
        let was = EXCLUSIONS[usize::from(kani::any::<u8>() % 3)];
        let now = EXCLUSIONS[usize::from(kani::any::<u8>() % 3)];
        spec_pause(operation, flags[0], was, now, flags[3]);
        kani::cover!(
            !keeps_pause_exclusion(operation, flags[0], was, now) && operation == Operation::Edit
        );
        kani::cover!(review_requirement(flags[3], now) == ReviewRequirement::Resumed);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_pr_merges_only_ready_current_passing_clean_and_without_an_owed_review() {
        let flags: [bool; 5] = [
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
        ];
        let [conflict, behind, clean, stacked, paused] = flags;
        let state = MergeState {
            lifecycle: LIFECYCLES[usize::from(kani::any::<u8>() % 4)],
            conflict,
            behind,
            checks: HEAD_CHECKS[usize::from(kani::any::<u8>() % 3)],
            clean,
            exclusion: EXCLUSIONS[usize::from(kani::any::<u8>() % 3)],
        };
        spec_merge(state, stacked, paused);
        let step = merge_step(state, stacked, paused);
        kani::cover!(step == Step::Merge && paused);
        kani::cover!(step == Step::Restack);
        kani::cover!(step == Step::Exclude);
        kani::cover!(step == Step::Wait && state.checks == HeadChecks::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flags;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every input")]
    fn every_input_meets_the_pr_specifications() {
        for operation in proofs::OPERATIONS {
            for generation in Generation::ALL {
                for issue in proofs::ISSUES {
                    for checks in [Checks::Success, Checks::Incomplete] {
                        for [validated, draft] in flags() {
                            proofs::spec_plan(
                                operation, generation, validated, draft, issue, checks,
                            );
                        }
                    }
                }
                for review in flags() {
                    proofs::spec_review(operation, generation, review);
                }
            }
            for was in proofs::EXCLUSIONS {
                for now in proofs::EXCLUSIONS {
                    for [draft, paused] in flags() {
                        proofs::spec_pause(operation, draft, was, now, paused);
                    }
                }
            }
        }
        for [first, second, third] in flags() {
            proofs::spec_flags(first, second, third);
        }
        for total in [0, 1, 2, usize::MAX] {
            for unfinished in [0, 1, usize::MAX] {
                for failed in [0, 1, usize::MAX] {
                    proofs::spec_checks(total, unfinished, failed);
                }
            }
        }
        for remaining in [0, 1, 2, u64::MAX] {
            for reserve in [0, 1, 2, u64::MAX] {
                proofs::spec_quota(remaining, reserve);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every merge state")]
    fn every_merge_state_meets_the_merge_specification() {
        for lifecycle in proofs::LIFECYCLES {
            for checks in proofs::HEAD_CHECKS {
                for exclusion in proofs::EXCLUSIONS {
                    for [conflict, behind, clean, stacked, paused] in flags() {
                        let state = MergeState {
                            lifecycle,
                            conflict,
                            behind,
                            checks,
                            clean,
                            exclusion,
                        };
                        proofs::spec_merge(state, stacked, paused);
                    }
                }
            }
        }
    }

    #[test]
    fn each_generation_has_its_own_key() {
        assert_eq!(
            Generation::ALL.map(Generation::key),
            ["local", "coderabbit"]
        );
        for generation in Generation::ALL {
            assert_eq!(Generation::from_key(generation.key()), Some(generation));
        }
        assert_eq!(Generation::from_key("remote"), None);
    }

    const READY: MergeState = MergeState {
        lifecycle: Lifecycle::Ready,
        conflict: false,
        behind: false,
        checks: HeadChecks::Success,
        clean: true,
        exclusion: Exclusion::Pause,
    };

    #[test]
    fn an_empty_or_unfinished_check_report_is_pending_and_any_failure_fails() {
        assert_eq!(head_checks(0, 0, 0), HeadChecks::Pending);
        assert_eq!(head_checks(3, 1, 0), HeadChecks::Pending);
        assert_eq!(head_checks(3, 1, 1), HeadChecks::Failed);
        assert_eq!(head_checks(3, 0, 0), HeadChecks::Success);
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
}
