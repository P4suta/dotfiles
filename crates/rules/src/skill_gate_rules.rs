//! Review coverage, complete dispositions, and the transitions of `specs/skill_gate.qnt`.
//! Each decision binds to its finding.
//! A recorded review covers a draft only when it decides every draft finding.

/// Whether the review covers every draft finding through adoption or a matching decision.
/// Finding tuples hold their indices and adoption flags.
/// Decision entries hold finding indices.
#[must_use]
pub fn review_applies(draft: Option<&[(usize, bool)]>, decided: &[usize]) -> bool {
    draft.is_none_or(|findings| {
        findings
            .iter()
            .all(|(id, adopted)| *adopted || decided.iter().any(|decided| decided == id))
    })
}

/// Why a set of dispositions leaves a review incomplete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gap {
    /// The review names another report than the analysis.
    Stale,
    /// The analysis lists one finding twice.
    RepeatedFinding,
    /// A decision names no finding of the analysis, or a finding twice.
    UnknownDecision,
    /// A decision lacks its reason, evidence, or revisit condition.
    IncompleteDecision,
    /// A finding has neither a decision nor an adoption.
    Undecided,
}

fn covered(findings: &[(usize, bool)], decisions: &[(usize, bool)]) -> bool {
    for index in 0..findings.len() {
        let Some((id, adopted)) = findings.get(index) else {
            return false;
        };
        if *adopted {
            continue;
        }
        let mut decided = false;
        for decision in 0..decisions.len() {
            if decisions
                .get(decision)
                .is_some_and(|(other, complete)| other == id && *complete)
            {
                decided = true;
                break;
            }
        }
        if !decided {
            return false;
        }
    }
    true
}

/// Checks report identity, unique findings, complete decisions, and coverage of every finding.
/// Finding tuples hold their IDs and adoption flags.
/// Decision tuples hold finding IDs and completeness flags.
///
/// # Errors
///
/// Returns the first gap: a stale review, a repeated finding, an unknown or repeated decision, an incomplete decision, or an undecided finding.
pub fn review_complete(
    current: bool,
    findings: &[(usize, bool)],
    decisions: &[(usize, bool)],
) -> Result<(), Gap> {
    if !current {
        return Err(Gap::Stale);
    }
    for index in 0..findings.len() {
        if let Some((id, _)) = findings.get(index) {
            for earlier in 0..index {
                if findings.get(earlier).is_some_and(|(other, _)| other == id) {
                    return Err(Gap::RepeatedFinding);
                }
            }
        }
    }
    for index in 0..decisions.len() {
        if let Some((id, complete)) = decisions.get(index) {
            let mut known = false;
            for finding in 0..findings.len() {
                if findings.get(finding).is_some_and(|(other, _)| other == id) {
                    known = true;
                    break;
                }
            }
            if !known {
                return Err(Gap::UnknownDecision);
            }
            for earlier in 0..index {
                if decisions.get(earlier).is_some_and(|(other, _)| other == id) {
                    return Err(Gap::UnknownDecision);
                }
            }
            if !complete {
                return Err(Gap::IncompleteDecision);
            }
        }
    }
    if covered(findings, decisions) {
        Ok(())
    } else {
        Err(Gap::Undecided)
    }
}

/// The observations the model records at most, and the new observations that call for another analysis.
pub const MAX_EVENTS: u8 = 3;
pub const THRESHOLD: u8 = 2;

/// The triaging processes the model runs.
pub const PROCESSES: usize = 2;

/// The locked store, or the former store that checked analysis and recorded reviews in separate steps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Store {
    Fixed,
    Old,
}

/// What the Stop hook answered last.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Answer {
    None,
    Allow,
    Request,
    End,
}

/// One state of the model, field for field as `specs/skill_gate.qnt` declares it.
/// A report goes by the observation count it analyzed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct Gate {
    pub events: u8,
    pub draft: Option<u8>,
    pub reviewed: Option<u8>,
    /// The report each old triage checked before recording its review.
    pub checked: [Option<u8>; PROCESSES],
    pub continued: bool,
    pub requests: u8,
    pub answer: Answer,
}

/// One transition of the model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Move {
    Observe,
    Analyze,
    /// The fixed triage checks the analysis and records the review under the store lock.
    Triage,
    /// The old triage checks the analysis.
    Check(usize),
    /// The old triage records the review it checked.
    Record(usize),
    Stop,
}

