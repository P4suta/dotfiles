//! Whether every subject of the decision core has a discharged obligation or a current exemption, whether a pilot still runs, and whether a deterministic metric holds its baseline.

/// The closed reasons an exemption may give.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// The property depends on an external system or operating system behavior that no verifier models.
    SemanticBoundary,
    /// The subject passes values through with no branch and no arithmetic.
    NoDecision,
    /// The cost of the obligation outweighs the risk.
    Disproportionate,
}

impl Reason {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::SemanticBoundary => "semantic-boundary",
            Self::NoDecision => "no-decision",
            Self::Disproportionate => "disproportionate",
        }
    }
}

/// What the gate knows of one subject.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each field is an independent fact the adapter observes in the source"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Subject {
    /// A harness or a verification helper of the subject's module mentions it.
    pub proved: bool,
    /// The exemption beside it, if any.
    pub exemption: Option<Reason>,
    /// Its body branches or computes.
    pub decides: bool,
    /// It lives in the pure core, which reaches no external system.
    pub pure: bool,
    /// The exemption's substitute evidence exists.
    pub evidence: bool,
    /// Code outside tests and proofs calls it.
    pub called: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Proved,
    Exempt,
    /// Neither a harness nor an exemption owns the subject.
    Unowned,
    /// The exemption no longer holds, or a harness already proves the subject.
    StaleExemption,
    /// No production code calls a proved subject, so the proof covers nothing that runs.
    Uncalled,
}

impl Verdict {
    #[must_use]
    pub const fn admitted(self) -> bool {
        matches!(self, Self::Proved | Self::Exempt)
    }
}

/// Whether an exemption still holds for `subject`.
const fn current(reason: Reason, subject: Subject) -> bool {
    subject.evidence
        && match reason {
            Reason::NoDecision => !subject.decides,
            Reason::SemanticBoundary => !subject.pure,
            Reason::Disproportionate => true,
        }
}

#[must_use]
pub const fn verdict(subject: Subject) -> Verdict {
    match (subject.proved, subject.exemption) {
        (true, None) if subject.called => Verdict::Proved,
        (true, None) => Verdict::Uncalled,
        (false, Some(reason)) if current(reason, subject) => Verdict::Exempt,
        (_, Some(_)) => Verdict::StaleExemption,
        (false, None) => Verdict::Unowned,
    }
}

/// Where a pilot stands, which its issue and its decision record decide.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pilot {
    /// The issue stays open, so the pilot runs advisory.
    Running,
    /// A decision record adopts, rejects, or defers the tool.
    Decided,
    /// The issue closed without a decision record.
    Undecided,
}

#[must_use]
pub const fn pilot(issue_closed: bool, decided: bool) -> Pilot {
    if decided {
        Pilot::Decided
    } else if issue_closed {
        Pilot::Undecided
    } else {
        Pilot::Running
    }
}

/// How a deterministic measurement compares with its recorded baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Standing {
    Holds,
    /// The measurement got worse than the baseline.
    Regressed,
    /// The measurement got better, so the baseline must move with it before the gate passes.
    Improved,
}

/// Which way a metric improves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Better {
    Lower,
    Higher,
}

