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

#[cfg(kani)]
fn any_duration() -> Option<u64> {
    if kani::any() { Some(kani::any()) } else { None }
}

#[cfg(kani)]
#[kani::proof]
fn only_a_measured_harness_inside_the_budget_passes() {
    let duration = any_duration();
    let result = verdict(duration);
    assert_eq!(
        result == Verdict::Within,
        matches!(duration, Some(millis) if millis <= BUDGET_MILLIS)
    );
    kani::cover!(result == Verdict::Within);
    kani::cover!(result != Verdict::Within);
}

#[cfg(kani)]
#[kani::proof]
fn an_overrun_and_a_missing_duration_are_told_apart_below_the_timeout() {
    let duration = any_duration();
    let result = verdict(duration);
    assert_eq!(result == Verdict::Unmeasured, duration.is_none());
    assert!(result != Verdict::Over || duration.is_some_and(|millis| millis > BUDGET_MILLIS));
    assert!(BUDGET_MILLIS * 2 <= TIMEOUT_SECONDS * 1_000);
    kani::cover!(result == Verdict::Over);
    kani::cover!(result == Verdict::Unmeasured);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_boundary_passes_and_one_millisecond_more_fails() {
        assert_eq!(verdict(Some(BUDGET_MILLIS)), Verdict::Within);
        assert_eq!(verdict(Some(BUDGET_MILLIS + 1)), Verdict::Over);
        assert_eq!(verdict(None), Verdict::Unmeasured);
    }
}