/// An empty store at the start of a session.
#[must_use]
pub const fn gate() -> Gate {
    Gate {
        events: 0,
        draft: None,
        reviewed: None,
        checked: [None; PROCESSES],
        continued: false,
        requests: 0,
        answer: Answer::None,
    }
}

/// The findings of the model's reports: the report of `n` observations lists the first `n`, none of them adopted, and its review decides each.
const FINDINGS: [(usize, bool); MAX_EVENTS as usize] = [(1, false), (2, false), (3, false)];
const DECISIONS: [usize; MAX_EVENTS as usize] = [1, 2, 3];

/// Whether the draft or observations require maintenance after accounting for a matching review.
#[must_use]
pub fn pending(store: Store, state: &Gate) -> bool {
    let since = |report: u8| state.events.saturating_sub(report) >= THRESHOLD;
    let draft_pending = |draft: u8| draft > 0 || since(draft);
    if state.draft.is_some_and(|draft| !draft_pending(draft)) {
        return false;
    }
    let applies = |reviewed: u8| match store {
        // The old gate let a recorded review stand in for whatever draft came later.
        Store::Old => true,
        Store::Fixed => DECISIONS
            .get(..usize::from(reviewed))
            .is_some_and(|decided| {
                review_applies(
                    state
                        .draft
                        .map(|draft| FINDINGS.get(..usize::from(draft)).unwrap_or(&FINDINGS)),
                    decided,
                ) && state.draft.is_none_or(|draft| draft <= MAX_EVENTS)
            }),
    };
    match (state.reviewed, state.draft) {
        (Some(reviewed), _) if applies(reviewed) => since(reviewed),
        (_, Some(draft)) => draft_pending(draft),
        (_, None) => state.events >= THRESHOLD,
    }
}

/// The state after `action`, or `None` when the action can't happen in `state`.
#[must_use]
pub fn step(store: Store, state: Gate, action: Move) -> Option<Gate> {
    let mut next = state;
    // Each answer belongs to the store at the moment of its Stop, so any other move clears it.
    next.answer = Answer::None;
    match action {
        Move::Observe => {
            if state.events >= MAX_EVENTS {
                return None;
            }
            next.events = state.events + 1;
        }
        Move::Analyze => next.draft = Some(state.events),
        Move::Triage => {
            if store == Store::Old || state.draft != Some(state.events) {
                return None;
            }
            next.reviewed = state.draft;
        }
        Move::Check(process) => {
            if store == Store::Fixed || state.draft != Some(state.events) {
                return None;
            }
            *next.checked.get_mut(process)? = state.draft;
        }
        Move::Record(process) => {
            let checked = next.checked.get_mut(process)?.take()?;
            next.reviewed = Some(checked);
        }
        Move::Stop => {
            next.answer = if !pending(store, &state) {
                Answer::Allow
            } else if state.continued {
                Answer::End
            } else {
                next.continued = true;
                next.requests = state.requests.checked_add(1)?;
                Answer::Request
            };
        }
    }
    Some(next)
}