#[must_use]
pub fn standing(better: Better, baseline: u64, measured: u64) -> Standing {
    match (measured.cmp(&baseline), better) {
        (core::cmp::Ordering::Equal, _) => Standing::Holds,
        (core::cmp::Ordering::Less, Better::Lower)
        | (core::cmp::Ordering::Greater, Better::Higher) => Standing::Improved,
        (core::cmp::Ordering::Less, Better::Higher)
        | (core::cmp::Ordering::Greater, Better::Lower) => Standing::Regressed,
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const REASONS: [Option<Reason>; 4] = [
        Some(Reason::SemanticBoundary),
        Some(Reason::NoDecision),
        Some(Reason::Disproportionate),
        None,
    ];

    /// A subject passes only with exactly one owner that still holds, and each refusal names why.
    pub fn spec_verdict(subject: Subject) {
        let result = verdict(subject);
        let holds = match subject.exemption {
            Some(Reason::NoDecision) => subject.evidence && !subject.decides,
            Some(Reason::SemanticBoundary) => subject.evidence && !subject.pure,
            Some(Reason::Disproportionate) => subject.evidence,
            None => false,
        };
        assert_eq!(
            result == Verdict::Proved,
            subject.proved && subject.exemption.is_none() && subject.called
        );
        assert_eq!(
            result == Verdict::Uncalled,
            subject.proved && subject.exemption.is_none() && !subject.called
        );
        assert_eq!(result == Verdict::Exempt, !subject.proved && holds);
        assert_eq!(
            result == Verdict::Unowned,
            !subject.proved && subject.exemption.is_none()
        );
        assert_eq!(
            result.admitted(),
            matches!(result, Verdict::Proved | Verdict::Exempt)
        );
    }

    pub fn spec_pilot(closed: bool, decided: bool) {
        let result = pilot(closed, decided);
        assert_eq!(result == Pilot::Undecided, closed && !decided);
        assert_eq!(result == Pilot::Decided, decided);
        assert_eq!(result == Pilot::Running, !closed && !decided);
    }

    pub fn spec_standing(lower: bool, baseline: u64, measured: u64) {
        let better = if lower { Better::Lower } else { Better::Higher };
        let result = standing(better, baseline, measured);
        assert_eq!(result == Standing::Holds, baseline == measured);
        assert_eq!(
            result == Standing::Regressed,
            if lower {
                measured > baseline
            } else {
                measured < baseline
            }
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_subject_passes_only_with_one_owner_that_still_holds() {
        let subject = Subject {
            proved: kani::any(),
            exemption: REASONS[usize::from(kani::any::<u8>() % 4)],
            decides: kani::any(),
            pure: kani::any(),
            evidence: kani::any(),
            called: kani::any(),
        };
        spec_verdict(subject);
        let result = verdict(subject);
        kani::cover!(result == Verdict::Exempt);
        kani::cover!(result == Verdict::StaleExemption && !subject.proved);
        kani::cover!(result == Verdict::Uncalled);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_closed_pilot_fails_until_a_record_decides_it() {
        let closed: bool = kani::any();
        let decided: bool = kani::any();
        spec_pilot(closed, decided);
        kani::cover!(pilot(closed, decided) == Pilot::Undecided);
        kani::cover!(pilot(closed, decided) == Pilot::Running);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_metric_holds_only_at_its_baseline_and_tells_the_direction() {
        let lower: bool = kani::any();
        let baseline: u64 = kani::any();
        let measured: u64 = kani::any();
        spec_standing(lower, baseline, measured);
        kani::cover!(lower && measured > baseline);
        kani::cover!(!lower && measured > baseline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specifications() {
        for exemption in proofs::REASONS {
            for [proved, decides, pure, evidence, called] in crate::domain::flags() {
                proofs::spec_verdict(Subject {
                    proved,
                    exemption,
                    decides,
                    pure,
                    evidence,
                    called,
                });
            }
        }
        for [closed, decided] in crate::domain::flags() {
            proofs::spec_pilot(closed, decided);
        }
        for lower in [false, true] {
            for baseline in [0, 1, 2, u64::MAX] {
                for measured in [0, 1, 2, u64::MAX] {
                    proofs::spec_standing(lower, baseline, measured);
                }
            }
        }
        for reason in [
            Reason::SemanticBoundary,
            Reason::NoDecision,
            Reason::Disproportionate,
        ] {
            assert_ne!(reason.key(), "");
        }
        assert_eq!(Reason::SemanticBoundary.key(), "semantic-boundary");
        assert_eq!(Reason::Disproportionate.key(), "disproportionate");
    }

    const OWNED: Subject = Subject {
        proved: true,
        exemption: None,
        decides: true,
        pure: true,
        evidence: false,
        called: true,
    };

    #[test]
    fn a_proved_subject_needs_a_caller_and_no_exemption() {
        assert_eq!(verdict(OWNED), Verdict::Proved);
        assert_eq!(
            verdict(Subject {
                called: false,
                ..OWNED
            }),
            Verdict::Uncalled
        );
        assert_eq!(
            verdict(Subject {
                exemption: Some(Reason::Disproportionate),
                evidence: true,
                ..OWNED
            }),
            Verdict::StaleExemption
        );
    }

    #[test]
    fn an_exemption_holds_only_while_its_reason_and_evidence_do() {
        let exempt = Subject {
            proved: false,
            exemption: Some(Reason::NoDecision),
            decides: false,
            evidence: true,
            ..OWNED
        };
        assert_eq!(verdict(exempt), Verdict::Exempt);
        assert_eq!(
            verdict(Subject {
                decides: true,
                ..exempt
            }),
            Verdict::StaleExemption
        );
        assert_eq!(
            verdict(Subject {
                evidence: false,
                ..exempt
            }),
            Verdict::StaleExemption
        );
        assert_eq!(
            verdict(Subject {
                exemption: Some(Reason::SemanticBoundary),
                ..exempt
            }),
            Verdict::StaleExemption
        );
        assert_eq!(
            verdict(Subject {
                exemption: None,
                ..exempt
            }),
            Verdict::Unowned
        );
    }

    #[test]
    fn a_pilot_runs_until_its_issue_closes_and_then_needs_a_decision() {
        assert_eq!(pilot(false, false), Pilot::Running);
        assert_eq!(pilot(true, false), Pilot::Undecided);
        assert_eq!(pilot(true, true), Pilot::Decided);
    }

    #[test]
    fn a_metric_refuses_both_a_regression_and_an_unrecorded_improvement() {
        assert_eq!(standing(Better::Lower, 3, 3), Standing::Holds);
        assert_eq!(standing(Better::Lower, 3, 4), Standing::Regressed);
        assert_eq!(standing(Better::Lower, 3, 2), Standing::Improved);
        assert_eq!(standing(Better::Higher, 3, 2), Standing::Regressed);
        assert_eq!(standing(Better::Higher, 3, 4), Standing::Improved);
        assert_eq!(Reason::NoDecision.key(), "no-decision");
    }
}
