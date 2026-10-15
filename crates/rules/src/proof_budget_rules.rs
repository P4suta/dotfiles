/// The longest one harness may run, half the verifier's hard stop, so a loaded host still finishes every proof.
pub const BUDGET_MILLIS: u64 = 30_000;

/// The `--harness-timeout` after which the verifier abandons a harness.
pub const TIMEOUT_SECONDS: u64 = 60;

/// The largest mutation worker pool and shared Cargo jobserver pool.
pub const MUTANT_JOB_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum ProofCampaign {
    Full,
    Ci,
}

#[must_use]
pub const fn mutation_required(campaign: ProofCampaign) -> bool {
    matches!(campaign, ProofCampaign::Full)
}

/// The hosted phase matrix must contain Kani, runtime checks, and metrics with models exactly once.
#[must_use]
pub const fn ci_phases_complete(counts: [u8; 3], unknown: bool) -> bool {
    let [kani, runtime, metrics_models] = counts;
    kani == 1 && runtime == 1 && metrics_models == 1 && !unknown
}

#[must_use]
pub const fn mutant_jobs(available: usize) -> usize {
    match available {
        0 => 1,
        1..=MUTANT_JOB_LIMIT => available,
        _ => MUTANT_JOB_LIMIT,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum MutationPhase {
    Passed,
    Failed,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum MutationOutcome {
    Caught,
    Missed,
    TimedOut,
    Unviable,
}

#[must_use]
pub const fn mutation_outcome(
    build: MutationPhase,
    test: Option<MutationPhase>,
) -> Option<MutationOutcome> {
    match (build, test) {
        (MutationPhase::Failed, None) => Some(MutationOutcome::Unviable),
        (MutationPhase::Passed, Some(MutationPhase::Failed)) => Some(MutationOutcome::Caught),
        (MutationPhase::Passed, Some(MutationPhase::Passed)) => Some(MutationOutcome::Missed),
        (MutationPhase::Passed, Some(MutationPhase::TimedOut)) => Some(MutationOutcome::TimedOut),
        _ => None,
    }
}

pub const MUTATION_CLEAN_EXIT: i32 = 0;
pub const MUTATION_MISSED_EXIT: i32 = 2;
pub const MUTATION_TIMEOUT_EXIT: i32 = 3;

#[must_use]
pub const fn mutation_exit(total: u64, counts: [u64; 4]) -> Option<i32> {
    let [caught, missed, timed_out, unviable] = counts;
    let Some(first) = caught.checked_add(missed) else {
        return None;
    };
    let Some(second) = first.checked_add(timed_out) else {
        return None;
    };
    let Some(sum) = second.checked_add(unviable) else {
        return None;
    };
    if total == 0 || total != sum {
        return None;
    }
    Some(if timed_out != 0 {
        MUTATION_TIMEOUT_EXIT
    } else if missed != 0 {
        MUTATION_MISSED_EXIT
    } else {
        MUTATION_CLEAN_EXIT
    })
}

/// A property Kani proves, rules out, reaches, or leaves unverified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Check {
    Proven,
    Impossible,
    Witness,
    Unverified,
}

/// Kani must prove a safety check and reach an execution for a cover.
#[must_use]
pub const fn verified(cover: bool, check: Check) -> bool {
    if cover {
        matches!(check, Check::Witness)
    } else {
        matches!(check, Check::Proven | Check::Impossible)
    }
}

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

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn every_ci_phase_inventory_requires_each_known_partition_exactly_once() {
        let counts: [u8; 3] = kani::any();
        let unknown: bool = kani::any();
        let complete = ci_phases_complete(counts, unknown);
        assert_eq!(complete, counts.iter().all(|count| *count == 1) && !unknown);
        kani::cover!(complete);
        kani::cover!(!complete);
    }

    pub fn spec_mutation_required(campaign: ProofCampaign) {
        assert_eq!(mutation_required(campaign), campaign == ProofCampaign::Full);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_the_full_campaign_requires_mutation() {
        let campaign: ProofCampaign = kani::any();
        spec_mutation_required(campaign);
        kani::cover!(mutation_required(campaign));
        kani::cover!(!mutation_required(campaign));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn mutation_pools_preserve_capacity_and_never_exceed_the_ceiling() {
        let available: usize = kani::any();
        let jobs = mutant_jobs(available);
        assert!(jobs > 0 && jobs <= MUTANT_JOB_LIMIT);
        if available == 0 {
            assert_eq!(jobs, 1);
        } else if available <= MUTANT_JOB_LIMIT {
            assert_eq!(jobs, available);
        } else {
            assert_eq!(jobs, MUTANT_JOB_LIMIT);
        }
        kani::cover!(available == 0 && jobs == 1);
        kani::cover!(available > MUTANT_JOB_LIMIT && jobs == MUTANT_JOB_LIMIT);
    }

    pub fn spec_mutation_outcome(build: MutationPhase, test: Option<MutationPhase>) {
        let outcome = mutation_outcome(build, test);
        assert_eq!(
            outcome == Some(MutationOutcome::Unviable),
            build == MutationPhase::Failed && test.is_none()
        );
        for (phase, expected) in [
            (MutationPhase::Failed, MutationOutcome::Caught),
            (MutationPhase::Passed, MutationOutcome::Missed),
            (MutationPhase::TimedOut, MutationOutcome::TimedOut),
        ] {
            assert_eq!(
                outcome == Some(expected),
                build == MutationPhase::Passed && test == Some(phase)
            );
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn only_completed_build_and_test_phases_decide_a_mutant() {
        let build: MutationPhase = kani::any();
        let test: Option<MutationPhase> = kani::any();
        spec_mutation_outcome(build, test);
        kani::cover!(mutation_outcome(build, test) == Some(MutationOutcome::Caught));
        kani::cover!(mutation_outcome(build, test).is_none());
    }

    pub fn spec_mutation_exit(total: u64, counts: [u64; 4]) {
        let [caught, missed, timed_out, unviable] = counts;
        let sum =
            u128::from(caught) + u128::from(missed) + u128::from(timed_out) + u128::from(unviable);
        let exit = mutation_exit(total, counts);
        assert_eq!(exit.is_some(), total != 0 && sum == u128::from(total));
        if let Some(exit) = exit {
            assert_eq!(
                exit,
                if timed_out != 0 {
                    MUTATION_TIMEOUT_EXIT
                } else if missed != 0 {
                    MUTATION_MISSED_EXIT
                } else {
                    MUTATION_CLEAN_EXIT
                }
            );
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn mutation_counts_cannot_overflow_or_disagree_and_timeouts_dominate_misses() {
        let total: u64 = kani::any();
        let counts: [u64; 4] = kani::any();
        spec_mutation_exit(total, counts);
        kani::cover!(mutation_exit(total, counts) == Some(MUTATION_TIMEOUT_EXIT));
        kani::cover!(mutation_exit(total, counts).is_none());
    }

    #[cfg(kani)]
    #[kani::proof]
    fn an_impossible_check_is_proved_but_an_impossible_witness_is_missing() {
        let cover: bool = kani::any();
        let check = match kani::any::<u8>() % 4 {
            0 => Check::Proven,
            1 => Check::Impossible,
            2 => Check::Witness,
            _ => Check::Unverified,
        };
        let accepted = verified(cover, check);
        assert_eq!(
            accepted,
            if cover {
                check == Check::Witness
            } else {
                check == Check::Proven || check == Check::Impossible
            }
        );
        kani::cover!(accepted);
        kani::cover!(!accepted);
    }

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
    fn ci_defers_mutation_while_the_full_campaign_preserves_it() {
        for campaign in [ProofCampaign::Full, ProofCampaign::Ci] {
            proofs::spec_mutation_required(campaign);
        }
    }

    #[test]
    fn mutation_phase_results_require_the_completed_test_or_a_failed_build() {
        for build in [
            MutationPhase::Passed,
            MutationPhase::Failed,
            MutationPhase::TimedOut,
        ] {
            for test in [
                None,
                Some(MutationPhase::Passed),
                Some(MutationPhase::Failed),
                Some(MutationPhase::TimedOut),
            ] {
                proofs::spec_mutation_outcome(build, test);
            }
        }
    }

    #[test]
    fn mutation_counts_hold_each_boundary_and_exit_precedence_without_overflow() {
        for (total, counts) in [
            (0, [0; 4]),
            (1, [1, 0, 0, 0]),
            (1, [0, 1, 0, 0]),
            (1, [0, 0, 1, 0]),
            (1, [0, 0, 0, 1]),
            (2, [0, 1, 1, 0]),
            (2, [1, 0, 0, 0]),
            (u64::MAX, [u64::MAX, 0, 0, 0]),
            (u64::MAX, [u64::MAX, 1, 0, 0]),
            (u64::MAX, [1, u64::MAX, 0, 0]),
            (u64::MAX, [1, 0, u64::MAX, 0]),
            (u64::MAX, [1, 0, 0, u64::MAX]),
        ] {
            proofs::spec_mutation_exit(total, counts);
        }
    }

    #[test]
    fn mutation_jobs_preserve_capacity_until_the_ceiling() {
        for (available, expected) in [
            (0, 1),
            (1, 1),
            (MUTANT_JOB_LIMIT - 1, MUTANT_JOB_LIMIT - 1),
            (MUTANT_JOB_LIMIT, MUTANT_JOB_LIMIT),
            (MUTANT_JOB_LIMIT + 1, MUTANT_JOB_LIMIT),
            (usize::MAX, MUTANT_JOB_LIMIT),
        ] {
            assert_eq!(mutant_jobs(available), expected);
        }
    }

    #[test]
    fn proof_results_keep_impossibility_separate_from_missing_evidence() {
        for cover in [false, true] {
            for check in [
                Check::Proven,
                Check::Impossible,
                Check::Witness,
                Check::Unverified,
            ] {
                assert_eq!(
                    verified(cover, check),
                    if cover {
                        check == Check::Witness
                    } else {
                        matches!(check, Check::Proven | Check::Impossible)
                    }
                );
            }
        }
    }

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
