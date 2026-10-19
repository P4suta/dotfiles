//! The decisions of `pr-workflow ready` and the step function of `specs/pr_ready.qnt`, which shares them.
//! GitHub's `markPullRequestReadyForReview` takes no expected head, so another push can land between the read and the mark.
//! The command reads the head again after marking and returns a moved PR to draft, so it never leaves an unchecked head ready.

/// The number of pushes the model lets other hosts make, which bounds its heads.
pub const PUSHES: u8 = 2;

/// The heads the model can reach, numbered in push order.
const HEADS: usize = PUSHES as usize + 1;

/// What the required checks of one head have reported.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Report {
    /// Some check hasn't finished, or no check reported.
    Pending,
    /// Every reported check passed, but a required check hasn't reported.
    Partial,
    /// Every required check reported, and every reported check passed.
    Complete,
    /// A reported check failed.
    Failed,
}

/// The report of a head from its rollup: the checks it lists, how many haven't finished or failed, and how many required checks it lacks or hasn't passed.
#[must_use]
pub const fn report(total: usize, unfinished: usize, failed: usize, missing: usize) -> Report {
    if failed > 0 {
        Report::Failed
    } else if total == 0 || unfinished > 0 {
        Report::Pending
    } else if missing > 0 {
        Report::Partial
    } else {
        Report::Complete
    }
}

/// The command as it runs now, or as it ran before it read the head again and required every required check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Workflow {
    Fixed,
    Old,
}

/// Whether a report lets the command mark the PR ready.
#[must_use]
pub const fn complete(workflow: Workflow, report: Report) -> bool {
    match workflow {
        Workflow::Fixed => matches!(report, Report::Complete),
        Workflow::Old => matches!(report, Report::Partial | Report::Complete),
    }
}

/// What the command does after marking the PR ready and reading its head again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Settlement {
    /// The head stayed the one the checks covered.
    Keep,
    /// Another push landed, so the PR goes back to draft.
    ReturnToDraft,
}

#[must_use]
pub fn settle<T: PartialEq + ?Sized>(viewed: &T, reread: &T) -> Settlement {
    if viewed == reread {
        Settlement::Keep
    } else {
        Settlement::ReturnToDraft
    }
}

/// Where the command stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Phase {
    Idle,
    Viewed,
    Marked,
    Refused,
    Returned,
}

/// One state of the model, field for field as `specs/pr_ready.qnt` declares it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct State {
    pub tip: u8,
    pub reports: [Report; HEADS],
    pub ready: bool,
    pub phase: Phase,
    /// The head the command read together with its checks.
    pub viewed: Option<u8>,
    /// Whether that read let the command mark the PR ready.
    pub checked: bool,
    /// The head when the command returned.
    pub returned_tip: Option<u8>,
}

/// One transition of the model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    /// Another host pushes a new head.
    Push,
    /// CI reports on the current head.
    Report(Report),
    /// The command reads the head and its checks in one response.
    View,
    /// The command marks the PR ready, or refuses.
    Mark,
    /// The command reads the head again and returns a moved PR to draft.
    Settle,
}

/// The state before the command runs, with no check reported.
#[must_use]
pub const fn initial() -> State {
    State {
        tip: 0,
        reports: [Report::Pending; HEADS],
        ready: false,
        phase: Phase::Idle,
        viewed: None,
        checked: false,
        returned_tip: None,
    }
}

/// The report on head `head` of `state`, pending for a head the model never reaches.
fn on(state: &State, head: u8) -> Report {
    state
        .reports
        .get(usize::from(head))
        .copied()
        .unwrap_or(Report::Pending)
}

/// The state after `action`, or `None` when the action can't happen in `state`.
#[must_use]
pub fn step(workflow: Workflow, state: State, action: Action) -> Option<State> {
    let mut next = state;
    match action {
        Action::Push => {
            if state.tip >= PUSHES {
                return None;
            }
            next.tip = state.tip + 1;
        }
        Action::Report(outcome) => {
            if matches!(on(&state, state.tip), Report::Complete | Report::Failed)
                || outcome == Report::Pending
            {
                return None;
            }
            let slot = next.reports.get_mut(usize::from(state.tip))?;
            *slot = outcome;
        }
        Action::View => {
            if state.phase != Phase::Idle {
                return None;
            }
            next.phase = Phase::Viewed;
            next.viewed = Some(state.tip);
            next.checked = complete(workflow, on(&state, state.tip));
        }
        Action::Mark => {
            if state.phase != Phase::Viewed {
                return None;
            }
            if state.checked {
                next.ready = true;
                if workflow == Workflow::Old {
                    next.phase = Phase::Returned;
                    next.returned_tip = Some(state.tip);
                } else {
                    next.phase = Phase::Marked;
                }
            } else {
                next.phase = Phase::Refused;
                next.returned_tip = Some(state.tip);
            }
        }
        Action::Settle => {
            if state.phase != Phase::Marked {
                return None;
            }
            next.phase = Phase::Returned;
            next.returned_tip = Some(state.tip);
            if settle(&state.viewed, &Some(state.tip)) == Settlement::ReturnToDraft {
                next.ready = false;
            }
        }
    }
    Some(next)
}

