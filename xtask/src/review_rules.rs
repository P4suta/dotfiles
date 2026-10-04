pub const HOUR_MS: u64 = 3_600_000;
pub const DAY_MS: u64 = 86_400_000;
pub const PROBE_GAP_MS: u64 = 60_000;
pub const HOURLY_LIMIT: usize = capacity_budget(10) as usize;
pub const DAILY_LIMIT: usize = HOURLY_LIMIT * 24;
pub const PROBE_DAILY_LIMIT: usize = DAILY_LIMIT;

pub const fn capacity_budget(capacity: u64) -> u64 {
    (capacity as u128 * 18 / 25) as u64
}

pub fn rolling_allowance(hourly: usize, daily: usize) -> bool {
    hourly < HOURLY_LIMIT && daily < DAILY_LIMIT
}

pub fn service_access(paused: bool, local_status: bool) -> bool {
    !paused || local_status
}

pub fn cooldown_complete(previous: u64, now: u64, gap: u64) -> bool {
    gap > 0 && now >= previous && now - previous >= gap
}

pub fn included_allowance(limit: u64, remaining: u64) -> bool {
    remaining <= limit && (u128::from(limit - remaining) + 1) * 25 <= u128::from(limit) * 18
}

#[cfg(kani)]
#[kani::proof]
fn layered_capacity_is_floored_once_without_overflow() {
    let capacity: u64 = kani::any();
    let budget = capacity_budget(capacity);
    assert_eq!(u128::from(budget), u128::from(capacity) * 18 / 25);
    assert!(budget <= capacity);
    kani::cover!(capacity > 0 && budget == 0);
    kani::cover!(budget > 0);
}

#[cfg(kani)]
#[kani::proof]
fn review_consumption_respects_the_layered_capacity_budget() {
    let limit: u64 = kani::any();
    let remaining: u64 = kani::any();
    let accepted = included_allowance(limit, remaining);
    if accepted {
        assert!(limit > 0 && remaining <= limit && remaining > 0);
        assert!((u128::from(limit - remaining) + 1) * 100 <= u128::from(limit) * 9 * 8);
    }
    assert_eq!(
        accepted,
        remaining <= limit
            && (u128::from(limit - remaining) + 1) * 100 <= u128::from(limit) * 9 * 8
    );
    kani::cover!(accepted);
    kani::cover!(!accepted && limit > 0 && remaining <= limit);
    kani::cover!(!accepted && remaining > limit);
}

#[cfg(kani)]
#[kani::proof]
fn rolling_attempts_cannot_exceed_the_layered_local_budget() {
    let hourly: usize = kani::any();
    let daily: usize = kani::any();
    let accepted = rolling_allowance(hourly, daily);
    assert_eq!(accepted, hourly < 7 && daily < 168);
    if accepted {
        assert!(hourly + 1 <= HOURLY_LIMIT);
        assert!(daily + 1 <= DAILY_LIMIT);
    }
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
    let accepted = cooldown_complete(previous, now, gap);
    assert_eq!(
        accepted,
        gap != 0 && now >= previous && now - previous >= gap
    );
    kani::cover!(accepted);
    kani::cover!(!accepted && now < previous);
    kani::cover!(!accepted && gap > 0 && now >= previous && now - previous < gap);
}

#[cfg(kani)]
#[kani::proof]
fn owner_pause_allows_only_local_guard_status() {
    let paused: bool = kani::any();
    let local_status: bool = kani::any();
    let accepted = service_access(paused, local_status);
    assert_eq!(accepted, !paused || local_status);
    assert!(!paused || !accepted || local_status);
    kani::cover!(accepted && paused && local_status);
    kani::cover!(!accepted && paused && !local_status);
}
