pub const HOUR_MS: u64 = 3_600_000;
pub const DAY_MS: u64 = 86_400_000;
pub const PROBE_GAP_MS: u64 = 60_000;
#[expect(
    clippy::cast_possible_truncation,
    reason = "the budget of ten reviews is seven, which every pointer width holds"
)]
pub const HOURLY_LIMIT: usize = budget(10) as usize;
pub const DAILY_LIMIT: usize = HOURLY_LIMIT * 24;
pub const PROBE_DAILY_LIMIT: usize = DAILY_LIMIT;

/// The share of the limit a budget may use: 72 percent, the 90 percent service margin times the 80 percent reserve.
pub const BUDGET_NUMERATOR: u128 = 18;
pub const BUDGET_DENOMINATOR: u128 = 25;

/// 72 percent of the `limit`, rounded down.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "72 percent of a u64 is at most the u64 itself, so the narrowing keeps every value"
)]
pub const fn budget(limit: u64) -> u64 {
    (limit as u128 * BUDGET_NUMERATOR / BUDGET_DENOMINATOR) as u64
}

#[must_use]
pub fn within_limits(hourly: usize, daily: usize) -> bool {
    hourly < HOURLY_LIMIT && daily < DAILY_LIMIT
}

/// The `CodeRabbit` review pools an owner pause stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseScope {
    None,
    Pr,
    Cli,
    All,
}

impl PauseScope {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pr => "pr",
            Self::Cli => "cli",
            Self::All => "all",
        }
    }
}

/// Combine the all, PR, and CLI pause markers into one scope.
#[must_use]
pub const fn pause_scope(all: bool, pr: bool, cli: bool) -> PauseScope {
    match (all || pr, all || cli) {
        (false, false) => PauseScope::None,
        (true, false) => PauseScope::Pr,
        (false, true) => PauseScope::Cli,
        (true, true) => PauseScope::All,
    }
}

#[must_use]
pub const fn pr_reviews_paused(scope: PauseScope) -> bool {
    matches!(scope, PauseScope::Pr | PauseScope::All)
}

#[must_use]
pub const fn cli_reviews_paused(scope: PauseScope) -> bool {
    matches!(scope, PauseScope::Cli | PauseScope::All)
}

/// Admit a vendor call unless the owner paused CLI use.
/// The local status stays readable.
#[must_use]
pub fn service_access(scope: PauseScope, local_status: bool) -> bool {
    !cli_reviews_paused(scope) || local_status
}

#[must_use]
pub fn cooldown_complete(previous: u64, now: u64, gap: u64) -> bool {
    gap > 0 && now >= previous && now - previous >= gap
}

/// Whether one more review fits the rolling hour and day of a ledger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Budget {
    Admit,
    /// A record lies after now, so clock rollback prevents window counting.
    Backwards,
    /// The rolling hour or day has no allowance left.
    Exhausted,
}

/// The records of `records` less than `window` milliseconds before `now`, all at or before `now`.
fn within(records: &[u64], now: u64, window: u64) -> usize {
    records
        .iter()
        .filter(|at| now.saturating_sub(**at) < window)
        .count()
}

/// The verdict on one more review at `now` over the ledger `records`.
/// This contract covers one call, and `specs/review_ledger.qnt` owns the rolling property over sequences of calls.
#[cfg_attr(kani, kani::ensures(|budget: &Budget| (*budget == Budget::Backwards) == records.iter().any(|at| *at > now)))]
#[cfg_attr(kani, kani::ensures(|budget: &Budget| *budget != Budget::Admit || (within(records, now, HOUR_MS) < HOURLY_LIMIT && within(records, now, DAY_MS) < DAILY_LIMIT)))]
#[must_use]
pub fn check_budget(records: &[u64], now: u64) -> Budget {
    if records.iter().any(|at| *at > now) {
        Budget::Backwards
    } else if within_limits(within(records, now, HOUR_MS), within(records, now, DAY_MS)) {
        Budget::Admit
    } else {
        Budget::Exhausted
    }
}

/// What an invocation of the `CodeRabbit` CLI does to the review budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Invocation {
    ReadOnly,
    Review,
    /// Paid reviews, inline API keys, remote scopes, cloud coding, and self-update.
    Forbidden,
}

