//! Evidence requirements for an implementation contract.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum ConfigurationSource {
    Document,
    Rust,
    Executable,
}

#[must_use]
pub const fn configuration_admitted(source: ConfigurationSource, stable_behavior: bool) -> bool {
    match source {
        ConfigurationSource::Document => true,
        ConfigurationSource::Rust => stable_behavior,
        ConfigurationSource::Executable => false,
    }
}

#[must_use]
pub const fn coverage_extension(stable_definition: bool, retains_subjects: bool) -> bool {
    stable_definition && retains_subjects
}

#[must_use]
pub const fn verification_module(owners: u64, gated: u64, uncertain: bool) -> bool {
    owners > 0 && owners == gated && !uncertain
}

#[must_use]
pub const fn unresolved_source_requires_production(
    in_package: bool,
    verification_only: bool,
    unresolved: bool,
) -> bool {
    in_package && !verification_only && unresolved
}

#[must_use]
pub const fn native_assertion_witness(
    total: u64,
    unreachable: u64,
    failed: u64,
    success: bool,
) -> bool {
    match total.checked_sub(unreachable) {
        Some(reachable) if reachable > 0 && failed <= reachable => {
            if success {
                failed == 0
            } else {
                failed > 0
            }
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Risk {
    Pure,
    Input,
    State,
    Effects,
    Concurrency,
    Unsafe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Method {
    Test,
    Proof,
    Mutation,
    Fault,
    Fuzz,
    Model,
    Memory,
    Stress,
}

impl Method {
    #[must_use]
    pub const fn bit(self) -> u16 {
        1 << self as u16
    }
}

const PURE_METHODS: u16 = 0b0000_0111;
const INPUT_METHODS: u16 = 0b0001_0101;
const STATE_METHODS: u16 = 0b0010_0101;
const EFFECTS_METHODS: u16 = 0b0000_1101;
const CONCURRENCY_METHODS: u16 = 0b1010_0101;
const UNSAFE_METHODS: u16 = 0b0100_0101;

#[must_use]
pub const fn required(risk: Risk) -> u16 {
    match risk {
        Risk::Pure => PURE_METHODS,
        Risk::Input => INPUT_METHODS,
        Risk::State => STATE_METHODS,
        Risk::Effects => EFFECTS_METHODS,
        Risk::Concurrency => CONCURRENCY_METHODS,
        Risk::Unsafe => UNSAFE_METHODS,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Stale,
    Tdd,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum TestOutcome {
    Passed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct TestHistory {
    pub expected: TestOutcome,
    pub actual: TestOutcome,
    pub completed: bool,
    pub matching_source: bool,
    pub matching_test: bool,
    pub before: u64,
    pub after: u64,
}

#[must_use]
pub const fn ordered(history: TestHistory) -> bool {
    history.completed
        && history.matching_source
        && history.matching_test
        && matches!(
            (history.expected, history.actual),
            (TestOutcome::Passed, TestOutcome::Passed) | (TestOutcome::Failed, TestOutcome::Failed)
        )
        && history.before > 0
        && history.before < history.after
}

#[must_use]
pub const fn completion(required: u16, verified: u16, current: bool, ordered: bool) -> Verdict {
    if !current {
        Verdict::Stale
    } else if !ordered {
        Verdict::Tdd
    } else if required & verified != required {
        Verdict::Missing
    } else {
        Verdict::Admit
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    #[cfg(kani)]
    #[kani::proof]
    fn unresolved_source_requires_a_local_production_context() {
        let local: bool = kani::any();
        let verification: bool = kani::any();
        let unresolved: bool = kani::any();
        assert_eq!(
            unresolved_source_requires_production(local, verification, unresolved),
            matches!((local, verification, unresolved), (true, false, true))
        );
        kani::cover!(local && !verification && unresolved);
        kani::cover!(!local && !verification && unresolved);
        kani::cover!(local && verification && unresolved);
        kani::cover!(local && !verification && !unresolved);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn module_exclusion_requires_nonempty_exclusive_verification_ownership() {
        let owners: u64 = kani::any();
        let gated: u64 = kani::any();
        let uncertain: bool = kani::any();
        let missing = i128::from(owners) - i128::from(gated);
        assert_eq!(
            verification_module(owners, gated, uncertain),
            owners != 0 && missing == 0 && !uncertain
        );
        kani::cover!(verification_module(owners, gated, uncertain));
        kani::cover!(!verification_module(owners, gated, uncertain));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn native_witness_requires_reachable_checks_and_consistent_failure_counts() {
        let total: u64 = kani::any();
        let unreachable: u64 = kani::any();
        let failed: u64 = kani::any();
        let success: bool = kani::any();
        let reachable = i128::from(total) - i128::from(unreachable);
        assert_eq!(
            native_assertion_witness(total, unreachable, failed, success),
            reachable > 0 && i128::from(failed) <= reachable && ((failed == 0) == success)
        );
        kani::cover!(native_assertion_witness(
            total,
            unreachable,
            failed,
            success
        ));
        kani::cover!(!native_assertion_witness(
            total,
            unreachable,
            failed,
            success
        ));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn configuration_admission_never_changes_executable_tokens() {
        let source: ConfigurationSource = kani::any();
        let stable: bool = kani::any();
        assert_eq!(
            configuration_admitted(source, stable),
            matches!(source, ConfigurationSource::Document)
                || (source == ConfigurationSource::Rust && stable)
        );
        kani::cover!(configuration_admitted(source, stable));
        kani::cover!(!configuration_admitted(source, stable));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn coverage_extension_preserves_the_definition_and_old_subjects() {
        let stable: bool = kani::any();
        let retains: bool = kani::any();
        assert_eq!(
            coverage_extension(stable, retains),
            [stable, retains] == [true, true]
        );
        kani::cover!(coverage_extension(stable, retains));
        kani::cover!(!coverage_extension(stable, retains));
    }

    pub fn spec_history(history: TestHistory) {
        let evidence = history.completed && history.matching_source && history.matching_test;
        let phase = match history.expected {
            TestOutcome::Passed => matches!(history.actual, TestOutcome::Passed),
            TestOutcome::Failed => matches!(history.actual, TestOutcome::Failed),
        };
        let interval = history.before != 0 && history.after > history.before;
        assert_eq!(ordered(history), evidence && phase && interval);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_an_earlier_matching_completed_test_establishes_the_required_phase() {
        let history: TestHistory = kani::any();
        spec_history(history);
        kani::cover!(ordered(history));
        kani::cover!(!ordered(history));
    }

    pub fn spec_method(method: Method) {
        let expected = match method {
            Method::Test => 0b0000_0001,
            Method::Proof => 0b0000_0010,
            Method::Mutation => 0b0000_0100,
            Method::Fault => 0b0000_1000,
            Method::Fuzz => 0b0001_0000,
            Method::Model => 0b0010_0000,
            Method::Memory => 0b0100_0000,
            Method::Stress => 0b1000_0000,
        };
        assert_eq!(method.bit(), expected);
    }

    pub fn spec_completion(required: u16, verified: u16, current: bool, ordered: bool) {
        assert_eq!(
            completion(required, verified, current, ordered) == Verdict::Admit,
            current && ordered && required & verified == required
        );
    }

    pub fn spec_required(risk: Risk) {
        let methods = required(risk);
        assert_eq!(methods & Method::Test.bit(), Method::Test.bit());
        assert_eq!(methods & Method::Mutation.bit(), Method::Mutation.bit());
        let additional = match risk {
            Risk::Pure => Method::Proof.bit(),
            Risk::Input => Method::Fuzz.bit(),
            Risk::State => Method::Model.bit(),
            Risk::Effects => Method::Fault.bit(),
            Risk::Concurrency => Method::Model.bit() | Method::Stress.bit(),
            Risk::Unsafe => Method::Memory.bit(),
        };
        assert_eq!(
            methods,
            Method::Test.bit() | Method::Mutation.bit() | additional
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn method_flags_are_distinct_and_nonzero() {
        let method: Method = kani::any();
        spec_method(method);
        kani::cover!(method == Method::Test);
        kani::cover!(method == Method::Stress);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn admission_requires_every_current_obligation_and_the_observed_order() {
        let required: u16 = kani::any();
        let verified: u16 = kani::any();
        let current: bool = kani::any();
        let ordered: bool = kani::any();
        spec_completion(required, verified, current, ordered);
        let result = completion(required, verified, current, ordered);
        kani::cover!(result == Verdict::Admit);
        kani::cover!(result == Verdict::Missing);
        kani::cover!(result == Verdict::Stale);
        kani::cover!(result == Verdict::Tdd);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn each_risk_requires_tests_mutation_and_its_specific_evidence() {
        let risk: Risk = kani::any();
        spec_required(risk);
        kani::cover!(risk == Risk::Pure);
        kani::cover!(risk == Risk::Concurrency);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_sources_cover_every_scope_and_resolution_combination() {
        for local in [false, true] {
            for verification in [false, true] {
                for unresolved in [false, true] {
                    assert_eq!(
                        unresolved_source_requires_production(local, verification, unresolved),
                        matches!((local, verification, unresolved), (true, false, true))
                    );
                }
            }
        }
    }

    #[test]
    fn configuration_coverage_and_native_witnesses_cover_every_decision() {
        for owners in [0, 1, 2, u64::MAX] {
            for gated in [0, 1, 2, u64::MAX] {
                for uncertain in [false, true] {
                    assert_eq!(
                        verification_module(owners, gated, uncertain),
                        owners != 0 && i128::from(owners) - i128::from(gated) == 0 && !uncertain
                    );
                }
            }
        }
        for stable in [false, true] {
            assert!(configuration_admitted(
                ConfigurationSource::Document,
                stable
            ));
            assert_eq!(
                configuration_admitted(ConfigurationSource::Rust, stable),
                stable
            );
            assert!(!configuration_admitted(
                ConfigurationSource::Executable,
                stable
            ));
            for retains in [false, true] {
                assert_eq!(
                    coverage_extension(stable, retains),
                    (stable, retains) == (true, true)
                );
            }
        }
        for total in [0, 1, 2, u64::MAX - 1, u64::MAX] {
            for unreachable in [0, 1, 2, u64::MAX - 1, u64::MAX] {
                for failed in [0, 1, 2, u64::MAX - 1, u64::MAX] {
                    for success in [false, true] {
                        let reachable = i128::from(total) - i128::from(unreachable);
                        assert_eq!(
                            native_assertion_witness(total, unreachable, failed, success),
                            reachable > 0
                                && i128::from(failed) <= reachable
                                && ((failed == 0) == success)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn chronology_covers_both_test_phases_and_sequence_boundaries() {
        for expected in [TestOutcome::Passed, TestOutcome::Failed] {
            for actual in [TestOutcome::Passed, TestOutcome::Failed] {
                for completed in [false, true] {
                    for matching_source in [false, true] {
                        for matching_test in [false, true] {
                            for before in [0, 1, 2, u64::MAX - 1, u64::MAX] {
                                for after in [0, 1, 2, u64::MAX - 1, u64::MAX] {
                                    proofs::spec_history(TestHistory {
                                        expected,
                                        actual,
                                        completed,
                                        matching_source,
                                        matching_test,
                                        before,
                                        after,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn each_missing_method_and_invalid_identity_refuses_completion() {
        for required in 0..=u8::MAX {
            for verified in 0..=u8::MAX {
                for current in [false, true] {
                    for ordered in [false, true] {
                        proofs::spec_completion(required.into(), verified.into(), current, ordered);
                    }
                }
            }
        }
        proofs::spec_completion(u16::MAX, u16::MAX, true, true);
        proofs::spec_completion(u16::MAX, u16::MAX - 1, true, true);
    }

    #[test]
    fn risk_requirements_cover_every_declared_failure_class() {
        for method in [
            Method::Test,
            Method::Proof,
            Method::Mutation,
            Method::Fault,
            Method::Fuzz,
            Method::Model,
            Method::Memory,
            Method::Stress,
        ] {
            proofs::spec_method(method);
        }
        for risk in [
            Risk::Pure,
            Risk::Input,
            Risk::State,
            Risk::Effects,
            Risk::Concurrency,
            Risk::Unsafe,
        ] {
            proofs::spec_required(risk);
        }
    }
}
