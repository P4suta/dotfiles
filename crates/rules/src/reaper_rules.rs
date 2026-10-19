//! The reaping decision of the session reaper and the step function of `specs/reaper_lock.qnt`, which shares it.
//! The operating system releases the advisory lock of a run that dies, so no run steals a lock, and the verdict reads the process again right before the signal.

/// The seconds a process has run, from the system uptime and the clock ticks at which it started, or `None` without a tick rate.
#[cfg_attr(kani, kani::ensures(|result: &Option<u64>| result.is_none() == (ticks_per_second == 0) && result.is_none_or(|age| age <= uptime)))]
#[must_use]
pub fn age(uptime: u64, start_ticks: u64, ticks_per_second: u64) -> Option<u64> {
    if ticks_per_second == 0 {
        None
    } else {
        Some(uptime.saturating_sub(elapsed_seconds(start_ticks, ticks_per_second)))
    }
}

#[cfg_attr(kani, kani::requires(frequency != 0))]
#[cfg_attr(kani, kani::ensures(|elapsed: &u64| *elapsed <= ticks))]
fn elapsed_seconds(ticks: u64, frequency: u64) -> u64 {
    ticks / frequency
}

/// Whether the reaper signals what it would reap, or only reports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Report,
    Reap,
}

/// The process behind an ID as the reaper reads it again right before the signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    /// The start time still equals the one the listing read, so nothing reused the ID.
    pub same_identity: bool,
    pub stopped: bool,
    pub orphan: bool,
}

/// How long a stopped orphan must have run and stayed in view before the reaper kills it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Thresholds {
    pub min_age: u64,
    pub grace: u64,
}

/// What the reaper does with a process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Kill,
    /// Every fact admits the reaping, but the mode only reports it.
    WouldKill,
    Spare,
}

/// The reaper as it runs now, or as it ran with the stolen directory lock and a verdict that read only the identity again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reaper {
    Fixed,
    Old,
}

/// Whether the process read again right before the signal matches the one the reaper observed, which stays stopped and orphaned.
#[must_use]
pub const fn confirmed(reaper: Reaper, snapshot: Snapshot) -> bool {
    match reaper {
        Reaper::Fixed => snapshot.same_identity && snapshot.stopped && snapshot.orphan,
        Reaper::Old => snapshot.same_identity,
    }
}

/// The verdict on a process of `age` seconds that the reaper first saw `waited` seconds ago, as `snapshot` shows it right before the signal.
#[must_use]
pub const fn verdict(
    mode: Mode,
    snapshot: Snapshot,
    age: u64,
    waited: u64,
    thresholds: Thresholds,
) -> Verdict {
    if !confirmed(Reaper::Fixed, snapshot) || age < thresholds.min_age || waited < thresholds.grace
    {
        Verdict::Spare
    } else {
        match mode {
            Mode::Reap => Verdict::Kill,
            Mode::Report => Verdict::WouldKill,
        }
    }
}

/// The runs the model starts.
pub const RUNS: usize = 3;

/// How often the model lets the operating system reuse the target's ID.
pub const REUSES: u8 = 1;

/// Where one run stands, as `specs/reaper_lock.qnt` names it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Phase {
    Idle,
    /// The old run judged the lock stale and now removes it.
    Stealing,
    /// The old run removed the lock and now creates it again.
    Creating,
    Holding,
    Done,
}

/// The process behind the target ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct Target {
    pub identity: u8,
    pub stopped: bool,
    pub orphan: bool,
}

/// What one reaping hit: the process at that moment and the identity the run had observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct Kill {
    pub identity: u8,
    pub observed: u8,
    pub stopped: bool,
    pub orphan: bool,
}

/// One state of the model, field for field as `specs/reaper_lock.qnt` declares it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct State {
    pub phase: [Phase; RUNS],
    pub observed: [Option<u8>; RUNS],
    pub locked: bool,
    pub target: Target,
    pub kill: Option<Kill>,
    pub refused: bool,
}

/// One transition of the model, with the run it moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Take(usize),
    Skip(usize),
    Steal(usize),
    Remove(usize),
    Create(usize),
    Observe(usize),
    Kill(usize),
    Release(usize),
    Crash(usize),
    Resume,
    Halt,
    Reuse { stopped: bool, orphan: bool },
}

/// No run started, and the target counts as a stopped orphan.
#[must_use]
pub const fn initial() -> State {
    State {
        phase: [Phase::Idle; RUNS],
        observed: [None; RUNS],
        locked: false,
        target: Target {
            identity: 0,
            stopped: true,
            orphan: true,
        },
        kill: None,
        refused: false,
    }
}