/// Whether an argument starts a paid or remote review, or carries an inline API key.
fn paid_or_remote(argument: &str) -> bool {
    ["--use-credits", "--api-key", "--remote"]
        .iter()
        .any(|flag| {
            argument == *flag
                || argument
                    .strip_prefix(flag)
                    .is_some_and(|rest| rest.starts_with('='))
        })
}

/// Skips options with their values to find the first command word of `args`.
#[must_use]
pub fn first_command<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut rest = args;
    while let [argument, tail @ ..] = rest {
        rest = tail;
        if !argument.starts_with('-') {
            return Some(argument);
        }
        let (flag, inline_value) = argument
            .split_once('=')
            .map_or((*argument, false), |(flag, _)| (flag, true));
        match flag {
            "-c" | "--config" => {
                while let [value, tail @ ..] = rest {
                    if value.starts_with('-') {
                        break;
                    }
                    rest = tail;
                }
            }
            "--deep" if !inline_value => {
                // Kani parses the whole module as an older edition, which has no let chains.
                if let Some((_, tail)) = rest
                    .split_first()
                    .filter(|(value, _)| !value.starts_with('-'))
                {
                    rest = tail;
                }
            }
            "--dir" | "--base" | "--base-commit" | "--source-branch" | "--region"
                if !inline_value =>
            {
                rest = rest.get(1..).unwrap_or_default();
            }
            _ => {}
        }
    }
    None
}

/// Whether the invocation queries the usage the guard probes.
#[must_use]
pub fn usage_query(args: &[&str]) -> bool {
    let command = first_command(args);
    command == Some("usage")
        || (matches!(command, None | Some("review")) && args.contains(&"--usage"))
}

/// Forbids paid and remote reviews, admits queries without spending the budget, and counts other invocations as reviews.
#[cfg_attr(kani, kani::ensures(|kind: &Invocation| !args.iter().any(|argument| paid_or_remote(argument)) || *kind == Invocation::Forbidden))]
#[must_use]
pub fn classify(args: &[&str]) -> Invocation {
    if args.iter().any(|argument| paid_or_remote(argument))
        || matches!(first_command(args), Some("code" | "handoff" | "update"))
    {
        return Invocation::Forbidden;
    }
    let read_only = usage_query(args)
        || matches!(args, [argument] if matches!(*argument, "--version" | "-V" | "--help" | "-h"))
        || args.first().is_some_and(|argument| {
            matches!(
                *argument,
                "auth" | "doctor" | "stats" | "usage" | "pullrequest" | "skills"
            )
        })
        || matches!(args, ["review", "findings", ..])
        || matches!(args, ["review", flag] if matches!(*flag, "--show-prompts" | "--usage" | "--help" | "-h"));
    if read_only {
        Invocation::ReadOnly
    } else {
        Invocation::Review
    }
}

#[must_use]
pub fn within_budget(limit: u64, remaining: u64) -> bool {
    remaining <= limit
        && (u128::from(limit - remaining) + 1) * BUDGET_DENOMINATOR
            <= u128::from(limit) * BUDGET_NUMERATOR
}

/// The guards `specs/review_ledger.qnt` runs at once.
pub const GUARDS: usize = 2;

/// The ticks of the model's rolling hour and day, and the last tick it reaches.
pub const LEDGER_HOUR: u8 = 1;
pub const LEDGER_DAY: u8 = 2;
pub const HORIZON: u8 = 3;

/// The ticks the model's ledger holds.
const TICKS: usize = HORIZON as usize + 1;

/// Where one guard of the model stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Stage {
    Idle,
    Locked,
    /// The guard read the pause.
    Read,
    Reserved,
    Done,
}

/// One state of the ledger model, field for field as `specs/review_ledger.qnt` declares it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct Ledger {
    pub cap: u8,
    pub now: u8,
    pub paused: bool,
    pub stage: [Stage; GUARDS],
    pub saw: [bool; GUARDS],
    pub paused_since_lock: [bool; GUARDS],
    pub holder: Option<usize>,
    pub records: [u8; TICKS],
    pub ignored_pause: bool,
}

