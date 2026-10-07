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
pub const fn standing(better: Better, baseline: u64, measured: u64) -> Standing {
    if measured == baseline {
        Standing::Holds
    } else if (measured < baseline) == matches!(better, Better::Lower) {
        Standing::Improved
    } else {
        Standing::Regressed
    }
}

#[cfg(kani)]
fn any_reason() -> Option<Reason> {
    match kani::any::<u8>() {
        0 => Some(Reason::SemanticBoundary),
        1 => Some(Reason::NoDecision),
        2 => Some(Reason::Disproportionate),
        _ => None,
    }
}

#[cfg(kani)]
fn any_subject() -> Subject {
    Subject {
        proved: kani::any(),
        exemption: any_reason(),
        decides: kani::any(),
        pure: kani::any(),
        evidence: kani::any(),
        called: kani::any(),
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_subject_passes_only_with_one_owner_that_still_holds() {
    let subject = any_subject();
    let result = verdict(subject);
    if result.admitted() {
        assert!(subject.proved != subject.exemption.is_some());
        assert!(!subject.proved || subject.called);
        assert!(subject.exemption.is_none() || subject.evidence);
    }
    if !subject.proved && subject.exemption.is_none() {
        assert!(result == Verdict::Unowned);
    }
    kani::cover!(result == Verdict::Proved);
    kani::cover!(result == Verdict::Exempt);
}

#[cfg(kani)]
#[kani::proof]
fn a_stale_exemption_never_passes() {
    let subject = any_subject();
    let result = verdict(subject);
    let stale_decision = subject.exemption == Some(Reason::NoDecision) && subject.decides;
    let stale_boundary = subject.exemption == Some(Reason::SemanticBoundary) && subject.pure;
    let lost_evidence = subject.exemption.is_some() && !subject.evidence;
    if stale_decision || stale_boundary || lost_evidence {
        assert!(!result.admitted());
    }
    kani::cover!(stale_decision && result == Verdict::StaleExemption);
    kani::cover!(result == Verdict::Uncalled);
}

#[cfg(kani)]
#[kani::proof]
fn a_closed_pilot_fails_until_a_record_decides_it() {
    let closed: bool = kani::any();
    let decided: bool = kani::any();
    let result = pilot(closed, decided);
    assert_eq!(result == Pilot::Undecided, closed && !decided);
    assert_eq!(result == Pilot::Decided, decided);
    kani::cover!(result == Pilot::Undecided);
    kani::cover!(result == Pilot::Running);
}

#[cfg(kani)]
#[kani::proof]
fn a_metric_holds_only_at_its_baseline_and_tells_the_direction() {
    let lower: bool = kani::any();
    let better = if lower { Better::Lower } else { Better::Higher };
    let baseline: u64 = kani::any();
    let measured: u64 = kani::any();
    let result = standing(better, baseline, measured);
    assert_eq!(result == Standing::Holds, baseline == measured);
    if result == Standing::Regressed {
        assert!(if lower {
            measured > baseline
        } else {
            measured < baseline
        });
    }
    kani::cover!(result == Standing::Regressed);
    kani::cover!(result == Standing::Improved);
}

#[cfg(test)]
mod tests {
    use super::*;

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
