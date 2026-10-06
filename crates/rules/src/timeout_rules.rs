/// The largest timeout Claude Code accepts for a shell command, in milliseconds.
pub const CLIENT_LIMIT_MS: u64 = 600_000;

/// How many of the newest runs of a signature decide its expectation.
pub const WINDOW: usize = 20;

/// The percentile of the recent runs that sets how long the next run should take.
const PERCENTILE: usize = 95;
const PERCENT: usize = 100;

/// A run gets its expectation again divided by this, so half again.
const MARGIN_DIVISOR: u64 = 2;

/// The start-up noise a run gets beyond its expectation, in milliseconds.
const START_UP_MS: u64 = 10_000;

/// The nearest-rank ninety-fifth percentile of `sorted`, ascending.
/// With fewer than 20 runs it equals the slowest one.
#[must_use]
pub fn percentile(sorted: &[u64]) -> Option<u64> {
    let rank = (sorted.len() * PERCENTILE).div_ceil(PERCENT);
    rank.checked_sub(1)
        .and_then(|index| sorted.get(index))
        .copied()
}

/// The time a run gets beyond its expectation: half again, plus ten seconds for start-up noise.
#[must_use]
pub const fn with_margin(expected: u64) -> u64 {
    expected
        .saturating_add(expected / MARGIN_DIVISOR)
        .saturating_add(START_UP_MS)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Timeout {
    /// The call's own timeout already covers the expectation.
    Keep(u64),
    /// The hook replaces the call's timeout.
    Set(u64),
}

impl Timeout {
    #[must_use]
    pub const fn millis(self) -> u64 {
        match self {
            Self::Keep(millis) | Self::Set(millis) => millis,
        }
    }
}

/// The timeout for one call: the observed high percentile with its margin, or the profile default without history, never over the client limit.
/// A requested timeout stays when it covers that floor within the limit, and one beyond the limit becomes the limit.
#[must_use]
pub const fn timeout(
    observed: Option<u64>,
    default: u64,
    requested: Option<u64>,
    limit: u64,
) -> Timeout {
    let needed = match observed {
        Some(observed) => with_margin(observed),
        None => default,
    };
    let floor = if needed < limit { needed } else { limit };
    match requested {
        Some(requested) if requested > limit => Timeout::Set(limit),
        Some(requested) if requested >= floor => Timeout::Keep(requested),
        _ => Timeout::Set(floor),
    }
}

/// The hook reports a run that took longer than its expectation, so the next one can run in the background.
#[must_use]
pub const fn overran(expected: Option<u64>, took: u64) -> bool {
    match expected {
        Some(expected) => took > expected,
        None => false,
    }
}

#[cfg(kani)]
#[kani::proof]
fn the_timeout_never_falls_below_the_observed_percentile_nor_exceeds_the_limit() {
    let observed: Option<u64> = kani::any();
    let default: u64 = kani::any();
    let requested: Option<u64> = kani::any();
    let limit: u64 = kani::any();
    let decided = timeout(observed, default, requested, limit);
    assert!(decided.millis() <= limit);
    if let Some(observed) = observed {
        assert!(decided.millis() >= if observed < limit { observed } else { limit });
    }
    assert_eq!(decided, timeout(observed, default, requested, limit));
    if let Timeout::Keep(kept) = decided {
        assert_eq!(requested, Some(kept));
    }
    kani::cover!(matches!(decided, Timeout::Keep(_)));
    kani::cover!(matches!(decided, Timeout::Set(_)) && requested.is_some());
    kani::cover!(decided.millis() == limit && observed.is_some());
}

#[cfg(kani)]
#[kani::proof]
fn without_history_the_profile_default_decides() {
    let observed: Option<u64> = kani::any();
    let default: u64 = kani::any();
    let limit: u64 = kani::any();
    let decided = timeout(observed, default, None, limit);
    if observed.is_none() {
        assert_eq!(
            decided,
            Timeout::Set(if default < limit { default } else { limit })
        );
    }
    kani::cover!(observed.is_none() && default < limit);
    kani::cover!(observed.is_none() && default > limit);
}

#[cfg(kani)]
#[kani::proof]
fn the_percentile_of_a_short_history_is_its_slowest_run() {
    let mut runs: [u64; 3] = kani::any();
    kani::assume(runs[0] <= runs[1] && runs[1] <= runs[2]);
    assert_eq!(percentile(&runs), Some(runs[2]));
    assert_eq!(percentile(&runs[..0]), None);
    kani::cover!(runs[1] < runs[2]);
    kani::cover!(runs[0] == runs[2]);
    runs[2] = runs[1];
    assert_eq!(percentile(&runs), Some(runs[1]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn a_short_timeout_is_raised_to_the_percentile_with_its_margin() {
        assert_eq!(
            timeout(Some(100_000), 120_000, Some(60_000), CLIENT_LIMIT_MS),
            Timeout::Set(160_000)
        );
        assert_eq!(
            timeout(Some(100_000), 120_000, None, CLIENT_LIMIT_MS),
            Timeout::Set(160_000)
        );
        assert_eq!(
            timeout(Some(100_000), 120_000, Some(300_000), CLIENT_LIMIT_MS),
            Timeout::Keep(300_000)
        );
    }

    #[test]
    fn the_client_limit_caps_every_timeout() {
        assert_eq!(
            timeout(Some(590_000), 120_000, None, CLIENT_LIMIT_MS),
            Timeout::Set(CLIENT_LIMIT_MS)
        );
        assert_eq!(
            timeout(None, 900_000, Some(700_000), CLIENT_LIMIT_MS),
            Timeout::Set(CLIENT_LIMIT_MS)
        );
    }

    #[test]
    fn without_history_the_default_applies() {
        assert_eq!(
            timeout(None, 300_000, None, CLIENT_LIMIT_MS),
            Timeout::Set(300_000)
        );
        assert_eq!(
            timeout(None, 300_000, Some(400_000), CLIENT_LIMIT_MS),
            Timeout::Keep(400_000)
        );
    }

    #[test]
    fn the_percentile_is_nearest_rank() {
        let runs: Vec<u64> = (1..=20).collect();
        assert_eq!(percentile(&runs), Some(19));
        assert_eq!(percentile(&[5]), Some(5));
        assert_eq!(percentile(&[]), None);
    }

    #[test]
    fn only_a_run_beyond_its_expectation_overran() {
        assert!(overran(Some(10), 11));
        assert!(!overran(Some(10), 10));
        assert!(!overran(None, 1_000_000));
    }
}
