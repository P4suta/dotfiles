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
const fn with_margin(expected: u64) -> u64 {
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
pub fn timeout(observed: Option<u64>, default: u64, requested: Option<u64>, limit: u64) -> Timeout {
    let needed = match observed {
        Some(observed) => with_margin(observed),
        None => default,
    };
    let floor = needed.min(limit);
    match requested {
        Some(requested) if requested > limit => Timeout::Set(limit),
        Some(requested) if requested >= floor => Timeout::Keep(requested),
        _ => Timeout::Set(floor),
    }
}

/// The fewest recent runs that make an expectation firm enough to warn about.
pub const MIN_SAMPLES: usize = 5;

/// A run shorter than this never earns the advice to move to the background, in milliseconds.
pub const BACKGROUND_FLOOR_MS: u64 = 30_000;

/// The hook reports a long run that took well past a firm expectation, so the next one can run in the background.
/// Well past means beyond the timeout margin: half again plus ten seconds.
#[must_use]
pub const fn overran(expected: Option<u64>, samples: usize, took: u64) -> bool {
    match expected {
        Some(expected) => {
            samples >= MIN_SAMPLES && took >= BACKGROUND_FLOOR_MS && took > with_margin(expected)
        }
        None => false,
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// The timeout never exceeds the limit, never falls below the needed time within it, and keeps only a request that covers that.
    pub fn spec_timeout(observed: Option<u64>, default: u64, requested: Option<u64>, limit: u64) {
        let decided = timeout(observed, default, requested, limit);
        let needed = observed.map_or(u128::from(default), |observed| {
            u128::from(observed) + u128::from(observed / 2) + u128::from(START_UP_MS)
        });
        let floor = needed.min(u128::from(limit));
        match requested {
            Some(requested) if requested > limit => assert_eq!(decided, Timeout::Set(limit)),
            Some(requested) if u128::from(requested) >= floor => {
                assert_eq!(decided, Timeout::Keep(requested));
            }
            _ => assert!(matches!(decided, Timeout::Set(millis) if u128::from(millis) == floor)),
        }
        assert!(decided.millis() <= limit);
        assert!(u128::from(decided.millis()) >= floor);
        match decided {
            Timeout::Keep(kept) | Timeout::Set(kept) => assert_eq!(decided.millis(), kept),
        }
    }

    /// A warning needs a firm expectation, a long run, and a run past half again plus ten seconds.
    pub fn spec_overran(expected: Option<u64>, samples: usize, took: u64) {
        assert_eq!(
            overran(expected, samples, took),
            expected.is_some_and(|expected| {
                samples >= 5
                    && took >= 30_000
                    && u128::from(took)
                        > u128::from(expected) + u128::from(expected / 2) + u128::from(START_UP_MS)
            })
        );
    }

    /// The nearest-rank percentile of a sorted history picks the run at the rank `(95 * n).div_ceil(100)`.
    pub fn spec_percentile(sorted: &[u64]) {
        let rank = (sorted.len() * 95).div_ceil(100);
        let expected = if rank == 0 {
            None
        } else {
            sorted.get(rank - 1).copied()
        };
        assert_eq!(percentile(sorted), expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn the_timeout_covers_the_needed_time_within_the_limit_and_keeps_only_a_covering_request() {
        let observed: Option<u64> = kani::any();
        let default: u64 = kani::any();
        let requested: Option<u64> = kani::any();
        let limit: u64 = kani::any();
        spec_timeout(observed, default, requested, limit);
        let decided = timeout(observed, default, requested, limit);
        kani::cover!(matches!(decided, Timeout::Keep(_)));
        kani::cover!(matches!(decided, Timeout::Set(_)) & requested.is_some());
        kani::cover!((decided.millis() == limit) & observed.is_some());
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_run_overran_only_past_a_known_expectation() {
        let expected: Option<u64> = kani::any();
        let samples: usize = kani::any();
        let took: u64 = kani::any();
        spec_overran(expected, samples, took);
        let overran = overran(expected, samples, took);
        kani::cover!(overran);
        kani::cover!(!overran & expected.is_some() & (samples >= MIN_SAMPLES));
        kani::cover!(!overran & expected.is_some_and(|expected| took > expected));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn the_percentile_of_a_short_history_is_its_slowest_run() {
        let runs: [u64; 3] = kani::any();
        kani::assume(runs[0] <= runs[1] && runs[1] <= runs[2]);
        let length: usize = kani::any();
        kani::assume(length <= runs.len());
        spec_percentile(&runs[..length]);
        kani::cover!(length == 3 && runs[1] < runs[2]);
        kani::cover!(length == 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[cfg(miri)]
    #[test]
    fn examples_share_one_miri_test() {
        every_history_up_to_two_windows_meets_the_percentile_specification();
        a_short_timeout_is_raised_to_the_percentile_with_its_margin();
        the_client_limit_caps_every_timeout();
        without_history_the_default_applies();
        the_percentile_is_nearest_rank();
        only_a_run_beyond_its_expectation_overran();
    }

    const VALUES: [u64; 8] = [0, 1, 2, 9_999, 10_000, 600_000, u64::MAX - 1, u64::MAX];

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every boundary")]
    fn every_boundary_meets_the_timeout_specifications() {
        let options: Vec<Option<u64>> = core::iter::once(None).chain(VALUES.map(Some)).collect();
        for &observed in &options {
            for default in VALUES {
                for &requested in &options {
                    for limit in VALUES {
                        proofs::spec_timeout(observed, default, requested, limit);
                    }
                }
            }
            for took in VALUES.into_iter().chain([29_999, 30_000, 160_000, 160_001]) {
                for samples in [0, MIN_SAMPLES - 1, MIN_SAMPLES, WINDOW] {
                    proofs::spec_overran(observed, samples, took);
                }
            }
        }
    }

    #[cfg_attr(not(miri), test)]
    fn every_history_up_to_two_windows_meets_the_percentile_specification() {
        let runs: Vec<u64> = (0u64..).take(2 * WINDOW).collect();
        for length in 0..=runs.len() {
            proofs::spec_percentile(runs.get(..length).unwrap_or_default());
        }
        assert_eq!(percentile(&runs), Some(37));
    }

    #[cfg_attr(not(miri), test)]
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

    #[cfg_attr(not(miri), test)]
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

    #[cfg_attr(not(miri), test)]
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

    #[cfg_attr(not(miri), test)]
    fn the_percentile_is_nearest_rank() {
        let runs: Vec<u64> = (1..=20).collect();
        assert_eq!(percentile(&runs), Some(19));
        assert_eq!(percentile(&[5]), Some(5));
        assert_eq!(percentile(&[]), None);
    }

    #[cfg_attr(not(miri), test)]
    fn only_a_run_beyond_its_expectation_overran() {
        assert!(overran(Some(100_000), MIN_SAMPLES, 160_001));
        assert!(!overran(Some(100_000), MIN_SAMPLES, 160_000));
        assert!(!overran(Some(100_000), MIN_SAMPLES - 1, 600_000));
        assert!(!overran(Some(3_000), WINDOW, BACKGROUND_FLOOR_MS - 1));
        assert!(overran(Some(3_000), WINDOW, BACKGROUND_FLOOR_MS));
        assert!(!overran(None, WINDOW, 1_000_000));
    }
}
