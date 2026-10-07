/// The longest one harness may run, half the verifier's hard stop, so a loaded host still finishes every proof.
pub const BUDGET_MILLIS: u64 = 30_000;

/// The `--harness-timeout` after which the verifier abandons a harness.
pub const TIMEOUT_SECONDS: u64 = 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Within,
    Over,
    /// The verifier reported no duration, so nothing shows the budget holds.
    Unmeasured,
}

#[must_use]
pub const fn verdict(duration_millis: Option<u64>) -> Verdict {
    match duration_millis {
        Some(millis) if millis <= BUDGET_MILLIS => Verdict::Within,
        Some(_) => Verdict::Over,
        None => Verdict::Unmeasured,
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// Only a measured harness inside the budget passes, and an overrun and a missing duration stay apart below the timeout.
    pub fn spec_verdict(duration: Option<u64>) {
        let result = verdict(duration);
        assert_eq!(
            result == Verdict::Within,
            duration.is_some_and(|millis| millis <= BUDGET_MILLIS)
        );
        assert_eq!(result == Verdict::Unmeasured, duration.is_none());
        assert_eq!(
            result == Verdict::Over,
            duration.is_some_and(|millis| millis > BUDGET_MILLIS)
        );
        const { assert!(BUDGET_MILLIS * 2 <= TIMEOUT_SECONDS * 1_000) };
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_measured_harness_inside_the_budget_passes() {
        let duration: Option<u64> = if kani::any() { Some(kani::any()) } else { None };
        spec_verdict(duration);
        kani::cover!(verdict(duration) == Verdict::Within);
        kani::cover!(verdict(duration) == Verdict::Over);
        kani::cover!(verdict(duration) == Verdict::Unmeasured);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_boundary_meets_the_specification() {
        proofs::spec_verdict(None);
        for millis in [
            0,
            1,
            BUDGET_MILLIS - 1,
            BUDGET_MILLIS,
            BUDGET_MILLIS + 1,
            u64::MAX,
        ] {
            proofs::spec_verdict(Some(millis));
        }
        assert_eq!((BUDGET_MILLIS, TIMEOUT_SECONDS), (30_000, 60));
    }

    #[test]
    fn the_budget_boundary_passes_and_one_millisecond_more_fails() {
        assert_eq!(verdict(Some(BUDGET_MILLIS)), Verdict::Within);
        assert_eq!(verdict(Some(BUDGET_MILLIS + 1)), Verdict::Over);
        assert_eq!(verdict(None), Verdict::Unmeasured);
    }
}
