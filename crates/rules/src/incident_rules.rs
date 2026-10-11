extern crate alloc;

pub const MAINTENANCE_PENDING_RULE: &str = "skill-ops.maintenance-pending";

/// A failed native operation or the checked status of existing maintenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeReport {
    Failure,
    MaintenancePending,
}

/// Whether the report observes a new failure instead of an existing incomplete obligation.
#[must_use]
pub const fn record_native(report: NativeReport) -> bool {
    matches!(report, NativeReport::Failure)
}

#[cfg(any(test, kani))]
mod native_proofs {
    use super::*;

    fn spec(report: NativeReport) -> bool {
        let recorded = record_native(report);
        assert_eq!(recorded, report != NativeReport::MaintenancePending);
        recorded
    }

    #[cfg(kani)]
    #[kani::proof]
    fn native_reports_record_failures_without_recording_pending_maintenance() {
        let report = if kani::any() {
            NativeReport::Failure
        } else {
            NativeReport::MaintenancePending
        };
        let recorded = spec(report);
        kani::cover!(recorded);
        kani::cover!(!recorded);
    }

    #[test]
    fn every_native_report_keeps_its_incident_decision() {
        for report in [NativeReport::Failure, NativeReport::MaintenancePending] {
            spec(report);
        }
    }
}

/// A Git operation that can produce a failure observation.
#[derive(Clone, Copy, Debug)]
pub struct Operation {
    pub push: bool,
    pub rewrite: bool,
    pub published: bool,
}

/// The source observed after the Git operation finishes.
#[must_use]
pub const fn signal(operation: Operation, success: bool) -> Option<Source> {
    if operation.push && !success {
        Some(Source::FailedDelivery)
    } else if success && operation.rewrite && operation.published {
        Some(Source::PublishedRewrite)
    } else {
        None
    }
}

#[cfg(any(test, kani))]
mod signal_proofs {
    use super::*;

    pub fn spec(operation: Operation, success: bool) {
        let Operation {
            push,
            rewrite,
            published,
        } = operation;
        let result = signal(operation, success);
        assert_eq!(result == Some(Source::FailedDelivery), push && !success);
        assert_eq!(
            result == Some(Source::PublishedRewrite),
            success && rewrite && published
        );
        assert_eq!(
            result.is_none(),
            (!push || success) && (!success || !rewrite || !published)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn failed_delivery_and_published_rewrites_record_their_sources() {
        let push: bool = kani::any();
        let success: bool = kani::any();
        let rewrite: bool = kani::any();
        let published: bool = kani::any();
        let operation = Operation {
            push,
            rewrite,
            published,
        };
        spec(operation, success);
        let result = signal(operation, success);
        kani::cover!(result.is_none());
        kani::cover!(result == Some(Source::FailedDelivery));
        kani::cover!(result == Some(Source::PublishedRewrite));
    }

    #[test]
    fn every_signal_tuple_is_checked() {
        for [push, success, rewrite, published] in crate::domain::flags() {
            spec(
                Operation {
                    push,
                    rewrite,
                    published,
                },
                success,
            );
        }
    }
}

/// Where an incident came from: the signal that means an operation that should have succeeded stopped or produced a wrong result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    /// A gate refused an operation, named by its rule.
    GateRefusal,
    /// A push or a required check failed.
    FailedDelivery,
    /// Published work an amendment, a revert, or a force push rewrote.
    PublishedRewrite,
    /// A receiving tool rejected a malformed input.
    MalformedInput,
    /// A verification agent reported a defect.
    VerificationDefect,
}

impl Source {
    pub const ALL: [Self; 5] = [
        Self::GateRefusal,
        Self::FailedDelivery,
        Self::PublishedRewrite,
        Self::MalformedInput,
        Self::VerificationDefect,
    ];

    /// The name an incident record stores.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::GateRefusal => "gate-refusal",
            Self::FailedDelivery => "failed-delivery",
            Self::PublishedRewrite => "published-rewrite",
            Self::MalformedInput => "malformed-input",
            Self::VerificationDefect => "verification-defect",
        }
    }

    /// The source an incident record names, or `None` for a name no source carries.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|source| source.key() == key)
    }
}