/// Moves `run` from `from` to `to`, or `None` when it stands elsewhere.
fn advance(state: &mut State, run: usize, from: Phase, to: Phase) -> Option<()> {
    let phase = state.phase.get_mut(run)?;
    if *phase != from {
        return None;
    }
    *phase = to;
    Some(())
}

/// The state after `action`, or `None` when the action can't happen in `state`.
#[must_use]
pub fn step(reaper: Reaper, state: State, action: Action) -> Option<State> {
    let mut next = state;
    match action {
        Action::Take(run) => {
            if state.locked {
                return None;
            }
            advance(&mut next, run, Phase::Idle, Phase::Holding)?;
            next.locked = true;
        }
        Action::Skip(run) => {
            if !state.locked {
                return None;
            }
            advance(&mut next, run, Phase::Idle, Phase::Done)?;
        }
        Action::Steal(run) => {
            if reaper == Reaper::Fixed || !state.locked {
                return None;
            }
            advance(&mut next, run, Phase::Idle, Phase::Stealing)?;
        }
        Action::Remove(run) => {
            advance(&mut next, run, Phase::Stealing, Phase::Creating)?;
            next.locked = false;
        }
        Action::Create(run) => {
            let to = if state.locked {
                Phase::Done
            } else {
                Phase::Holding
            };
            advance(&mut next, run, Phase::Creating, to)?;
            next.locked = true;
        }
        Action::Observe(run) => {
            if !(state.target.stopped && state.target.orphan) {
                return None;
            }
            advance(&mut next, run, Phase::Holding, Phase::Holding)?;
            let observed = next.observed.get_mut(run)?;
            if observed.is_some() {
                return None;
            }
            *observed = Some(state.target.identity);
        }
        Action::Kill(run) => {
            advance(&mut next, run, Phase::Holding, Phase::Holding)?;
            let seen = next.observed.get_mut(run)?.take()?;
            let target = state.target;
            let snapshot = Snapshot {
                same_identity: target.identity == seen,
                stopped: target.stopped,
                orphan: target.orphan,
            };
            if confirmed(reaper, snapshot) {
                next.kill = Some(Kill {
                    identity: target.identity,
                    observed: seen,
                    stopped: target.stopped,
                    orphan: target.orphan,
                });
            } else {
                next.refused = true;
            }
        }
        Action::Release(run) => {
            advance(&mut next, run, Phase::Holding, Phase::Done)?;
            next.locked = false;
            *next.observed.get_mut(run)? = None;
        }
        Action::Crash(run) => {
            advance(&mut next, run, Phase::Holding, Phase::Done)?;
            // The operating system drops the advisory lock of a dead process, and the old directory outlives it.
            next.locked = reaper == Reaper::Old && state.locked;
            *next.observed.get_mut(run)? = None;
        }
        Action::Resume => {
            if !state.target.stopped {
                return None;
            }
            next.target.stopped = false;
        }
        Action::Halt => {
            if state.target.stopped {
                return None;
            }
            next.target.stopped = true;
        }
        Action::Reuse { stopped, orphan } => {
            if state.target.identity >= REUSES {
                return None;
            }
            next.target = Target {
                identity: state.target.identity + 1,
                stopped,
                orphan,
            };
        }
    }
    Some(next)
}