/// When the command returns with the PR ready, the head it left ready has every required check passed.
#[must_use]
pub fn safe(state: &State) -> bool {
    if state.phase != Phase::Returned || !state.ready {
        return true;
    }
    match state.viewed {
        Some(viewed) => state.returned_tip == Some(viewed) && on(state, viewed) == Report::Complete,
        None => false,
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const REPORTS: [Report; 4] = [
        Report::Pending,
        Report::Partial,
        Report::Complete,
        Report::Failed,
    ];
    #[cfg(kani)]
    pub const PHASES: [Phase; 5] = [
        Phase::Idle,
        Phase::Viewed,
        Phase::Marked,
        Phase::Refused,
        Phase::Returned,
    ];

    /// A failure decides first, then an unfinished or empty rollup, then a missing required check.
    pub fn spec_report(total: usize, unfinished: usize, failed: usize, missing: usize) -> Report {
        let decided = report(total, unfinished, failed, missing);
        let expected = if failed > 0 {
            Report::Failed
        } else if total == 0 || unfinished > 0 {
            Report::Pending
        } else if missing > 0 {
            Report::Partial
        } else {
            Report::Complete
        };
        assert_eq!(decided, expected);
        decided
    }

    /// Only a complete report readies a PR now, and the old command also took a partial one.
    pub fn spec_complete(report: Report) {
        assert_eq!(
            complete(Workflow::Fixed, report),
            report == Report::Complete
        );
        assert_eq!(
            complete(Workflow::Old, report),
            matches!(report, Report::Partial | Report::Complete)
        );
    }

    /// The command keeps the PR ready only when the head it read again matches the one it checked.
    pub fn spec_settle(viewed: u8, reread: u8) -> Settlement {
        let settled = settle(&viewed, &reread);
        assert_eq!(settled == Settlement::Keep, viewed == reread);
        settled
    }

    /// The facts every reachable state of the fixed command keeps, which imply `safe` and survive every step.
    pub fn invariant(state: &State) -> bool {
        let viewed_complete = state
            .viewed
            .is_some_and(|viewed| viewed <= state.tip && on(state, viewed) == Report::Complete);
        let fresh = state.viewed.is_none() && !state.checked && state.returned_tip.is_none();
        state.tip <= PUSHES
            && (state.phase == Phase::Idle) == state.viewed.is_none()
            && (!state.checked || viewed_complete)
            && match state.phase {
                Phase::Idle => fresh && !state.ready,
                Phase::Viewed => !state.ready,
                Phase::Refused => !state.ready && !state.checked,
                Phase::Marked => state.ready && state.checked,
                Phase::Returned => {
                    state.checked && (!state.ready || state.returned_tip == state.viewed)
                }
            }
    }

    /// The fixed command keeps the invariant through every step, so no run of any length returns with an unchecked head ready.
    pub fn spec_step(state: State, action: Action) -> Option<State> {
        let next = step(Workflow::Fixed, state, action);
        if invariant(&state) {
            assert!(safe(&state));
            if let Some(next) = next {
                assert!(invariant(&next));
            }
        }
        next
    }

    /// The model's state from Kani's choices, with every field inside its declared range.
    #[cfg(kani)]
    fn any_state() -> State {
        let viewed: Option<u8> = kani::any();
        let returned_tip: Option<u8> = kani::any();
        State {
            tip: kani::any(),
            reports: [
                REPORTS[usize::from(kani::any::<u8>() % 4)],
                REPORTS[usize::from(kani::any::<u8>() % 4)],
                REPORTS[usize::from(kani::any::<u8>() % 4)],
            ],
            ready: kani::any(),
            phase: PHASES[usize::from(kani::any::<u8>() % 5)],
            viewed,
            checked: kani::any(),
            returned_tip,
        }
    }

    #[cfg(kani)]
    fn any_action() -> Action {
        match kani::any::<u8>() % 5 {
            0 => Action::Push,
            1 => Action::Report(REPORTS[usize::from(kani::any::<u8>() % 4)]),
            2 => Action::View,
            3 => Action::Mark,
            _ => Action::Settle,
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_failure_then_an_unfinished_check_then_a_missing_required_check_decides_the_report() {
        let decided = spec_report(kani::any(), kani::any(), kani::any(), kani::any());
        kani::cover!(decided == Report::Partial);
        kani::cover!(decided == Report::Complete);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_complete_report_readies_a_pr() {
        let report = REPORTS[usize::from(kani::any::<u8>() % 4)];
        spec_complete(report);
        kani::cover!(report == Report::Partial);
        kani::cover!(report == Report::Complete);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_moved_head_returns_the_pr_to_draft() {
        let settled = spec_settle(kani::any(), kani::any());
        kani::cover!(settled == Settlement::Keep);
        kani::cover!(settled == Settlement::ReturnToDraft);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_step_of_the_fixed_command_keeps_the_invariant_that_implies_safety() {
        // The base case of the induction: the command starts inside the invariant.
        assert!(invariant(&initial()));
        let state = any_state();
        let action = any_action();
        let next = spec_step(state, action);
        kani::cover!(
            invariant(&state)
                && next.is_some_and(|next| next.phase == Phase::Returned && next.ready)
        );
        kani::cover!(
            invariant(&state)
                && next.is_some_and(|next| next.phase == Phase::Returned && !next.ready)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use alloc::vec::Vec;

    /// The model checker's count of distinct states of `specs/pr_ready.qnt`, which `policy/obligations.toml` records too.
    const DISTINCT_STATES: usize = 971;

    fn actions() -> Vec<Action> {
        let mut actions = Vec::from([Action::Push, Action::View, Action::Mark, Action::Settle]);
        actions.extend(proofs::REPORTS.map(Action::Report));
        actions
    }

    /// Every state a workflow reaches from the initial state.
    fn reachable(workflow: Workflow) -> BTreeSet<State> {
        let mut seen = BTreeSet::from([initial()]);
        let mut frontier = Vec::from([initial()]);
        while let Some(state) = frontier.pop() {
            for action in actions() {
                // Kani parses the whole module as an older edition, which has no let chains.
                if let Some(next) = step(workflow, state, action).filter(|next| seen.insert(*next))
                {
                    frontier.push(next);
                }
            }
        }
        seen
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every reachable state")]
    fn the_fixed_command_reaches_exactly_the_states_tlc_counts_and_each_is_safe() {
        let states = reachable(Workflow::Fixed);
        assert_eq!(states.len(), DISTINCT_STATES);
        for state in &states {
            assert!(proofs::invariant(state), "{state:?}");
            for action in actions() {
                proofs::spec_step(*state, action);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every reachable state")]
    fn the_old_command_leaves_a_head_pushed_after_its_read_ready() {
        let mut state = initial();
        for action in [
            Action::Report(Report::Complete),
            Action::View,
            Action::Push,
            Action::Mark,
        ] {
            state = step(Workflow::Old, state, action).unwrap();
        }
        assert!(state.ready && !safe(&state));
        assert!(reachable(Workflow::Old).iter().any(|state| !safe(state)));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every report and head")]
    fn every_report_and_head_pair_meets_its_specification() {
        for total in 0..3 {
            for unfinished in 0..3 {
                for failed in 0..3 {
                    for missing in 0..3 {
                        proofs::spec_report(total, unfinished, failed, missing);
                    }
                }
            }
        }
        for report in proofs::REPORTS {
            proofs::spec_complete(report);
        }
        for viewed in 0..=u8::MAX {
            proofs::spec_settle(viewed, viewed);
            proofs::spec_settle(viewed, viewed.wrapping_add(1));
        }
    }

    #[test]
    fn a_pending_report_and_a_disabled_action_change_nothing() {
        let state = initial();
        assert_eq!(
            step(Workflow::Fixed, state, Action::Report(Report::Pending)),
            None
        );
        assert_eq!(step(Workflow::Fixed, state, Action::Mark), None);
        assert_eq!(step(Workflow::Fixed, state, Action::Settle), None);
        let mut pushed = state;
        pushed.tip = PUSHES;
        assert_eq!(step(Workflow::Fixed, pushed, Action::Push), None);
    }
}