/// The guards as they run now, or as they ran when they read the pause before the lock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Guard {
    Fixed,
    Early,
}

/// One transition of the ledger model, with the guard it moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    Tick,
    Pause,
    Resume,
    ReadEarly(usize),
    Lock(usize),
    Read(usize),
    /// The guard reserves at its clock, which lags by `skew` ticks.
    Reserve(usize, u8),
    Launch(usize),
    Crash(usize),
}

/// Starts every guard with an empty ledger and no pause, under the hourly limit `cap`.
#[must_use]
pub const fn ledger(cap: u8) -> Ledger {
    Ledger {
        cap,
        now: 0,
        paused: false,
        stage: [Stage::Idle; GUARDS],
        saw: [false; GUARDS],
        paused_since_lock: [false; GUARDS],
        holder: None,
        records: [0; TICKS],
        ignored_pause: false,
    }
}

/// The records from tick `start` up to and excluding `end`.
fn recorded(records: [u8; TICKS], start: i16, end: i16) -> u32 {
    records
        .iter()
        .zip(0i16..)
        .filter(|(_, tick)| start <= *tick && *tick < end)
        .map(|(count, _)| u32::from(*count))
        .sum()
}

/// Moves guard `guard` from `from` to `to`, or `None` when it stands elsewhere.
fn advance(state: &mut Ledger, guard: usize, from: Stage, to: Stage) -> Option<()> {
    let current = state.stage.get_mut(guard)?;
    if *current != from {
        return None;
    }
    *current = to;
    Some(())
}

/// The ledger after `event`, or `None` when the event can't happen in `state`.
#[must_use]
pub fn record(guard: Guard, state: Ledger, event: Event) -> Option<Ledger> {
    let mut next = state;
    match event {
        Event::Tick => {
            if state.now >= HORIZON {
                return None;
            }
            next.now = state.now + 1;
        }
        Event::Pause => {
            if state.paused {
                return None;
            }
            next.paused = true;
        }
        Event::Resume => {
            if !state.paused {
                return None;
            }
            next.paused = false;
            next.paused_since_lock = [false; GUARDS];
        }
        Event::ReadEarly(who) => {
            if guard == Guard::Fixed {
                return None;
            }
            advance(&mut next, who, Stage::Idle, Stage::Idle)?;
            *next.saw.get_mut(who)? = state.paused;
        }
        Event::Lock(who) => {
            if state.holder.is_some() {
                return None;
            }
            advance(&mut next, who, Stage::Idle, Stage::Locked)?;
            next.holder = Some(who);
            *next.paused_since_lock.get_mut(who)? = state.paused;
        }
        Event::Read(who) => {
            advance(&mut next, who, Stage::Locked, Stage::Read)?;
            if guard == Guard::Fixed {
                *next.saw.get_mut(who)? = state.paused;
            }
        }
        Event::Reserve(who, skew) => {
            if skew > 1 {
                return None;
            }
            advance(&mut next, who, Stage::Read, Stage::Read)?;
            let clock = state.now.saturating_sub(skew);
            let tick = i16::from(clock);
            let admitted = !*state.saw.get(who)?
                && recorded(state.records, tick + 1, i16::from(HORIZON) + 1) == 0
                && recorded(state.records, tick - i16::from(LEDGER_HOUR) + 1, tick + 1)
                    < u32::from(state.cap)
                && recorded(state.records, tick - i16::from(LEDGER_DAY) + 1, tick + 1)
                    < u32::from(state.cap) * u32::from(LEDGER_DAY / LEDGER_HOUR);
            if admitted {
                *next.stage.get_mut(who)? = Stage::Reserved;
                let slot = next.records.get_mut(usize::from(clock))?;
                *slot += 1;
                next.ignored_pause = state.ignored_pause || *state.paused_since_lock.get(who)?;
            } else {
                *next.stage.get_mut(who)? = Stage::Done;
                next.holder = None;
            }
        }
        Event::Launch(who) => {
            advance(&mut next, who, Stage::Reserved, Stage::Done)?;
            next.holder = None;
        }
        Event::Crash(who) => {
            if matches!(state.stage.get(who)?, Stage::Idle | Stage::Done) {
                return None;
            }
            *next.stage.get_mut(who)? = Stage::Done;
            if state.holder == Some(who) {
                next.holder = None;
            }
        }
    }
    Some(next)
}