/// The facts that make a failure expected, so it records no incident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Expected {
    /// A test run failed during test-first work, before its implementation existed.
    pub red_test: bool,
    /// The failure an intentional counterexample produces, which a proof rejects on purpose.
    pub counterexample: bool,
    /// A dry run stopped before it changed anything.
    pub dry_run: bool,
}

/// Whether the recorder marks a failure expected, so it records no incident.
#[must_use]
pub fn excluded(expected: Expected) -> bool {
    expected.red_test || expected.counterexample || expected.dry_run
}

/// How far one rule's incidents have repeated, which ranks them: the most repeated rule comes first.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Rank {
    /// One incident of the rule.
    Once,
    /// The rule repeated within the window this repository counts.
    Repeated,
}

/// The rank of a rule with `repetitions` recorded incidents.
#[must_use]
pub fn rank(repetitions: u64) -> Rank {
    if repetitions > 1 {
        Rank::Repeated
    } else {
        Rank::Once
    }
}

/// The outcome a disposition records for one rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// A gate or a test mechanizes the prevention, and its evidence exists and reproduces the incident.
    Mechanized {
        evidence_present: bool,
        reproduces: bool,
    },
    /// An exemption with a closed-set reason and a reconsideration condition.
    Exempt {
        reason: Option<Reason>,
        reconsideration: bool,
    },
}

/// The closed set of exemption reasons.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// The prevention costs more than the incident's damage.
    Disproportionate,
    /// The rule would refuse exactly the behavior its caller wants.
    Intended,
}

impl Reason {
    pub const ALL: [Self; 2] = [Self::Disproportionate, Self::Intended];

    /// The name a discharge record stores.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Disproportionate => "disproportionate",
            Self::Intended => "intended",
        }
    }

    /// The reason a discharge record names, or `None` for a name no reason carries.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|reason| reason.key() == key)
    }
}

/// The words an exemption reason carries.
#[must_use]
pub const fn reason_text(reason: Reason) -> &'static str {
    match reason {
        Reason::Disproportionate => "the prevention costs more than the incident's damage",
        Reason::Intended => "the behavior the rule would refuse is the intended one",
    }
}

/// Whether a disposition completes one rule's incidents.
#[must_use]
pub fn complete(outcome: Outcome) -> bool {
    match outcome {
        Outcome::Mechanized {
            evidence_present,
            reproduces,
        } => evidence_present && reproduces,
        Outcome::Exempt {
            reason,
            reconsideration,
        } => reason.is_some() && reconsideration,
    }
}