/// A stop needs a decided draft and at most one maintenance request per session.
#[must_use]
pub fn safe(state: &Gate) -> bool {
    let triaged = state
        .draft
        .is_none_or(|draft| draft == 0 || state.reviewed.is_some_and(|reviewed| reviewed >= draft));
    (state.answer != Answer::Allow || triaged) && state.requests <= 1
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// The facts every reachable state of the fixed store keeps, which imply `safe` and survive every step.
    pub fn invariant(state: &Gate) -> bool {
        state.events <= MAX_EVENTS
            && state.draft.is_none_or(|draft| draft <= state.events)
            && state
                .reviewed
                .is_none_or(|reviewed| reviewed <= state.events)
            && state.checked == [None; PROCESSES]
            && state.requests == u8::from(state.continued)
            && (state.answer != Answer::Request || state.continued)
            && safe(state)
    }

    /// The fixed store keeps the invariant through every step, so no session ends with an untriaged draft allowed.
    pub fn spec_step(state: Gate, action: Move) -> Option<Gate> {
        let next = step(Store::Fixed, state, action);
        if invariant(&state) {
            assert!(safe(&state));
            if let Some(next) = next {
                assert!(invariant(&next));
                assert!(next.events >= state.events);
            }
        }
        next
    }

    /// A stop answers `Allow` only without pending work, and an empty session owes nothing.
    pub fn spec_pending(state: Gate, action: Move) -> Option<Gate> {
        let next = spec_step(state, action);
        if state == gate() {
            assert!(!pending(Store::Fixed, &state));
        }
        if next
            .as_ref()
            .is_some_and(|next| next.answer == Answer::Allow)
        {
            assert!(!pending(Store::Fixed, &state));
        }
        next
    }

    /// A review applies to no draft, or to a draft whose every finding it decides or a reviewed revision adopted, and to nothing else.
    pub fn spec_applies(draft: Option<(usize, bool)>, decided: usize) -> bool {
        let applies = review_applies(draft.as_ref().map(core::slice::from_ref), &[decided]);
        assert_eq!(
            applies,
            draft.is_none_or(|(id, adopted)| adopted || id == decided)
        );
        applies
    }

    #[cfg(kani)]
    fn any_answer() -> Answer {
        match kani::any::<u8>() % 4 {
            0 => Answer::None,
            1 => Answer::Allow,
            2 => Answer::Request,
            _ => Answer::End,
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn every_step_of_the_fixed_store_keeps_the_invariant_that_implies_safety() {
        assert!(invariant(&gate()));
        let state = Gate {
            events: kani::any(),
            draft: kani::any(),
            reviewed: kani::any(),
            checked: kani::any(),
            continued: kani::any(),
            requests: kani::any(),
            answer: any_answer(),
        };
        kani::assume(invariant(&state));
        let process = usize::from(kani::any::<u8>() % 3);
        let action = match kani::any::<u8>() % 6 {
            0 => Move::Observe,
            1 => Move::Analyze,
            2 => Move::Triage,
            3 => Move::Check(process),
            4 => Move::Record(process),
            _ => Move::Stop,
        };
        let next = spec_pending(state, action);
        kani::cover!(
            invariant(&state)
                && next.is_some_and(|next| next.answer == Answer::Allow
                    && next.draft.is_some_and(|draft| draft > 0))
        );
        kani::cover!(invariant(&state) && next.is_some_and(|next| next.answer == Answer::End));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn every_step_of_an_invalid_store_keeps_the_pending_answer_specification() {
        let state = Gate {
            events: kani::any(),
            draft: kani::any(),
            reviewed: kani::any(),
            checked: kani::any(),
            continued: kani::any(),
            requests: kani::any(),
            answer: any_answer(),
        };
        kani::assume(!invariant(&state));
        let process = usize::from(kani::any::<u8>() % 3);
        let action = match kani::any::<u8>() % 6 {
            0 => Move::Observe,
            1 => Move::Analyze,
            2 => Move::Triage,
            3 => Move::Check(process),
            4 => Move::Record(process),
            _ => Move::Stop,
        };
        let next = spec_pending(state, action);
        kani::cover!(next.is_some());
        kani::cover!(next.is_none());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(3)]
    fn a_review_binds_only_to_the_findings_it_decides() {
        let applies = spec_applies(kani::any(), kani::any());
        kani::cover!(applies);
        kani::cover!(!applies);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_complete_review_decides_or_adopts_every_finding() {
        let result = bounded_review::<4>();
        kani::cover!(result.is_ok());
        kani::cover!(result == Err(Gap::Undecided));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_review_of_three_findings_decides_or_adopts_each() {
        let result = bounded_review::<3>();
        kani::cover!(result.is_ok());
        kani::cover!(result == Err(Gap::Undecided));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_review_of_two_findings_decides_or_adopts_each() {
        let result = bounded_review::<2>();
        kani::cover!(result.is_ok());
        kani::cover!(result == Err(Gap::Undecided));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_review_of_one_finding_decides_or_adopts_it() {
        let result = bounded_review::<1>();
        kani::cover!(result.is_ok());
        kani::cover!(result == Err(Gap::Undecided));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn an_empty_review_accepts_no_unknown_decision() {
        let result = bounded_review::<0>();
        kani::cover!(result.is_ok());
        kani::cover!(result == Err(Gap::UnknownDecision));
    }

    #[cfg(kani)]
    fn bounded_review<const N: usize>() -> Result<(), Gap> {
        review_input::<N, _>(|findings, decisions| {
            let current = kani::any();
            let result = review_complete(current, findings, decisions);
            if N == 0 {
                assert_eq!(result.is_ok(), current && decisions.is_empty());
                return result;
            }
            let mut known = 0u8;
            let mut adopted = 0u8;
            let mut decided = 0u8;
            let mut complete = true;
            for (id, adoption) in findings {
                known |= 1u8 << id;
                if *adoption {
                    adopted |= 1u8 << id;
                }
            }
            for (id, completion) in decisions {
                decided |= 1u8 << id;
                complete &= *completion;
            }
            let eligible = current
                && decided & !known == 0
                && complete
                && known & !(adopted | decided) == 0
                && usize::try_from(known.count_ones()).unwrap() == findings.len()
                && usize::try_from(decided.count_ones()).unwrap() == decisions.len();
            assert_eq!(result.is_ok(), eligible);
            result
        })
    }

    #[cfg(kani)]
    fn review_input<const N: usize, R>(
        run: impl FnOnce(&[(usize, bool)], &[(usize, bool)]) -> R,
    ) -> R {
        let findings: [(usize, bool); N] =
            core::array::from_fn(|_| (usize::from(kani::any::<u8>() % 4), kani::any()));
        let decisions: [(usize, bool); 4] = [
            (usize::from(kani::any::<u8>() % 5), kani::any()),
            (usize::from(kani::any::<u8>() % 5), kani::any()),
            (usize::from(kani::any::<u8>() % 5), kani::any()),
            (usize::from(kani::any::<u8>() % 5), kani::any()),
        ];
        run(&findings, &decisions[..usize::from(kani::any::<u8>() % 5)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use alloc::vec::Vec;

    /// The model checker's count of distinct states of `specs/skill_gate.qnt`, which `policy/obligations.toml` records too.
    const DISTINCT_STATES: usize = 121;

    #[test]
    fn an_invalid_request_count_is_refused_without_overflow() {
        let mut state = gate();
        state.events = THRESHOLD;
        state.requests = u8::MAX;
        assert_eq!(step(Store::Fixed, state, Move::Stop), None);
    }

    fn moves() -> Vec<Move> {
        let mut moves = Vec::from([Move::Observe, Move::Analyze, Move::Triage, Move::Stop]);
        for process in 0..PROCESSES {
            moves.extend([Move::Check(process), Move::Record(process)]);
        }
        moves
    }

    fn reachable(store: Store) -> BTreeSet<Gate> {
        let mut seen = BTreeSet::from([gate()]);
        let mut frontier = Vec::from([gate()]);
        while let Some(state) = frontier.pop() {
            for action in moves() {
                if let Some(next) = step(store, state, action).filter(|next| seen.insert(*next)) {
                    frontier.push(next);
                }
            }
        }
        seen
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every reachable store")]
    fn the_fixed_store_reaches_exactly_the_states_tlc_counts_and_each_is_safe() {
        let states = reachable(Store::Fixed);
        let complete = Gate {
            events: MAX_EVENTS,
            draft: Some(MAX_EVENTS),
            reviewed: Some(MAX_EVENTS),
            ..gate()
        };
        assert!(!pending(Store::Fixed, &complete));
        assert!(pending(
            Store::Fixed,
            &Gate {
                draft: Some(MAX_EVENTS + 1),
                ..complete
            }
        ));
        assert_eq!(
            review_complete(true, &[(0, false), (1, false)], &[(0, true)]),
            Err(Gap::Undecided)
        );
        assert_eq!(states.len(), DISTINCT_STATES);
        for state in &states {
            assert!(proofs::invariant(state), "{state:?}");
            for action in moves() {
                proofs::spec_pending(*state, action);
            }
        }
        // A triage between two analyses binds an old review that the old gate let stand in for the newer draft.
        let mut state = gate();
        for action in [
            Move::Observe,
            Move::Analyze,
            Move::Check(0),
            Move::Observe,
            Move::Analyze,
            Move::Record(0),
            Move::Stop,
        ] {
            state = step(Store::Old, state, action).unwrap_or(state);
        }
        assert!(!safe(&state));
        for draft in [None, Some((0, false)), Some((0, true)), Some((1, false))] {
            for decided in 0..2 {
                proofs::spec_applies(draft, decided);
            }
        }
        assert_eq!(review_complete(false, &[], &[]), Err(Gap::Stale));
        assert_eq!(
            review_complete(true, &[(0, false), (0, true)], &[]),
            Err(Gap::RepeatedFinding)
        );
        assert_eq!(
            review_complete(true, &[(0, false)], &[(1, true)]),
            Err(Gap::UnknownDecision)
        );
        assert_eq!(
            review_complete(true, &[(0, false)], &[(0, true), (0, true)]),
            Err(Gap::UnknownDecision)
        );
        assert_eq!(
            review_complete(true, &[(0, false)], &[(0, false)]),
            Err(Gap::IncompleteDecision)
        );
        assert_eq!(
            review_complete(true, &[(0, false), (1, true)], &[]),
            Err(Gap::Undecided)
        );
        assert_eq!(
            review_complete(true, &[(0, false), (1, true)], &[(0, true)]),
            Ok(())
        );
    }
}