/// No rolling hour holds more than the hourly limit, no rolling day more than a day of hours, and no guard reserved under a pause it took the lock under.
#[must_use]
pub fn ledger_safe(state: &Ledger) -> bool {
    let hours = u32::from(LEDGER_DAY / LEDGER_HOUR);
    (0..=i16::from(HORIZON)).all(|tick| {
        recorded(state.records, tick, tick + i16::from(LEDGER_HOUR)) <= u32::from(state.cap)
            && recorded(state.records, tick, tick + i16::from(LEDGER_DAY))
                <= u32::from(state.cap) * hours
    }) && !state.ignored_pause
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// A count fits the budget exactly when 25 times it stays within 18 times the limit, so the budget floors once.
    pub fn spec_budget(limit: u64, count: u64) -> u64 {
        let budget = budget(limit);
        assert!(budget <= limit);
        assert_eq!(
            count <= budget,
            u128::from(count) * BUDGET_DENOMINATOR <= u128::from(limit) * BUDGET_NUMERATOR
        );
        budget
    }

    /// A review fits the remaining budget when the consumed count after it stays within the layered budget of the limit.
    pub fn spec_within_budget(limit: u64, remaining: u64) -> bool {
        let accepted = within_budget(limit, remaining);
        assert_eq!(
            accepted,
            remaining <= limit
                && (u128::from(limit - remaining) + 1) * 100 <= u128::from(limit) * 9 * 8
        );
        if accepted {
            assert!(limit > 0 && remaining > 0);
        }
        accepted
    }

    pub fn spec_within_limits(hourly: usize, daily: usize) {
        assert_eq!(
            within_limits(hourly, daily),
            hourly < HOURLY_LIMIT && daily < DAILY_LIMIT
        );
    }

    pub fn spec_cooldown_complete(previous: u64, now: u64, gap: u64) {
        assert_eq!(
            cooldown_complete(previous, now, gap),
            gap != 0 && now >= previous && now - previous >= gap
        );
    }

    pub fn spec_pause([all, pr, cli, local_status]: [bool; 4]) {
        let scope = pause_scope(all, pr, cli);
        assert_eq!(pr_reviews_paused(scope), all || pr);
        assert_eq!(cli_reviews_paused(scope), all || cli);
        assert_eq!(scope == PauseScope::All, all || (pr && cli));
        assert_eq!(scope == PauseScope::None, !all && !pr && !cli);
        assert_eq!(
            service_access(scope, local_status),
            !(all || cli) || local_status
        );
    }

    /// The words a bounded argument list draws from: commands, options with and without values, and the paid and remote switches.
    pub const WORDS: [&str; 12] = [
        "review",
        "--use-credits",
        "--api-key=k",
        "--remote",
        "--remote=x",
        "code",
        "usage",
        "--usage",
        "-c",
        "--dir",
        "findings",
        "--help",
    ];

    /// A paid or remote switch anywhere forbids the invocation, so it never reads only, and a forbidden command never reviews.
    pub fn spec_classify(args: &[&str]) -> Invocation {
        let kind = classify(args);
        let paid = args.iter().any(|argument| {
            ["--use-credits", "--api-key", "--remote"]
                .iter()
                .any(|flag| {
                    argument == flag
                        || argument
                            .strip_prefix(flag)
                            .is_some_and(|rest| rest.starts_with('='))
                })
        });
        if paid {
            assert_eq!(kind, Invocation::Forbidden);
        }
        if kind == Invocation::ReadOnly {
            assert!(!paid);
        }
        if usage_query(args)
            && !paid
            && !matches!(first_command(args), Some("code" | "handoff" | "update"))
        {
            assert_eq!(kind, Invocation::ReadOnly);
        }
        kind
    }

    /// One call admits a review only when the ledger holds no future record and both windows have room.
    pub fn spec_check_budget(records: &[u64], now: u64) -> Budget {
        let budget = check_budget(records, now);
        let backwards = records.iter().any(|at| *at > now);
        assert_eq!(budget == Budget::Backwards, backwards);
        let hourly = records
            .iter()
            .filter(|at| **at <= now && now - **at < HOUR_MS)
            .count();
        let daily = records
            .iter()
            .filter(|at| **at <= now && now - **at < DAY_MS)
            .count();
        if !backwards {
            assert_eq!(
                budget == Budget::Admit,
                hourly < HOURLY_LIMIT && daily < DAILY_LIMIT
            );
        }
        budget
    }

    #[cfg(kani)]
    pub const STAGES: [Stage; 5] = [
        Stage::Idle,
        Stage::Locked,
        Stage::Read,
        Stage::Reserved,
        Stage::Done,
    ];

    /// The facts every reachable state of the fixed guards keeps, field for field as `inductive` of `specs/review_ledger.qnt`.
    pub fn ledger_invariant(state: &Ledger) -> bool {
        let holds = |guard: usize| {
            state
                .stage
                .get(guard)
                .is_some_and(|stage| matches!(stage, Stage::Locked | Stage::Read | Stage::Reserved))
        };
        (1..=2).contains(&state.cap)
            && state.now <= HORIZON
            && state
                .records
                .iter()
                .zip(0u8..)
                .all(|(count, tick)| tick <= state.now || *count == 0)
            && (0..GUARDS).all(|guard| (state.holder == Some(guard)) == holds(guard))
            && state.holder.is_none_or(|holder| holder < GUARDS)
            && (0..GUARDS).all(|guard| {
                let since = state.paused_since_lock.get(guard).copied().unwrap_or(false);
                (state.stage.get(guard) != Some(&Stage::Read)
                    || !since
                    || state.saw.get(guard).copied().unwrap_or(false))
                    && (!since || state.paused)
            })
            && ledger_safe(state)
    }

    /// The fixed guards keep the invariant through every event, so no sequence of reservations passes a rolling limit or a pause.
    pub fn spec_record(state: Ledger, event: Event) -> Option<Ledger> {
        let next = record(Guard::Fixed, state, event);
        if ledger_invariant(&state) {
            assert!(ledger_safe(&state));
            if let Some(next) = next {
                assert!(ledger_invariant(&next));
            }
        }
        next
    }

    #[cfg(kani)]
    fn any_ledger() -> Ledger {
        let stage = || STAGES[usize::from(kani::any::<u8>() % 5)];
        let holder: Option<usize> = if kani::any() {
            Some(usize::from(kani::any::<u8>() % 3))
        } else {
            None
        };
        Ledger {
            cap: kani::any::<u8>() % 4,
            now: kani::any::<u8>() % 5,
            paused: kani::any(),
            stage: [stage(), stage()],
            saw: kani::any(),
            paused_since_lock: kani::any(),
            holder,
            records: [
                kani::any::<u8>() % 4,
                kani::any::<u8>() % 4,
                kani::any::<u8>() % 4,
                kani::any::<u8>() % 4,
            ],
            ignored_pause: kani::any(),
        }
    }

    #[cfg(kani)]
    fn any_event() -> Event {
        let who = usize::from(kani::any::<u8>() % 3);
        match kani::any::<u8>() % 9 {
            0 => Event::Tick,
            1 => Event::Pause,
            2 => Event::Resume,
            3 => Event::ReadEarly(who),
            4 => Event::Lock(who),
            5 => Event::Read(who),
            6 => Event::Reserve(who, kani::any::<u8>() % 3),
            7 => Event::Launch(who),
            _ => Event::Crash(who),
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_event_of_the_fixed_guards_keeps_the_invariant_that_implies_the_rolling_limits() {
        assert!(ledger_invariant(&ledger(1)) && ledger_invariant(&ledger(2)));
        let state = any_ledger();
        let event = any_event();
        let next = spec_record(state, event);
        kani::cover!(
            ledger_invariant(&state) && next.is_some_and(|next| next.stage[0] == Stage::Reserved)
        );
        kani::cover!(
            ledger_invariant(&state)
                && next.is_some_and(|next| next.stage[1] == Stage::Done && next.saw[1])
        );
    }

    #[cfg(kani)]
    #[kani::proof_for_contract(check_budget)]
    #[kani::unwind(10)]
    fn one_budget_check_admits_only_a_review_both_windows_hold() {
        let records: [u64; 8] = kani::any();
        let length = usize::from(kani::any::<u8>() % 9);
        let budget = check_budget(&records[..length], kani::any());
        kani::cover!(budget == Budget::Exhausted);
        kani::cover!(budget == Budget::Admit);
    }

    #[cfg(kani)]
    #[kani::proof_for_contract(classify)]
    #[kani::unwind(10)]
    fn a_paid_or_remote_switch_never_reads_only() {
        let args = [
            WORDS[usize::from(kani::any::<u8>() % 12)],
            WORDS[usize::from(kani::any::<u8>() % 12)],
            WORDS[usize::from(kani::any::<u8>() % 12)],
        ];
        let length = usize::from(kani::any::<u8>() % 4);
        let kind = spec_classify(&args[..length]);
        kani::cover!(kind == Invocation::ReadOnly);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_count_fits_the_layered_budget_exactly_when_25_times_it_stays_within_18_times_the_limit() {
        let limit: u64 = kani::any();
        let count: u64 = kani::any();
        let budget = spec_budget(limit, count);
        kani::cover!(limit > 0 && budget == 0);
        kani::cover!(count == budget && count > 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn review_consumption_respects_the_layered_budget() {
        let limit: u64 = kani::any();
        let remaining: u64 = kani::any();
        let accepted = spec_within_budget(limit, remaining);
        kani::cover!(accepted);
        kani::cover!(!accepted && limit > 0 && remaining <= limit);
        kani::cover!(!accepted && remaining > limit);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn rolling_attempts_cannot_exceed_the_layered_local_budget() {
        let hourly: usize = kani::any();
        let daily: usize = kani::any();
        spec_within_limits(hourly, daily);
        let accepted = within_limits(hourly, daily);
        kani::cover!(accepted);
        kani::cover!(!accepted && hourly >= HOURLY_LIMIT);
        kani::cover!(!accepted && daily >= DAILY_LIMIT);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn cooldown_refuses_rollback_and_every_incomplete_interval() {
        let previous: u64 = kani::any();
        let now: u64 = kani::any();
        let gap: u64 = kani::any();
        spec_cooldown_complete(previous, now, gap);
        let accepted = cooldown_complete(previous, now, gap);
        kani::cover!(accepted);
        kani::cover!(!accepted && now < previous);
        kani::cover!(!accepted && gap > 0 && now >= previous && now - previous < gap);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn the_pause_scope_is_the_union_of_its_markers_and_leaves_only_local_status() {
        let all: bool = kani::any();
        let pr: bool = kani::any();
        let cli: bool = kani::any();
        let local_status: bool = kani::any();
        spec_pause([all, pr, cli, local_status]);
        let scope = pause_scope(all, pr, cli);
        kani::cover!(scope == PauseScope::Pr && service_access(scope, false));
        kani::cover!(!service_access(scope, local_status));
        kani::cover!(scope == PauseScope::None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Capacities and counts around every rounding step of the budget and both ends of the range.
    const VALUES: [u64; 12] = [0, 1, 2, 3, 7, 8, 9, 10, 49, 50, u64::MAX - 1, u64::MAX];

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every boundary")]
    fn every_boundary_meets_the_budget_specifications() {
        for first in VALUES {
            for second in VALUES {
                proofs::spec_budget(first, second);
                proofs::spec_within_budget(first, second);
                for third in VALUES {
                    proofs::spec_cooldown_complete(first, second, third);
                }
            }
        }
        for limit in 0..=100 {
            for count in 0..=100 {
                proofs::spec_budget(limit, count);
                proofs::spec_within_budget(limit, count);
            }
        }
    }

    /// The model checker's count of distinct states of `specs/review_ledger.qnt`, which `policy/obligations.toml` records too.
    const DISTINCT_STATES: usize = 1740;

    fn events() -> alloc::vec::Vec<Event> {
        let mut events = alloc::vec::Vec::from([Event::Tick, Event::Pause, Event::Resume]);
        for who in 0..GUARDS {
            events.extend([
                Event::ReadEarly(who),
                Event::Lock(who),
                Event::Read(who),
                Event::Reserve(who, 0),
                Event::Reserve(who, 1),
                Event::Launch(who),
                Event::Crash(who),
            ]);
        }
        events
    }

    /// Every state the guards reach from the initial ledgers of every limit up to two.
    fn reachable(guard: Guard) -> alloc::collections::BTreeSet<Ledger> {
        let mut seen = alloc::collections::BTreeSet::from([ledger(1), ledger(2)]);
        let mut frontier = alloc::vec::Vec::from([ledger(1), ledger(2)]);
        while let Some(state) = frontier.pop() {
            for event in events() {
                if let Some(next) = record(guard, state, event).filter(|next| seen.insert(*next)) {
                    frontier.push(next);
                }
            }
        }
        seen
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every reachable ledger")]
    fn the_fixed_guards_reach_exactly_the_states_tlc_counts_and_each_is_safe() {
        let states = reachable(Guard::Fixed);
        assert_eq!(states.len(), DISTINCT_STATES);
        for state in &states {
            assert!(proofs::ledger_invariant(state), "{state:?}");
            for event in events() {
                proofs::spec_record(*state, event);
            }
        }
        let mut state = ledger(1);
        for event in [
            Event::ReadEarly(0),
            Event::Pause,
            Event::Lock(0),
            Event::Read(0),
            Event::Reserve(0, 0),
        ] {
            state = record(Guard::Early, state, event).unwrap_or(state);
        }
        assert!(!ledger_safe(&state));
        assert!(
            reachable(Guard::Early)
                .iter()
                .any(|state| !ledger_safe(state))
        );
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri takes seconds over every bounded argument list and ledger"
    )]
    fn every_bounded_argument_list_and_ledger_meets_its_specification() {
        let words = proofs::WORDS;
        for first in words {
            proofs::spec_classify(&[first]);
            for second in words {
                proofs::spec_classify(&[first, second]);
                for third in words {
                    proofs::spec_classify(&[first, second, third]);
                }
            }
        }
        proofs::spec_classify(&[]);
        let times = [0, 1, HOUR_MS - 1, HOUR_MS, DAY_MS - 1, DAY_MS, DAY_MS + 1];
        for now in times {
            for count in 0..=8 {
                for at in times {
                    proofs::spec_check_budget(&[at; 8][..count], now);
                }
            }
        }
        assert_eq!(
            check_budget(&[1, 2, 3, 4, 5, 6, 7], HOUR_MS),
            Budget::Exhausted
        );
        assert_eq!(
            check_budget(&[1, 2, 3, 4, 5, 6, 7], HOUR_MS + 1),
            Budget::Admit
        );
        assert_eq!(check_budget(&[100], 99), Budget::Backwards);
        assert_eq!(classify(&["--deep", "x", "code"]), Invocation::Forbidden);
        assert_eq!(classify(&["--dir", "usage", "review"]), Invocation::Review);
        assert_eq!(
            first_command(&["-c", "a", "b", "--x", "review"]),
            Some("review")
        );
    }

    #[test]
    fn every_window_count_and_pause_meets_its_specification() {
        for hourly in [0, 1, HOURLY_LIMIT - 1, HOURLY_LIMIT, usize::MAX] {
            for daily in [0, 1, DAILY_LIMIT - 1, DAILY_LIMIT, usize::MAX] {
                proofs::spec_within_limits(hourly, daily);
            }
        }
        for markers in crate::domain::flags() {
            proofs::spec_pause(markers);
        }
    }

    #[test]
    fn ten_reviews_an_hour_leave_seven_and_a_day_of_them_168() {
        assert_eq!(budget(10), 7);
        assert_eq!(HOURLY_LIMIT, 7);
        assert_eq!(DAILY_LIMIT, 168);
        assert_eq!(PROBE_DAILY_LIMIT, DAILY_LIMIT);
        assert_eq!(budget(49), 35);
        assert_eq!(budget(u64::MAX), 13_281_655_733_070_877_162);
    }

    #[test]
    fn each_pause_scope_has_its_own_name() {
        assert_eq!(
            [
                PauseScope::None,
                PauseScope::Pr,
                PauseScope::Cli,
                PauseScope::All
            ]
            .map(PauseScope::name),
            ["none", "pr", "cli", "all"]
        );
    }
}