/// The rules of one incident's rule name, which every refusal of an open incident names.
#[must_use]
pub fn next_action() -> &'static str {
    "Run `skill-ops discharge --rule <rule>` with mechanized evidence, or an exemption with a reason and a reconsideration condition."
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// An expected failure records nothing, a rank never falls as incidents repeat, and a disposition completes exactly its two outcomes.
    pub fn spec_incident(expected: Expected, repetitions: u64, outcome: Outcome) {
        assert_eq!(
            excluded(expected),
            expected.red_test || expected.counterexample || expected.dry_run
        );
        assert!(rank(repetitions) <= rank(repetitions.saturating_add(1)));
        assert_eq!(
            complete(outcome),
            match outcome {
                Outcome::Mechanized {
                    evidence_present,
                    reproduces,
                } => evidence_present && reproduces,
                Outcome::Exempt {
                    reason,
                    reconsideration,
                } => reason.is_some() && reconsideration,
            }
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn expected_failures_record_nothing_and_dispositions_complete_their_outcomes() {
        let red_test: bool = kani::any();
        let counterexample: bool = kani::any();
        let dry_run: bool = kani::any();
        let repetitions: u64 = kani::any();
        let expected = Expected {
            red_test,
            counterexample,
            dry_run,
        };
        spec_incident(
            expected,
            repetitions,
            Outcome::Mechanized {
                evidence_present: kani::any(),
                reproduces: kani::any(),
            },
        );
        spec_incident(
            expected,
            repetitions,
            Outcome::Exempt {
                reason: kani::any::<bool>().then_some(Reason::Intended),
                reconsideration: kani::any(),
            },
        );
        assert_eq!(rank(1), Rank::Once);
        assert_eq!(rank(2), Rank::Repeated);
        assert!(!next_action().is_empty());
        assert!(!reason_text(Reason::Disproportionate).is_empty());
        assert!(!reason_text(Reason::Intended).is_empty());
        kani::cover!(excluded(expected));
        kani::cover!(!excluded(expected));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri takes seconds over every incident tuple and repetition"
    )]
    fn every_input_meets_the_incident_specification() {
        for [red_test, counterexample, dry_run, evidence, reproduces] in crate::domain::flags() {
            for repetitions in [0, 1, 2, 3, u64::MAX - 1, u64::MAX] {
                for reason in [None, Some(Reason::Disproportionate), Some(Reason::Intended)] {
                    for reconsideration in [false, true] {
                        let expected = Expected {
                            red_test,
                            counterexample,
                            dry_run,
                        };
                        let outcome = Outcome::Exempt {
                            reason,
                            reconsideration,
                        };
                        super::proofs::spec_incident(expected, repetitions, outcome);
                    }
                }
                super::proofs::spec_incident(
                    Expected {
                        red_test,
                        counterexample,
                        dry_run,
                    },
                    repetitions,
                    Outcome::Mechanized {
                        evidence_present: evidence,
                        reproduces,
                    },
                );
            }
        }
    }

    #[test]
    fn expected_failures_are_excluded_and_ranked_by_repetition() {
        for expected in [
            Expected {
                red_test: true,
                counterexample: false,
                dry_run: false,
            },
            Expected {
                red_test: false,
                counterexample: true,
                dry_run: false,
            },
            Expected {
                red_test: false,
                counterexample: false,
                dry_run: true,
            },
        ] {
            assert!(excluded(expected), "{expected:?}");
        }
        assert!(!excluded(Expected {
            red_test: false,
            counterexample: false,
            dry_run: false,
        }));
        assert_eq!(rank(0), Rank::Once);
        assert_eq!(rank(1), Rank::Once);
        assert_eq!(rank(2), Rank::Repeated);
        assert_eq!(rank(u64::MAX), Rank::Repeated);
    }

    #[test]
    fn a_disposition_completes_only_with_its_evidence() {
        assert!(complete(Outcome::Mechanized {
            evidence_present: true,
            reproduces: true,
        }));
        for outcome in [
            Outcome::Mechanized {
                evidence_present: false,
                reproduces: true,
            },
            Outcome::Mechanized {
                evidence_present: true,
                reproduces: false,
            },
            Outcome::Exempt {
                reason: None,
                reconsideration: true,
            },
            Outcome::Exempt {
                reason: Some(Reason::Intended),
                reconsideration: false,
            },
        ] {
            assert!(!complete(outcome), "{outcome:?}");
        }
        assert!(complete(Outcome::Exempt {
            reason: Some(Reason::Disproportionate),
            reconsideration: true,
        }));
        assert!(next_action().starts_with("Run `skill-ops discharge"));
        assert_eq!(
            reason_text(Reason::Disproportionate),
            "the prevention costs more than the incident's damage"
        );
        assert_eq!(
            reason_text(Reason::Intended),
            "the behavior the rule would refuse is the intended one"
        );
    }

    #[test]
    fn every_source_names_a_distinct_signal() {
        let sources = [
            Source::GateRefusal,
            Source::FailedDelivery,
            Source::PublishedRewrite,
            Source::MalformedInput,
            Source::VerificationDefect,
        ];
        for (index, source) in sources.iter().enumerate() {
            assert!(!sources[..index].contains(source), "{source:?} repeats");
            assert_eq!(Source::from_key(source.key()), Some(*source));
        }
        assert_eq!(Source::from_key("no-such-source"), None);
        for reason in [Reason::Disproportionate, Reason::Intended] {
            assert_eq!(Reason::from_key(reason.key()), Some(reason));
        }
        assert_eq!(Reason::from_key("no-such-reason"), None);
    }
}
