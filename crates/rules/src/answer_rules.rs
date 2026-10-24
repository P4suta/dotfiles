/// A completed proof and an incomplete attempt carry different evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Proof {
    NotRequired,
    Verified,
    Failed,
    Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Preference {
    Higher,
    Lower,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ranking {
    First,
    Second,
    Equal,
}

#[must_use]
pub const fn rank(first: i64, second: i64, preference: Preference) -> Ranking {
    if first == second {
        Ranking::Equal
    } else if matches!(preference, Preference::Higher) == (first > second) {
        Ranking::First
    } else {
        Ranking::Second
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Binding {
    Current,
    OtherRequest,
    OtherDraft,
    OtherProducer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Check {
    Complete,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum ProposalRegion {
    Prose,
    Plan,
    AfterPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum PlanDelimiter {
    Text,
    Open,
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    UnexpectedOpen,
    UnexpectedClose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProposalStep {
    pub region: ProposalRegion,
    pub in_plan: bool,
}

/// Advances proposal scope for one line outside fenced code.
///
/// # Errors
///
/// Refuses a nested, repeated, or unmatched plan delimiter.
pub const fn proposal_step(
    region: ProposalRegion,
    delimiter: PlanDelimiter,
) -> Result<ProposalStep, PlanError> {
    let next = match (region, delimiter) {
        (ProposalRegion::Prose, PlanDelimiter::Open) => ProposalRegion::Plan,
        (ProposalRegion::Plan, PlanDelimiter::Close) => ProposalRegion::AfterPlan,
        (_, PlanDelimiter::Text) => region,
        (_, PlanDelimiter::Open) => return Err(PlanError::UnexpectedOpen),
        (_, PlanDelimiter::Close) => return Err(PlanError::UnexpectedClose),
    };
    Ok(ProposalStep {
        region: next,
        in_plan: matches!(region, ProposalRegion::Plan) || matches!(next, ProposalRegion::Plan),
    })
}

#[must_use]
pub const fn proposal_finished(region: ProposalRegion) -> bool {
    !matches!(region, ProposalRegion::Plan)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Evidence {
    pub binding: Binding,
    pub coverage: Check,
    pub sources: Check,
    pub proof_required: bool,
    pub proof: Proof,
}

#[must_use]
pub const fn binding(request: bool, draft: bool, producer: bool) -> Binding {
    if !request {
        Binding::OtherRequest
    } else if !draft {
        Binding::OtherDraft
    } else if !producer {
        Binding::OtherProducer
    } else {
        Binding::Current
    }
}

#[must_use]
pub const fn may_complete(evidence: Evidence) -> bool {
    matches!(evidence.binding, Binding::Current)
        && matches!(evidence.coverage, Check::Complete)
        && matches!(evidence.sources, Check::Complete)
        && matches!(
            (evidence.proof_required, evidence.proof),
            (true, Proof::Verified) | (false, Proof::NotRequired)
        )
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    fn spec_proposal(region: ProposalRegion, delimiter: PlanDelimiter) {
        let step = proposal_step(region, delimiter);
        assert_eq!(
            step.is_ok(),
            matches!(delimiter, PlanDelimiter::Text)
                || matches!(
                    (region, delimiter),
                    (ProposalRegion::Prose, PlanDelimiter::Open)
                        | (ProposalRegion::Plan, PlanDelimiter::Close)
                )
        );
        if let Ok(step) = step {
            assert_eq!(
                step.in_plan,
                matches!(region, ProposalRegion::Plan) || matches!(delimiter, PlanDelimiter::Open)
            );
            assert_eq!(
                proposal_finished(step.region),
                !matches!(step.region, ProposalRegion::Plan)
            );
            match delimiter {
                PlanDelimiter::Text => assert_eq!(step.region, region),
                PlanDelimiter::Open => assert_eq!(step.region, ProposalRegion::Plan),
                PlanDelimiter::Close => assert_eq!(step.region, ProposalRegion::AfterPlan),
            }
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn proposal_scope_requires_balanced_nonrepeating_delimiters() {
        let region = kani::any();
        let delimiter = kani::any();
        spec_proposal(region, delimiter);
        kani::cover!(proposal_step(region, delimiter).is_ok());
        kani::cover!(proposal_step(region, delimiter).is_err());
        assert!(!proposal_finished(ProposalRegion::Plan));
        assert!(proposal_finished(ProposalRegion::Prose));
        assert!(proposal_finished(ProposalRegion::AfterPlan));
    }

    #[test]
    fn every_proposal_region_and_delimiter_preserves_scope() {
        for region in [
            ProposalRegion::Prose,
            ProposalRegion::Plan,
            ProposalRegion::AfterPlan,
        ] {
            for delimiter in [
                PlanDelimiter::Text,
                PlanDelimiter::Open,
                PlanDelimiter::Close,
            ] {
                spec_proposal(region, delimiter);
            }
        }
    }

    fn spec_rank(first: i64, second: i64, preference: Preference) {
        let result = rank(first, second, preference);
        assert_eq!(result == Ranking::Equal, first == second);
        assert_eq!(
            result == Ranking::First,
            match preference {
                Preference::Higher => first > second,
                Preference::Lower => first < second,
            }
        );
        assert_eq!(
            result == Ranking::Second,
            match preference {
                Preference::Higher => first < second,
                Preference::Lower => first > second,
            }
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn ranking_preserves_direction_and_equality_without_arithmetic_overflow() {
        let first: i64 = kani::any();
        let second: i64 = kani::any();
        spec_rank(first, second, kani::any());
        kani::cover!(first == second);
        kani::cover!(first != second);
    }

    #[test]
    fn rankings_cover_boundaries_and_both_directions() {
        for first in [i64::MIN, -1, 0, 1, i64::MAX] {
            for second in [i64::MIN, -1, 0, 1, i64::MAX] {
                for preference in [Preference::Higher, Preference::Lower] {
                    spec_rank(first, second, preference);
                }
            }
        }
    }

    fn spec(evidence: Evidence) {
        let accepted = may_complete(evidence);
        assert_eq!(
            accepted,
            [
                evidence.binding == Binding::Current,
                evidence.coverage == Check::Complete,
                evidence.sources == Check::Complete,
            ]
            .into_iter()
            .all(core::convert::identity)
                && if evidence.proof_required {
                    evidence.proof == Proof::Verified
                } else {
                    evidence.proof == Proof::NotRequired
                }
        );
        if accepted && evidence.proof_required {
            assert_eq!(evidence.proof, Proof::Verified);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn only_current_complete_evidence_can_complete_an_answer() {
        let evidence: Evidence = kani::any();
        let request: bool = kani::any();
        let draft: bool = kani::any();
        let producer: bool = kani::any();
        assert_eq!(
            binding(request, draft, producer) == Binding::Current,
            request && draft && producer
        );
        spec(evidence);
        kani::cover!(may_complete(evidence));
        kani::cover!(!may_complete(evidence));
    }

    #[test]
    fn every_evidence_state_distinguishes_missing_and_completed_proofs() {
        for bits in 0_u8..64 {
            for proof in [
                Proof::NotRequired,
                Proof::Verified,
                Proof::Failed,
                Proof::Inconclusive,
            ] {
                spec(Evidence {
                    binding: binding(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0),
                    coverage: if bits & 8 != 0 {
                        Check::Complete
                    } else {
                        Check::Missing
                    },
                    sources: if bits & 16 != 0 {
                        Check::Complete
                    } else {
                        Check::Missing
                    },
                    proof_required: bits & 32 != 0,
                    proof,
                });
            }
        }
    }
}
