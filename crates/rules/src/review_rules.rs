pub const HOUR_MS: u64 = 3_600_000;
pub const DAY_MS: u64 = 86_400_000;
pub const PROBE_GAP_MS: u64 = 60_000;
#[expect(
    clippy::cast_possible_truncation,
    reason = "the budget of ten reviews is seven, which every pointer width holds"
)]
pub const HOURLY_LIMIT: usize = capacity_budget(10) as usize;
pub const DAILY_LIMIT: usize = HOURLY_LIMIT * 24;
pub const PROBE_DAILY_LIMIT: usize = DAILY_LIMIT;

/// The share of included capacity a budget may use: 72 percent, the 90 percent service margin times the 80 percent reserve.
pub const BUDGET_NUMERATOR: u128 = 18;
pub const BUDGET_DENOMINATOR: u128 = 25;

/// 72 percent of `capacity`, rounded down.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "72 percent of a u64 is at most the u64 itself, so the narrowing keeps every value"
)]
pub const fn capacity_budget(capacity: u64) -> u64 {
    (capacity as u128 * BUDGET_NUMERATOR / BUDGET_DENOMINATOR) as u64
}

#[must_use]
pub fn rolling_allowance(hourly: usize, daily: usize) -> bool {
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

#[must_use]
pub fn included_allowance(limit: u64, remaining: u64) -> bool {
    remaining <= limit
        && (u128::from(limit - remaining) + 1) * BUDGET_DENOMINATOR
            <= u128::from(limit) * BUDGET_NUMERATOR
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// A count fits the budget exactly when 25 times it stays within 18 times the capacity, so the budget floors once.
    pub fn spec_capacity_budget(capacity: u64, count: u64) -> u64 {
        let budget = capacity_budget(capacity);
        assert!(budget <= capacity);
        assert_eq!(
            count <= budget,
            u128::from(count) * BUDGET_DENOMINATOR <= u128::from(capacity) * BUDGET_NUMERATOR
        );
        budget
    }

    /// A review fits the included allowance when the consumed count after it stays within the layered budget of the limit.
    pub fn spec_included_allowance(limit: u64, remaining: u64) -> bool {
        let accepted = included_allowance(limit, remaining);
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

    pub fn spec_rolling_allowance(hourly: usize, daily: usize) {
        assert_eq!(
            rolling_allowance(hourly, daily),
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

    #[cfg(kani)]
    #[kani::proof]
    fn a_count_fits_the_layered_budget_exactly_when_25_times_it_stays_within_18_times_the_capacity()
    {
        let capacity: u64 = kani::any();
        let count: u64 = kani::any();
        let budget = spec_capacity_budget(capacity, count);
        kani::cover!(capacity > 0 && budget == 0);
        kani::cover!(count == budget && count > 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn review_consumption_respects_the_layered_capacity_budget() {
        let limit: u64 = kani::any();
        let remaining: u64 = kani::any();
        let accepted = spec_included_allowance(limit, remaining);
        kani::cover!(accepted);
        kani::cover!(!accepted && limit > 0 && remaining <= limit);
        kani::cover!(!accepted && remaining > limit);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn rolling_attempts_cannot_exceed_the_layered_local_budget() {
        let hourly: usize = kani::any();
        let daily: usize = kani::any();
        spec_rolling_allowance(hourly, daily);
        let accepted = rolling_allowance(hourly, daily);
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
                proofs::spec_capacity_budget(first, second);
                proofs::spec_included_allowance(first, second);
                for third in VALUES {
                    proofs::spec_cooldown_complete(first, second, third);
                }
            }
        }
        for capacity in 0..=100 {
            for count in 0..=100 {
                proofs::spec_capacity_budget(capacity, count);
                proofs::spec_included_allowance(capacity, count);
            }
        }
    }

    #[test]
    fn every_window_count_and_pause_meets_its_specification() {
        for hourly in [0, 1, HOURLY_LIMIT - 1, HOURLY_LIMIT, usize::MAX] {
            for daily in [0, 1, DAILY_LIMIT - 1, DAILY_LIMIT, usize::MAX] {
                proofs::spec_rolling_allowance(hourly, daily);
            }
        }
        for markers in crate::domain::flags() {
            proofs::spec_pause(markers);
        }
    }

    #[test]
    fn ten_reviews_an_hour_leave_seven_and_a_day_of_them_168() {
        assert_eq!(capacity_budget(10), 7);
        assert_eq!(HOURLY_LIMIT, 7);
        assert_eq!(DAILY_LIMIT, 168);
        assert_eq!(PROBE_DAILY_LIMIT, DAILY_LIMIT);
        assert_eq!(capacity_budget(49), 35);
        assert_eq!(capacity_budget(u64::MAX), 13_281_655_733_070_877_162);
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
