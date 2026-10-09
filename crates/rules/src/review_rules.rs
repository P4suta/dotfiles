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

/// Whether both rolling windows have room for one more review.
fn room(records: &[u64], now: u64) -> bool {
    let mut hourly = 0;
    let mut daily = 0;
    let hour_start = now.saturating_sub(HOUR_MS);
    let day_start = now.saturating_sub(DAY_MS);
    for index in 0..records.len() {
        let at = records.get(index).copied().unwrap_or_default();
        hourly += usize::from(now < HOUR_MS || at > hour_start);
        daily += usize::from(now < DAY_MS || at > day_start);
    }
    within_limits(hourly, daily)
}

fn future(records: &[u64], now: u64) -> bool {
    for index in 0..records.len() {
        if records.get(index).is_some_and(|at| *at > now) {
            return true;
        }
    }
    false
}

/// The verdict on one more review at `now` over the ledger `records`.
/// This contract covers one call, and `specs/review_ledger.qnt` owns the rolling property over sequences of calls.
#[must_use]
pub fn check_budget(records: &[u64], now: u64) -> Budget {
    if future(records, now) {
        Budget::Backwards
    } else if room(records, now) {
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
    let switch = |flag: &str| {
        let bytes = argument.as_bytes();
        if bytes.len() < flag.len() {
            return false;
        }
        for index in 0..flag.len() {
            if bytes.get(index) != flag.as_bytes().get(index) {
                return false;
            }
        }
        bytes.get(flag.len()).is_none_or(|byte| *byte == b'=')
    };
    switch("--use-credits") || switch("--api-key") || switch("--remote")
}

fn paid(args: &[&str]) -> bool {
    for index in 0..args.len() {
        if args
            .get(index)
            .is_some_and(|argument| paid_or_remote(argument))
        {
            return true;
        }
    }
    false
}

fn word_is(argument: &str, word: &str) -> bool {
    if argument.len() != word.len() {
        return false;
    }
    for index in 0..word.len() {
        if argument.as_bytes().get(index) != word.as_bytes().get(index) {
            return false;
        }
    }
    true
}

fn one_of(argument: &str, words: &[&str]) -> bool {
    for index in 0..words.len() {
        if words.get(index).is_some_and(|word| word_is(argument, word)) {
            return true;
        }
    }
    false
}

const FORBIDDEN_COMMANDS: [&str; 3] = ["code", "handoff", "update"];
const COMMAND_AND_OPTION: usize = 2;

/// Skips options with their values to find the first command word of `args`.
#[must_use]
pub fn first_command<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut position = 0;
    for _ in 0..args.len() {
        let Some(argument) = args.get(position) else {
            break;
        };
        position += 1;
        if argument.as_bytes().first() != Some(&b'-') {
            return Some(argument);
        }
        let mut equal = None;
        for index in 0..argument.len() {
            if argument.as_bytes().get(index) == Some(&b'=') {
                equal = Some(index);
                break;
            }
        }
        let (flag, inline_value) = equal
            .and_then(|index| argument.get(..index))
            .map_or((*argument, false), |flag| (flag, true));
        if one_of(flag, &["-c", "--config"]) {
            for _ in 0..args.len() {
                let Some(value) = args.get(position) else {
                    break;
                };
                if value.as_bytes().first() == Some(&b'-') {
                    break;
                }
                position += 1;
            }
        } else if word_is(flag, "--deep") && !inline_value {
            if args
                .get(position)
                .is_some_and(|value| value.as_bytes().first() != Some(&b'-'))
            {
                position += 1;
            }
        } else if one_of(
            flag,
            &[
                "--dir",
                "--base",
                "--base-commit",
                "--source-branch",
                "--region",
            ],
        ) && !inline_value
        {
            position += 1;
        }
    }
    None
}

/// Whether the invocation queries the usage the guard probes.
#[must_use]
pub fn usage_query(args: &[&str]) -> bool {
    usage(args, first_command(args))
}

fn usage(args: &[&str], command: Option<&str>) -> bool {
    if command.is_some_and(|command| word_is(command, "usage")) {
        return true;
    }
    if command.is_none_or(|command| word_is(command, "review")) {
        for index in 0..args.len() {
            if args
                .get(index)
                .is_some_and(|argument| word_is(argument, "--usage"))
            {
                return true;
            }
        }
    }
    false
}