/// At most one run holds the lock, and every reaping hit a process its run had observed stopped and orphaned.
#[must_use]
pub fn safe(state: &State) -> bool {
    let holders = state
        .phase
        .iter()
        .filter(|phase| **phase == Phase::Holding)
        .count();
    holders <= 1
        && state
            .kill
            .is_none_or(|kill| kill.identity == kill.observed && kill.stopped && kill.orphan)
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    #[cfg(kani)]
    pub const PHASES: [Phase; 5] = [
        Phase::Idle,
        Phase::Stealing,
        Phase::Creating,
        Phase::Holding,
        Phase::Done,
    ];

    /// A process age never underflows, needs a known tick rate, and never exceeds the uptime.
    pub fn spec_age(uptime: u64, ticks: u64, hz: u64) {
        match age(uptime, ticks, hz) {
            None => assert_eq!(hz, 0),
            Some(age) => {
                assert_ne!(hz, 0);
                assert!(age <= uptime);
                assert_eq!(age, uptime.saturating_sub(ticks / hz));
            }
        }
    }

    /// The reaper reaps only a confirmed stopped orphan past both thresholds in reap mode, and a longer run never spares what a shorter one would reap.
    pub fn spec_verdict(
        mode: Mode,
        snapshot: Snapshot,
        age: u64,
        waited: u64,
        thresholds: Thresholds,
    ) -> Verdict {
        let decided = verdict(mode, snapshot, age, waited, thresholds);
        let admitted = snapshot.same_identity
            && snapshot.stopped
            && snapshot.orphan
            && age >= thresholds.min_age
            && waited >= thresholds.grace;
        assert_eq!(decided == Verdict::Kill, admitted && mode == Mode::Reap);
        assert_eq!(
            decided == Verdict::WouldKill,
            admitted && mode == Mode::Report
        );
        if decided != Verdict::Spare {
            assert_eq!(
                verdict(mode, snapshot, age.saturating_add(1), waited, thresholds),
                decided
            );
            assert_eq!(
                verdict(mode, snapshot, age, waited.saturating_add(1), thresholds),
                decided
            );
        }
        assert_eq!(confirmed(Reaper::Old, snapshot), snapshot.same_identity);
        decided
    }

    /// The facts every reachable state of the fixed reaper keeps, which imply `safe` and survive every step.
    pub fn invariant(state: &State) -> bool {
        let holders = state
            .phase
            .iter()
            .filter(|phase| **phase == Phase::Holding)
            .count();
        let runs_hold = state.phase.iter().zip(state.observed).all(|(phase, seen)| {
            !matches!(phase, Phase::Stealing | Phase::Creating)
                && (seen.is_none() || *phase == Phase::Holding)
                && seen.is_none_or(|seen| seen <= state.target.identity)
        });
        holders <= 1
            && state.locked == (holders == 1)
            && runs_hold
            && state.target.identity <= REUSES
            && safe(state)
    }

    /// The fixed reaper keeps the invariant through every step, so no run of any length breaks safety.
    pub fn spec_step(state: State, action: Action) -> Option<State> {
        let next = step(Reaper::Fixed, state, action);
        if invariant(&state) {
            assert!(safe(&state));
            if let Some(next) = next {
                assert!(invariant(&next));
            }
        }
        next
    }

    #[cfg(kani)]
    fn any_phase() -> Phase {
        PHASES[usize::from(kani::any::<u8>() % 5)]
    }

    #[cfg(kani)]
    fn any_state() -> State {
        let kill: Option<Kill> = if kani::any() {
            Some(Kill {
                identity: kani::any(),
                observed: kani::any(),
                stopped: kani::any(),
                orphan: kani::any(),
            })
        } else {
            None
        };
        State {
            phase: [any_phase(), any_phase(), any_phase()],
            observed: [kani::any(), kani::any(), kani::any()],
            locked: kani::any(),
            target: Target {
                identity: kani::any(),
                stopped: kani::any(),
                orphan: kani::any(),
            },
            kill,
            refused: kani::any(),
        }
    }

    #[cfg(kani)]
    fn any_action() -> Action {
        let run = usize::from(kani::any::<u8>() % 4);
        match kani::any::<u8>() % 12 {
            0 => Action::Take(run),
            1 => Action::Skip(run),
            2 => Action::Steal(run),
            3 => Action::Remove(run),
            4 => Action::Create(run),
            5 => Action::Observe(run),
            6 => Action::Kill(run),
            7 => Action::Release(run),
            8 => Action::Crash(run),
            9 => Action::Resume,
            10 => Action::Halt,
            _ => Action::Reuse {
                stopped: kani::any(),
                orphan: kani::any(),
            },
        }
    }

    #[cfg(kani)]
    #[kani::proof_for_contract(elapsed_seconds)]
    fn elapsed_seconds_never_exceed_the_ticks() {
        let ticks: u64 = kani::any();
        let elapsed = elapsed_seconds(ticks, kani::any());
        kani::cover!(elapsed == 0 && ticks > 0);
        kani::cover!(elapsed > 0);
    }

    #[cfg(kani)]
    #[kani::proof_for_contract(age)]
    #[kani::stub_verified(elapsed_seconds)]
    fn process_age_never_underflows_or_divides_by_zero() {
        let decided = age(kani::any(), kani::any(), kani::any());
        kani::cover!(decided.is_none());
        kani::cover!(decided.is_some_and(|value| value > 0));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_confirmed_stopped_orphan_past_both_thresholds_is_killed() {
        let mode = if kani::any() {
            Mode::Reap
        } else {
            Mode::Report
        };
        let snapshot = Snapshot {
            same_identity: kani::any(),
            stopped: kani::any(),
            orphan: kani::any(),
        };
        let thresholds = Thresholds {
            min_age: kani::any(),
            grace: kani::any(),
        };
        let decided = spec_verdict(mode, snapshot, kani::any(), kani::any(), thresholds);
        kani::cover!(decided == Verdict::Kill);
        kani::cover!(decided == Verdict::Spare && snapshot.same_identity);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_step_of_the_fixed_reaper_keeps_the_invariant_that_implies_safety() {
        assert!(invariant(&initial()));
        let state = any_state();
        let action = any_action();
        let next = spec_step(state, action);
        kani::cover!(invariant(&state) && next.is_some_and(|next| next.kill.is_some()));
        kani::cover!(invariant(&state) && next.is_some_and(|next| next.refused && !state.refused));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use alloc::vec::Vec;

    /// The model checker's count of distinct states of `specs/reaper_lock.qnt`, which `policy/obligations.toml` records too.
    const DISTINCT_STATES: usize = 970;

    const VALUES: [u64; 7] = [0, 1, 2, 99, 100, 101, u64::MAX];

    fn actions() -> Vec<Action> {
        let mut actions = Vec::from([Action::Resume, Action::Halt]);
        for run in 0..RUNS {
            actions.extend([
                Action::Take(run),
                Action::Skip(run),
                Action::Steal(run),
                Action::Remove(run),
                Action::Create(run),
                Action::Observe(run),
                Action::Kill(run),
                Action::Release(run),
                Action::Crash(run),
            ]);
        }
        for stopped in [false, true] {
            for orphan in [false, true] {
                actions.push(Action::Reuse { stopped, orphan });
            }
        }
        actions
    }

    fn reachable(reaper: Reaper) -> BTreeSet<State> {
        let mut seen = BTreeSet::from([initial()]);
        let mut frontier = Vec::from([initial()]);
        while let Some(state) = frontier.pop() {
            for action in actions() {
                if let Some(next) = step(reaper, state, action).filter(|next| seen.insert(*next)) {
                    frontier.push(next);
                }
            }
        }
        seen
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every reachable state")]
    fn the_fixed_reaper_reaches_exactly_the_states_tlc_counts_and_each_is_safe() {
        let states = reachable(Reaper::Fixed);
        assert_eq!(states.len(), DISTINCT_STATES);
        // A run skips only a held lock.
        assert_eq!(step(Reaper::Fixed, initial(), Action::Skip(0)), None);
        let held = step(Reaper::Fixed, initial(), Action::Take(0)).unwrap();
        let skipped = step(Reaper::Fixed, held, Action::Skip(1)).unwrap();
        assert_eq!(skipped.phase.get(1), Some(&Phase::Done));
        for state in &states {
            assert!(proofs::invariant(state), "{state:?}");
            for action in actions() {
                proofs::spec_step(*state, action);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every reachable state")]
    fn the_old_reaper_lets_two_runs_steal_one_lock_and_kills_a_resumed_process() {
        let mut state = initial();
        for action in [
            Action::Take(0),
            Action::Crash(0),
            Action::Steal(1),
            Action::Steal(2),
            Action::Remove(1),
            Action::Create(1),
            Action::Remove(2),
            Action::Create(2),
        ] {
            state = step(Reaper::Old, state, action).unwrap();
        }
        assert!(!safe(&state));
        let mut state = initial();
        for action in [
            Action::Take(0),
            Action::Observe(0),
            Action::Resume,
            Action::Kill(0),
        ] {
            state = step(Reaper::Old, state, action).unwrap();
        }
        assert!(!safe(&state));
        assert!(reachable(Reaper::Old).iter().any(|state| !safe(state)));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every boundary")]
    fn every_boundary_meets_the_age_and_verdict_specifications() {
        for uptime in VALUES {
            for ticks in VALUES {
                for hz in VALUES {
                    proofs::spec_age(uptime, ticks, hz);
                }
            }
        }
        assert_eq!(age(10, 250, 100), Some(8));
        assert_eq!(age(1, 250, 100), Some(0));
        assert_eq!(age(10, 250, 0), None);
        for [same_identity, stopped, orphan, reap] in crate::domain::flags() {
            let snapshot = Snapshot {
                same_identity,
                stopped,
                orphan,
            };
            let mode = if reap { Mode::Reap } else { Mode::Report };
            for age in VALUES {
                for waited in VALUES {
                    for min_age in [0, 100, u64::MAX] {
                        for grace in [0, 100, u64::MAX] {
                            proofs::spec_verdict(
                                mode,
                                snapshot,
                                age,
                                waited,
                                Thresholds { min_age, grace },
                            );
                        }
                    }
                }
            }
        }
    }
}
