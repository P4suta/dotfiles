//! Complete coverage of every benchmark by its measurements and ceilings.

pub const BENCHMARKS: usize = 7;

/// One instruction varied between repeated measurements of the rewritten message.
pub const INSTRUCTION_BAND: u64 = 1;

/// A measurement may improve by the recorded band and may never exceed its ceiling.
#[must_use]
pub const fn instructions_hold(ceiling: u64, measured: u64) -> bool {
    measured <= ceiling && ceiling - measured <= INSTRUCTION_BAND
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Counts {
    pub instructions: u64,
    pub heap: u64,
    pub measurements: u64,
}

#[must_use]
pub fn complete(counts: [Counts; BENCHMARKS], binaries: u64, unknown: bool) -> bool {
    !unknown
        && binaries == 1
        && counts
            .iter()
            .all(|count| count.instructions == 1 && count.heap == 1 && count.measurements == 1)
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    #[cfg(kani)]
    #[kani::proof]
    fn the_instruction_band_never_admits_a_raised_measurement() {
        let ceiling: u64 = kani::any();
        let measured: u64 = kani::any();
        let holds = instructions_hold(ceiling, measured);
        assert_eq!(
            holds,
            measured == ceiling || measured.checked_add(INSTRUCTION_BAND) == Some(ceiling)
        );
        kani::cover!(holds);
        kani::cover!(!holds);
    }

    #[test]
    fn a_regression_and_an_improvement_beyond_the_band_are_refused() {
        for ceiling in [0, 1, 2, u64::MAX - 1, u64::MAX] {
            assert!(instructions_hold(ceiling, ceiling));
            if let Some(lower) = ceiling.checked_sub(INSTRUCTION_BAND) {
                assert!(instructions_hold(ceiling, lower));
            }
            if let Some(lower) = ceiling.checked_sub(INSTRUCTION_BAND + 1) {
                assert!(!instructions_hold(ceiling, lower));
            }
            if let Some(higher) = ceiling.checked_add(1) {
                assert!(!instructions_hold(ceiling, higher));
            }
        }
    }

    fn spec(counts: [Counts; BENCHMARKS], binaries: u64, unknown: bool) {
        let mut expected = !unknown && binaries == 1;
        for count in counts {
            expected &= count.instructions == 1 && count.heap == 1 && count.measurements == 1;
        }
        assert_eq!(complete(counts, binaries, unknown), expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn each_benchmark_needs_one_measurement_and_both_ceilings() {
        let counts = core::array::from_fn(|_| Counts {
            instructions: kani::any(),
            heap: kani::any(),
            measurements: kani::any(),
        });
        let binaries = kani::any();
        let unknown = kani::any();
        spec(counts, binaries, unknown);
        kani::cover!(complete(counts, binaries, unknown));
        kani::cover!(!complete(counts, binaries, unknown));
    }

    #[cfg(test)]
    pub const ONCE: Counts = Counts {
        instructions: 1,
        heap: 1,
        measurements: 1,
    };

    #[cfg(test)]
    #[test]
    fn a_missing_duplicate_or_unknown_entry_never_covers_the_inventory() {
        let counts = [ONCE; BENCHMARKS];
        assert!(complete(counts, 1, false));
        for index in 0..BENCHMARKS {
            for value in [0, 2, u64::MAX] {
                for altered in [
                    Counts {
                        instructions: value,
                        ..ONCE
                    },
                    Counts {
                        heap: value,
                        ..ONCE
                    },
                    Counts {
                        measurements: value,
                        ..ONCE
                    },
                ] {
                    let mut counts = counts;
                    counts[index] = altered;
                    spec(counts, 1, false);
                    assert!(!complete(counts, 1, false));
                }
            }
        }
        for binaries in [0, 2, u64::MAX] {
            spec(counts, binaries, false);
            assert!(!complete(counts, binaries, false));
        }
        spec(counts, 1, true);
        assert!(!complete(counts, 1, true));
    }
}