/// Forbids paid and remote reviews, admits queries without spending the budget, and counts other invocations as reviews.
#[must_use]
pub fn classify(args: &[&str]) -> Invocation {
    if paid(args) {
        return Invocation::Forbidden;
    }
    let command = first_command(args);
    if command.is_some_and(|command| one_of(command, &FORBIDDEN_COMMANDS)) {
        return Invocation::Forbidden;
    }
    let first = args.first().copied().unwrap_or_default();
    let second = args.get(1).copied().unwrap_or_default();
    let read_only = usage(args, command)
        || (args.len() == 1 && one_of(first, &["--version", "-V", "--help", "-h"]))
        || one_of(
            first,
            &["auth", "doctor", "stats", "usage", "pullrequest", "skills"],
        )
        || (word_is(first, "review") && word_is(second, "findings"))
        || (args.len() == COMMAND_AND_OPTION
            && word_is(first, "review")
            && one_of(second, &["--show-prompts", "--usage", "--help", "-h"]));
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
    let mut count = 0;
    for tick in 0..=HORIZON {
        if start <= i16::from(tick) && i16::from(tick) < end {
            count += u32::from(records.get(usize::from(tick)).copied().unwrap_or_default());
        }
    }
    count
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
                    < u32::from(state.cap) * u32::from(LEDGER_DAY);
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
    state.records.iter().all(|count| *count <= state.cap) && !state.ignored_pause
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_ledger_safe(state: &Ledger) -> bool {
        let expected = (0..=i16::from(HORIZON)).all(|tick| {
            recorded(state.records, tick, tick + i16::from(LEDGER_HOUR)) <= u32::from(state.cap)
                && recorded(state.records, tick, tick + i16::from(LEDGER_DAY))
                    <= u32::from(state.cap) * u32::from(LEDGER_DAY / LEDGER_HOUR)
        }) && !state.ignored_pause;
        let safe = ledger_safe(state);
        assert_eq!(safe, expected);
        safe
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_ledger_keeps_exactly_both_rolling_limits_and_the_pause() {
        let mut state = ledger(kani::any());
        state.records = kani::any();
        state.ignored_pause = kani::any();
        let safe = spec_ledger_safe(&state);
        kani::cover!(safe);
        kani::cover!(!safe & state.ignored_pause);
        kani::cover!(!safe & (state.records[0] > state.cap));
    }

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

    pub fn spec_classify_codes(codes: &[u8]) -> Invocation {
        assert!(codes.len() <= 3);
        let args = [
            WORDS
                .get(usize::from(codes.first().copied().unwrap_or_default()))
                .copied()
                .unwrap_or_default(),
            WORDS
                .get(usize::from(codes.get(1).copied().unwrap_or_default()))
                .copied()
                .unwrap_or_default(),
            WORDS
                .get(usize::from(codes.get(2).copied().unwrap_or_default()))
                .copied()
                .unwrap_or_default(),
        ];
        let actual = classify(args.get(..codes.len()).unwrap_or_default());
        assert_classify_result(codes, actual);
        actual
    }

    fn assert_classify_result(codes: &[u8], actual: Invocation) {
        let mut paid = false;
        let mut usage = false;
        for index in 0..codes.len() {
            paid |= codes.get(index).is_some_and(|code| matches!(code, 1..=4));
            usage |= codes.get(index) == Some(&7);
        }
        let mut position = 0;
        let mut command = None;
        while let Some(code) = codes.get(position) {
            position += 1;
            if matches!(code, 0 | 5 | 6 | 10) {
                command = Some(*code);
                break;
            }
            if *code == 8 {
                while codes
                    .get(position)
                    .is_some_and(|code| matches!(code, 0 | 5 | 6 | 10))
                {
                    position += 1;
                }
            } else if *code == 9 {
                position += 1;
            }
        }
        let expected = if paid || command == Some(5) {
            Invocation::Forbidden
        } else if command == Some(6)
            || (matches!(command, None | Some(0)) && usage)
            || (codes.len() == 1 && codes.first() == Some(&11))
            || codes.first() == Some(&6)
            || (codes.first() == Some(&0) && codes.get(1) == Some(&10))
            || (codes.len() == 2 && codes.first() == Some(&0) && codes.get(1) == Some(&11))
        {
            Invocation::ReadOnly
        } else {
            Invocation::Review
        };
        assert_eq!(actual, expected);
        if paid {
            assert_eq!(actual, Invocation::Forbidden);
        }
    }

    /// A paid or remote switch anywhere forbids the invocation, so it never reads only, and a forbidden command never reviews.
    pub fn spec_classify(args: &[&str]) -> Invocation {
        let kind = classify(args);
        let mut paid = false;
        for index in 0..args.len() {
            let argument = args.get(index).copied().unwrap_or_default().as_bytes();
            let mut end = argument.len();
            for byte in 0..argument.len() {
                if argument.get(byte) == Some(&b'=') {
                    end = byte;
                    break;
                }
            }
            for flag in [b"--use-credits".as_slice(), b"--api-key", b"--remote"] {
                if end != flag.len() {
                    continue;
                }
                let mut same = true;
                for byte in 0..end {
                    same &= argument.get(byte) == flag.get(byte);
                }
                paid |= same;
            }
        }
        if paid {
            assert_eq!(kind, Invocation::Forbidden);
        }
        if kind == Invocation::ReadOnly {
            assert!(!paid);
        }
        if !paid
            && usage_query(args)
            && !first_command(args).is_some_and(|command| one_of(command, &FORBIDDEN_COMMANDS))
        {
            assert_eq!(kind, Invocation::ReadOnly);
        }
        kind
    }

    /// One call admits a review only when the ledger holds no future record and both windows have room.
    pub fn spec_check_budget(records: &[u64], now: u64) -> Budget {
        let budget = check_budget(records, now);
        assert_budget_result(records, now, budget);
        budget
    }

    fn assert_budget_result(records: &[u64], now: u64, budget: Budget) {
        let mut backwards = false;
        for index in 0..records.len() {
            let at = records.get(index).copied().unwrap_or_default();
            backwards |= at > now;
        }
        assert_eq!(budget == Budget::Backwards, backwards);
        assert!(budget != Budget::Admit || room(records, now));
        if !backwards {
            let mut hourly = 0;
            let mut daily = 0;
            for index in 0..records.len() {
                let at = records.get(index).copied().unwrap_or_default();
                if at <= now {
                    hourly += usize::from(now - at < HOUR_MS);
                    daily += usize::from(now - at < DAY_MS);
                }
            }
            assert_eq!(
                budget == Budget::Admit,
                hourly < HOURLY_LIMIT && daily < DAILY_LIMIT
            );
        }
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
            && (0..=HORIZON)
                .all(|tick| tick <= state.now || state.records.get(usize::from(tick)) == Some(&0))
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
        if let Some(next) = next {
            assert_eq!(next.cap, state.cap);
            assert!(next.now >= state.now);
        }
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
    fn any_event(case: u8) -> Event {
        let who = usize::from(kani::any::<u8>() % 3);
        match case {
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
        let (state, next) = bounded_event(0);
        kani::cover!(ledger_invariant(&state) && state.cap == 1 && next.is_some());
        kani::cover!(ledger_invariant(&state) && state.cap == 2 && next.is_some());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_pause_of_the_fixed_guards_keeps_the_rolling_invariant() {
        let (state, next) = bounded_event(1);
        kani::cover!(ledger_invariant(&state) && state.cap == 1 && next.is_some());
        kani::cover!(ledger_invariant(&state) && state.cap == 2 && next.is_some());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_resume_of_the_fixed_guards_keeps_the_rolling_invariant() {
        let (state, next) = bounded_event(2);
        kani::cover!(ledger_invariant(&state) && state.cap == 1 && next.is_some());
        kani::cover!(ledger_invariant(&state) && state.cap == 2 && next.is_some());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_early_read_of_the_fixed_guards_keeps_the_rolling_invariant() {
        let (state, next) = bounded_event(3);
        kani::cover!(ledger_invariant(&state) && state.cap == 1 && next.is_none());
        kani::cover!(ledger_invariant(&state) && state.cap == 2 && next.is_none());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_lock_of_the_fixed_guards_keeps_the_rolling_invariant() {
        let (state, next) = bounded_event(4);
        kani::cover!(ledger_invariant(&state) && state.cap == 1 && next.is_some());
        kani::cover!(ledger_invariant(&state) && state.cap == 2 && next.is_some());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_read_of_the_fixed_guards_keeps_the_rolling_invariant() {
        let (state, next) = bounded_event(5);
        kani::cover!(ledger_invariant(&state) && state.cap == 1 && next.is_some());
        kani::cover!(ledger_invariant(&state) && state.cap == 2 && next.is_some());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_reserve_of_the_fixed_guards_keeps_the_rolling_invariant() {
        let (state, next) = bounded_reserve(0);
        kani::cover!(
            ledger_invariant(&state) && next.is_some_and(|next| next.stage[0] == Stage::Reserved)
        );
        kani::cover!(
            ledger_invariant(&state)
                && next.is_some_and(|next| next.stage[1] == Stage::Done && next.saw[1])
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_reserve_of_the_second_guard_keeps_the_rolling_invariant() {
        let (state, next) = bounded_reserve(1);
        kani::cover!(
            ledger_invariant(&state) && next.is_some_and(|next| next.stage[1] == Stage::Reserved)
        );
        kani::cover!(
            ledger_invariant(&state)
                && next.is_some_and(|next| next.stage[1] == Stage::Done && next.saw[1])
        );
    }

    #[cfg(kani)]
    fn bounded_reserve(who: usize) -> (Ledger, Option<Ledger>) {
        assert!(ledger_invariant(&ledger(1)) && ledger_invariant(&ledger(2)));
        let state = any_ledger();
        kani::assume(ledger_invariant(&state));
        let next = spec_record(state, Event::Reserve(who, kani::any::<u8>() % 2));
        (state, next)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_launch_of_the_fixed_guards_keeps_the_rolling_invariant() {
        let (state, next) = bounded_event(7);
        kani::cover!(ledger_invariant(&state) && state.cap == 1 && next.is_some());
        kani::cover!(ledger_invariant(&state) && state.cap == 2 && next.is_some());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_crash_of_the_fixed_guards_keeps_the_rolling_invariant() {
        let (state, next) = bounded_event(8);
        kani::cover!(ledger_invariant(&state) && state.cap == 1 && next.is_some());
        kani::cover!(ledger_invariant(&state) && state.cap == 2 && next.is_some());
    }

    #[cfg(kani)]
    fn bounded_event(case: u8) -> (Ledger, Option<Ledger>) {
        assert!(ledger_invariant(&ledger(1)) && ledger_invariant(&ledger(2)));
        let state = any_ledger();
        kani::assume(ledger_invariant(&state));
        let next = spec_record(state, any_event(case));
        (state, next)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_event_of_an_invalid_ledger_keeps_the_guarded_specification() {
        let state = any_ledger();
        kani::assume(!ledger_invariant(&state));
        let next = spec_record(state, any_event(kani::any::<u8>() % 9));
        kani::cover!(next.is_some());
        kani::cover!(next.is_none());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn one_budget_check_admits_only_a_review_both_windows_hold() {
        let (budget, _) = bounded_budget::<8>();
        kani::cover!(budget == Budget::Admit);
        kani::cover!(budget == Budget::Exhausted);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_budget_check_of_0_records_meets_both_windows() {
        let (budget, now) = bounded_budget::<0>();
        kani::cover!(budget == Budget::Admit && now == 0);
        kani::cover!(budget == Budget::Admit && now == u64::MAX);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_budget_check_of_1_records_meets_both_windows() {
        let (budget, now) = bounded_budget::<1>();
        kani::cover!(budget == Budget::Admit && now == 0);
        kani::cover!(budget == Budget::Admit && now == u64::MAX);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_budget_check_of_2_records_meets_both_windows() {
        let (budget, now) = bounded_budget::<2>();
        kani::cover!(budget == Budget::Admit && now == 0);
        kani::cover!(budget == Budget::Admit && now == u64::MAX);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_budget_check_of_3_records_meets_both_windows() {
        let (budget, now) = bounded_budget::<3>();
        kani::cover!(budget == Budget::Admit && now == 0);
        kani::cover!(budget == Budget::Admit && now == u64::MAX);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_budget_check_of_4_records_meets_both_windows() {
        let (budget, now) = bounded_budget::<4>();
        kani::cover!(budget == Budget::Admit && now == 0);
        kani::cover!(budget == Budget::Admit && now == u64::MAX);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_budget_check_of_5_records_meets_both_windows() {
        let (budget, now) = bounded_budget::<5>();
        kani::cover!(budget == Budget::Admit && now == 0);
        kani::cover!(budget == Budget::Admit && now == u64::MAX);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_budget_check_of_6_records_meets_both_windows() {
        let (budget, now) = bounded_budget::<6>();
        kani::cover!(budget == Budget::Admit && now == 0);
        kani::cover!(budget == Budget::Admit && now == u64::MAX);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_budget_check_of_7_records_meets_both_windows() {
        let (budget, _) = bounded_budget::<7>();
        kani::cover!(budget == Budget::Admit);
        kani::cover!(budget == Budget::Exhausted);
    }

    #[cfg(kani)]
    fn bounded_budget<const N: usize>() -> (Budget, u64) {
        let records: [u64; N] = kani::any();
        let now: u64 = kani::any();
        for index in 0..N {
            kani::assume(records.get(index).is_some_and(|at| *at <= now));
        }
        let budget = check_budget(&records, now);
        assert_budget_result(&records, now, budget);
        (budget, now)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_future_record_prevents_every_bounded_budget_check() {
        let (budget, now) = bounded_future_at::<0>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    fn bounded_future<const N: usize>() -> (Budget, u64) {
        let records: [u64; N] = kani::any();
        let now: u64 = kani::any();
        kani::assume(records.iter().any(|at| *at > now));
        let budget = check_budget(&records, now);
        assert_budget_result(&records, now, budget);
        (budget, now)
    }

    #[cfg(kani)]
    fn bounded_future_at<const FIRST: usize>() -> (Budget, u64) {
        let records: [u64; 8] = kani::any();
        let now: u64 = kani::any();
        for index in 0..FIRST {
            kani::assume(records.get(index).is_some_and(|at| *at <= now));
        }
        kani::assume(records.get(FIRST).is_some_and(|at| *at > now));
        let budget = check_budget(&records, now);
        assert_budget_result(&records, now, budget);
        (budget, now)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_future_record_at_1_prevents_the_eight_record_budget_check() {
        let (budget, now) = bounded_future_at::<1>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_future_record_at_2_prevents_the_eight_record_budget_check() {
        let (budget, now) = bounded_future_at::<2>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_future_record_at_3_prevents_the_eight_record_budget_check() {
        let (budget, now) = bounded_future_at::<3>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_future_record_at_4_prevents_the_eight_record_budget_check() {
        let (budget, now) = bounded_future_at::<4>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_future_record_at_5_prevents_the_eight_record_budget_check() {
        let (budget, now) = bounded_future_at::<5>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_future_record_at_6_prevents_the_eight_record_budget_check() {
        let (budget, now) = bounded_future_at::<6>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn a_future_record_at_7_prevents_the_eight_record_budget_check() {
        let (budget, now) = bounded_future_at::<7>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn one_future_record_prevents_a_budget_check() {
        let (budget, now) = bounded_future::<1>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn two_records_with_a_future_record_prevent_a_budget_check() {
        let (budget, now) = bounded_future::<2>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn three_records_with_a_future_record_prevent_a_budget_check() {
        let (budget, now) = bounded_future::<3>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn four_records_with_a_future_record_prevent_a_budget_check() {
        let (budget, now) = bounded_future::<4>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn five_records_with_a_future_record_prevent_a_budget_check() {
        let (budget, now) = bounded_future::<5>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn six_records_with_a_future_record_prevent_a_budget_check() {
        let (budget, now) = bounded_future::<6>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn seven_records_with_a_future_record_prevent_a_budget_check() {
        let (budget, now) = bounded_future::<7>();
        kani::cover!(budget == Budget::Backwards && now == 0);
        kani::cover!(budget == Budget::Backwards && now == 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_paid_or_remote_switch_never_reads_only() {
        let (kind, _) = bounded_classify::<3>(0);
        kani::cover!(kind == Invocation::ReadOnly);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_paid_switch_is_forbidden() {
        let (kind, second) = bounded_classify::<3>(1);
        kani::cover!(kind == Invocation::Forbidden && second == 0);
        kani::cover!(kind == Invocation::Forbidden && second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn an_inline_api_key_is_forbidden() {
        let (kind, second) = bounded_classify::<3>(2);
        kani::cover!(kind == Invocation::Forbidden && second == 0);
        kani::cover!(kind == Invocation::Forbidden && second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_remote_switch_is_forbidden() {
        let (kind, second) = bounded_classify::<3>(3);
        kani::cover!(kind == Invocation::Forbidden && second == 0);
        kani::cover!(kind == Invocation::Forbidden && second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn an_inline_remote_scope_is_forbidden() {
        let (kind, second) = bounded_classify::<3>(4);
        kani::cover!(kind == Invocation::Forbidden && second == 0);
        kani::cover!(kind == Invocation::Forbidden && second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_code_command_is_forbidden() {
        let (kind, second) = bounded_classify::<3>(5);
        kani::cover!(kind == Invocation::Forbidden && second == 0);
        kani::cover!(kind == Invocation::Forbidden && second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_usage_command_never_uses_the_review_pool() {
        let (kind, _) = bounded_classify::<3>(6);
        kani::cover!(kind == Invocation::ReadOnly);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_usage_option_never_admits_a_paid_switch() {
        let (kind, _) = classify_prefix::<7, 0>();
        kani::cover!(kind == Invocation::ReadOnly);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_config_option_never_admits_a_paid_switch() {
        let (kind, _) = classify_prefix::<8, 0>();
        kani::cover!(kind == Invocation::ReadOnly);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_directory_option_never_admits_a_paid_switch() {
        let (kind, _) = classify_prefix::<9, 0>();
        kani::cover!(kind == Invocation::ReadOnly);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_findings_word_never_admits_a_paid_switch() {
        let (kind, _) = bounded_classify::<3>(10);
        kani::cover!(kind == Invocation::Review);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_help_option_never_admits_a_paid_switch() {
        let (kind, _) = classify_prefix::<11, 0>();
        kani::cover!(kind == Invocation::ReadOnly);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_paid_or_remote_switch_never_reads_only_in_a_short_invocation() {
        let kind = every_first_word();
        kani::cover!(kind == Invocation::ReadOnly);
        kani::cover!(kind == Invocation::Forbidden);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_paid_or_remote_switch_never_reads_only_with_two_words() {
        let (_, second) = bounded_classify::<2>(0);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_paid_switch_is_forbidden_with_two_words() {
        let (_, second) = bounded_classify::<2>(1);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn an_inline_api_key_is_forbidden_with_two_words() {
        let (_, second) = bounded_classify::<2>(2);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_remote_switch_is_forbidden_with_two_words() {
        let (_, second) = bounded_classify::<2>(3);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn an_inline_remote_scope_is_forbidden_with_two_words() {
        let (_, second) = bounded_classify::<2>(4);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_code_command_is_forbidden_with_two_words() {
        let (_, second) = bounded_classify::<2>(5);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_usage_command_never_uses_the_review_pool_with_two_words() {
        let (_, second) = bounded_classify::<2>(6);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_usage_option_never_admits_a_paid_switch_with_two_words() {
        let kinds = every_second_word::<7>();
        kani::cover!(kinds.first() == Some(&Invocation::ReadOnly));
        kani::cover!(kinds.get(1) == Some(&Invocation::Forbidden));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_config_option_never_admits_a_paid_switch_with_two_words() {
        let kinds = every_second_word::<8>();
        kani::cover!(kinds.first() == Some(&Invocation::Review));
        kani::cover!(kinds.get(1) == Some(&Invocation::Forbidden));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_directory_option_never_admits_a_paid_switch_with_two_words() {
        let (_, second) = bounded_classify::<2>(9);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_findings_word_never_admits_a_paid_switch_with_two_words() {
        let (_, second) = bounded_classify::<2>(10);
        kani::cover!(second == 0);
        kani::cover!(second == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn a_help_option_never_admits_a_paid_switch_with_two_words() {
        let kinds = every_second_word::<11>();
        kani::cover!(kinds.first() == Some(&Invocation::Review));
        kani::cover!(kinds.get(1) == Some(&Invocation::Forbidden));
    }

    #[cfg(kani)]
    fn bounded_classify<const N: usize>(first: u8) -> (Invocation, usize) {
        let second = usize::from(kani::any::<u8>() % 12);
        let third = kani::any::<u8>() % 12;
        let codes = [first, u8::try_from(second).unwrap_or_default(), third];
        let kind = spec_classify_codes(codes.get(..N).unwrap_or_default());
        (kind, second)
    }

    #[cfg(kani)]
    fn every_first_word() -> Invocation {
        assert_eq!(super::classify(&[]), Invocation::Review);
        let kinds = [
            super::classify(&["review"]),
            super::classify(&["--use-credits"]),
            super::classify(&["--api-key=k"]),
            super::classify(&["--remote"]),
            super::classify(&["--remote=x"]),
            super::classify(&["code"]),
            super::classify(&["usage"]),
            super::classify(&["--usage"]),
            super::classify(&["-c"]),
            super::classify(&["--dir"]),
            super::classify(&["findings"]),
            super::classify(&["--help"]),
        ];
        for first in 0..12u8 {
            let actual = kinds
                .get(usize::from(first))
                .copied()
                .unwrap_or(Invocation::Review);
            assert_classify_result(&[first], actual);
        }
        kinds
            .get(usize::from(kani::any::<u8>() % 12))
            .copied()
            .unwrap_or(Invocation::Review)
    }

    #[cfg(kani)]
    fn every_second_word<const FIRST: u8>() -> [Invocation; 12] {
        let first = WORDS.get(usize::from(FIRST)).copied().unwrap_or_default();
        let kinds = [
            super::classify(&[first, "review"]),
            super::classify(&[first, "--use-credits"]),
            super::classify(&[first, "--api-key=k"]),
            super::classify(&[first, "--remote"]),
            super::classify(&[first, "--remote=x"]),
            super::classify(&[first, "code"]),
            super::classify(&[first, "usage"]),
            super::classify(&[first, "--usage"]),
            super::classify(&[first, "-c"]),
            super::classify(&[first, "--dir"]),
            super::classify(&[first, "findings"]),
            super::classify(&[first, "--help"]),
        ];
        for second in 0..12u8 {
            let actual = kinds
                .get(usize::from(second))
                .copied()
                .unwrap_or(Invocation::Review);
            assert_classify_result(&[FIRST, second], actual);
        }
        kinds
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_1_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 1>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_2_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 2>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_3_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 3>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_4_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 4>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_5_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 5>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_6_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 6>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_7_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 7>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_8_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 8>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_9_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 9>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_10_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 10>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_7_11_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<7, 11>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_1_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 1>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_2_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 2>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_3_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 3>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_4_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 4>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_5_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 5>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_6_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 6>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_7_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 7>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_8_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 8>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_9_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 9>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_10_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 10>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_8_11_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<8, 11>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_1_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 1>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_2_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 2>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_3_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 3>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_4_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 4>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_5_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 5>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_6_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 6>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_7_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 7>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_8_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 8>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_9_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 9>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_10_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 10>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_11_11_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<11, 11>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_1_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 1>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_2_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 2>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_3_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 3>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_4_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 4>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_5_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 5>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_6_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 6>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_7_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 7>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_8_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 8>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_9_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 9>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_10_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 10>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn classifier_prefix_9_11_keeps_every_three_word_case() {
        let (_, third) = classify_prefix::<9, 11>();
        kani::cover!(third == 0);
        kani::cover!(third == 6);
    }

    #[cfg(kani)]
    fn classify_prefix<const FIRST: u8, const SECOND: u8>() -> (Invocation, u8) {
        let first = WORDS.get(usize::from(FIRST)).copied().unwrap_or_default();
        let second = WORDS.get(usize::from(SECOND)).copied().unwrap_or_default();
        let kinds = [
            super::classify(&[first, second, "review"]),
            super::classify(&[first, second, "--use-credits"]),
            super::classify(&[first, second, "--api-key=k"]),
            super::classify(&[first, second, "--remote"]),
            super::classify(&[first, second, "--remote=x"]),
            super::classify(&[first, second, "code"]),
            super::classify(&[first, second, "usage"]),
            super::classify(&[first, second, "--usage"]),
            super::classify(&[first, second, "-c"]),
            super::classify(&[first, second, "--dir"]),
            super::classify(&[first, second, "findings"]),
            super::classify(&[first, second, "--help"]),
        ];
        for third in 0..12u8 {
            let actual = kinds
                .get(usize::from(third))
                .copied()
                .unwrap_or(Invocation::Review);
            assert_classify_result(&[FIRST, SECOND, third], actual);
        }
        let selected = kani::any::<u8>() % 12;
        (
            kinds
                .get(usize::from(selected))
                .copied()
                .unwrap_or(Invocation::Review),
            selected,
        )
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(14)]
    fn every_bounded_invocation_matches_the_classifier_specification() {
        let kind = every_first_word();
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
        kani::cover!(!accepted & (limit > 0) & (remaining <= limit));
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
        const COUNTS: [u8; 4] = [0, 1, 2, 3];
        for cap in COUNTS {
            for mut code in 0..COUNTS.len().pow(u32::try_from(TICKS).unwrap()) {
                let records: [u8; TICKS] = core::array::from_fn(|_| {
                    let count = COUNTS[code % COUNTS.len()];
                    code /= COUNTS.len();
                    count
                });
                for ignored_pause in [false, true] {
                    proofs::spec_ledger_safe(&Ledger {
                        records,
                        ignored_pause,
                        ..ledger(cap)
                    });
                }
                for now in 0..=HORIZON {
                    let mut state = ledger(cap);
                    state.now = now;
                    state.records = records;
                    state.stage[0] = Stage::Read;
                    state.holder = Some(0);
                    for skew in [0, 1, 2] {
                        let actual = record(Guard::Fixed, state, Event::Reserve(0, skew));
                        if skew > 1 {
                            assert_eq!(actual, None);
                            continue;
                        }
                        let clock = usize::from(now.saturating_sub(skew));
                        let recent = |window: u8| -> u32 {
                            records
                                .iter()
                                .enumerate()
                                .filter_map(|(at, count)| {
                                    clock
                                        .checked_sub(at)
                                        .filter(|age| *age < usize::from(window))
                                        .map(|_| u32::from(*count))
                                })
                                .sum()
                        };
                        let future = records
                            .iter()
                            .enumerate()
                            .any(|(at, count)| at > clock && *count > 0);
                        let admitted = !future
                            && recent(LEDGER_HOUR) < u32::from(cap)
                            && recent(LEDGER_DAY)
                                < u32::from(cap) * u32::from(LEDGER_DAY / LEDGER_HOUR);
                        let mut expected = state;
                        if admitted {
                            expected.stage[0] = Stage::Reserved;
                            expected.records[clock] += 1;
                        } else {
                            expected.stage[0] = Stage::Done;
                            expected.holder = None;
                        }
                        assert_eq!(actual, Some(expected), "{state:?}, skew={skew}");
                    }
                }
            }
        }
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
        for (first_code, first) in words.iter().enumerate() {
            let first_code = u8::try_from(first_code).unwrap();
            assert_eq!(
                proofs::spec_classify_codes(&[first_code]),
                proofs::spec_classify(&[first])
            );
            for (second_code, second) in words.iter().enumerate() {
                let second_code = u8::try_from(second_code).unwrap();
                assert_eq!(
                    proofs::spec_classify_codes(&[first_code, second_code]),
                    proofs::spec_classify(&[first, second])
                );
                for (third_code, third) in words.iter().enumerate() {
                    let third_code = u8::try_from(third_code).unwrap();
                    assert_eq!(
                        proofs::spec_classify_codes(&[first_code, second_code, third_code]),
                        proofs::spec_classify(&[first, second, third])
                    );
                }
            }
        }
        assert_eq!(proofs::spec_classify_codes(&[]), proofs::spec_classify(&[]));
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
        assert_eq!(classify(&["usage"]), Invocation::ReadOnly);
        assert!(usage_query(&["usage"]));
        assert!(!usage_query(&["code"]));
        let records = [0; DAILY_LIMIT + 1];
        for now in [0, HOUR_MS - 1, HOUR_MS, DAY_MS - 1, DAY_MS, DAY_MS + 1] {
            for count in [DAILY_LIMIT - 1, DAILY_LIMIT, DAILY_LIMIT + 1] {
                proofs::spec_check_budget(&records[..count], now);
            }
        }
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
