#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewRoute {
    None,
    Cli,
    Pr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Consequence {
    Severe,
    Catastrophic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Likelihood {
    Hypothetical,
    Credible,
    Exposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlastRadius {
    Limited,
    Broad,
    Systemic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rollback {
    Recoverable,
    Difficult,
    Irreversible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SevereRisk {
    pub consequence: Consequence,
    pub likelihood: Likelihood,
    pub blast_radius: BlastRadius,
    pub rollback: Rollback,
}

#[must_use]
pub const fn route(level: ReviewLevel) -> ReviewRoute {
    match level {
        ReviewLevel::Low | ReviewLevel::Medium => ReviewRoute::None,
        ReviewLevel::High => ReviewRoute::Cli,
        ReviewLevel::Critical => ReviewRoute::Pr,
    }
}

#[must_use]
pub fn assessment_allowed(level: ReviewLevel, risk: Option<SevereRisk>) -> bool {
    let minimum = risk.and_then(|risk| {
        if risk.likelihood == Likelihood::Hypothetical {
            None
        } else if risk.consequence == Consequence::Catastrophic
            && risk.blast_radius != BlastRadius::Limited
            && risk.rollback != Rollback::Recoverable
        {
            Some(ReviewLevel::Critical)
        } else {
            Some(ReviewLevel::High)
        }
    });
    matches!(
        (level, risk, minimum),
        (ReviewLevel::Low | ReviewLevel::Medium, None, None)
            | (ReviewLevel::High, Some(_), Some(ReviewLevel::High))
            | (ReviewLevel::Critical, Some(_), Some(ReviewLevel::Critical))
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewEvidence {
    None,
    Cli,
    Pr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    Accepted,
    Paused,
    Missing,
    WrongRoute,
    Unresolved,
}

#[must_use]
pub fn completion(
    route: ReviewRoute,
    cli_paused: bool,
    pr_paused: bool,
    evidence: ReviewEvidence,
    supported_findings_resolved: bool,
) -> Completion {
    if !supported_findings_resolved {
        return Completion::Unresolved;
    }
    if (route == ReviewRoute::Cli && cli_paused) || (route == ReviewRoute::Pr && pr_paused) {
        return Completion::Paused;
    }
    match (route, evidence) {
        (ReviewRoute::None, ReviewEvidence::None)
        | (ReviewRoute::Cli, ReviewEvidence::Cli)
        | (ReviewRoute::Pr, ReviewEvidence::Pr) => Completion::Accepted,
        (ReviewRoute::Cli | ReviewRoute::Pr, ReviewEvidence::None) => Completion::Missing,
        _ => Completion::WrongRoute,
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_route(level: ReviewLevel) {
        let actual = route(level);
        assert_eq!(
            actual == ReviewRoute::None,
            matches!(level, ReviewLevel::Low | ReviewLevel::Medium)
        );
        assert_eq!(actual == ReviewRoute::Cli, level == ReviewLevel::High);
        assert_eq!(actual == ReviewRoute::Pr, level == ReviewLevel::Critical);
    }

    pub fn spec_assessment(level: ReviewLevel, risk: Option<SevereRisk>) {
        let credible = risk.is_some_and(|risk| risk.likelihood != Likelihood::Hypothetical);
        let catastrophic = risk.is_some_and(|risk| {
            risk.consequence == Consequence::Catastrophic
                && risk.blast_radius != BlastRadius::Limited
                && risk.rollback != Rollback::Recoverable
        });
        assert_eq!(
            assessment_allowed(level, risk),
            match level {
                ReviewLevel::Low | ReviewLevel::Medium => risk.is_none(),
                ReviewLevel::High => credible && !catastrophic,
                ReviewLevel::Critical => credible && catastrophic,
            }
        );
    }

    pub fn spec_completion(
        route: ReviewRoute,
        cli: bool,
        pr: bool,
        evidence: ReviewEvidence,
        resolved: bool,
    ) {
        let actual = completion(route, cli, pr, evidence, resolved);
        let paused =
            matches!(route, ReviewRoute::Cli) && cli || matches!(route, ReviewRoute::Pr) && pr;
        let matching = matches!(
            (route, evidence),
            (ReviewRoute::None, ReviewEvidence::None)
                | (ReviewRoute::Cli, ReviewEvidence::Cli)
                | (ReviewRoute::Pr, ReviewEvidence::Pr)
        );
        assert_eq!(
            actual == Completion::Accepted,
            resolved && !paused && matching
        );
        assert_eq!(actual == Completion::Paused, resolved && paused);
        assert_eq!(actual == Completion::Unresolved, !resolved);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_level_selects_exactly_one_review_route() {
        let level = kani::any();
        let selected = route(level);
        kani::cover!(selected == ReviewRoute::None, "no review route is selected");
        kani::cover!(
            matches!(selected, ReviewRoute::Cli | ReviewRoute::Pr),
            "one severe review route is selected"
        );
        spec_route(level);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn severe_review_levels_require_credible_concrete_consequences() {
        let level = kani::any();
        let risk = kani::any();
        let admitted = assessment_allowed(level, risk);
        kani::cover!(admitted, "rationalized severity is admitted");
        kani::cover!(!admitted, "unsupported severity is refused");
        spec_assessment(level, risk);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn completion_requires_the_selected_unpaused_route_and_resolved_findings() {
        let route = kani::any();
        let cli = kani::any();
        let pr = kani::any();
        let evidence = kani::any();
        let resolved = kani::any();
        let actual = completion(route, cli, pr, evidence, resolved);
        kani::cover!(
            actual == Completion::Accepted,
            "completed selected review is accepted"
        );
        kani::cover!(
            actual != Completion::Accepted,
            "incomplete selected review is refused"
        );
        spec_completion(route, cli, pr, evidence, resolved);
    }
}

#[cfg(kani)]
impl kani::Arbitrary for ReviewLevel {
    fn any() -> Self {
        match kani::any::<u8>() % 4 {
            0 => Self::Low,
            1 => Self::Medium,
            2 => Self::High,
            _ => Self::Critical,
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for ReviewRoute {
    fn any() -> Self {
        match kani::any::<u8>() % 3 {
            0 => Self::None,
            1 => Self::Cli,
            _ => Self::Pr,
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for ReviewEvidence {
    fn any() -> Self {
        match kani::any::<u8>() % 3 {
            0 => Self::None,
            1 => Self::Cli,
            _ => Self::Pr,
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for SevereRisk {
    fn any() -> Self {
        Self {
            consequence: if kani::any() {
                Consequence::Severe
            } else {
                Consequence::Catastrophic
            },
            likelihood: match kani::any::<u8>() % 3 {
                0 => Likelihood::Hypothetical,
                1 => Likelihood::Credible,
                _ => Likelihood::Exposed,
            },
            blast_radius: match kani::any::<u8>() % 3 {
                0 => BlastRadius::Limited,
                1 => BlastRadius::Broad,
                _ => BlastRadius::Systemic,
            },
            rollback: match kani::any::<u8>() % 3 {
                0 => Rollback::Recoverable,
                1 => Rollback::Difficult,
                _ => Rollback::Irreversible,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declared_level_has_one_exclusive_route() {
        for level in [
            ReviewLevel::Low,
            ReviewLevel::Medium,
            ReviewLevel::High,
            ReviewLevel::Critical,
        ] {
            proofs::spec_route(level);
        }
    }

    #[test]
    fn every_severity_context_obeys_the_rare_review_thresholds() {
        for level in [
            ReviewLevel::Low,
            ReviewLevel::Medium,
            ReviewLevel::High,
            ReviewLevel::Critical,
        ] {
            proofs::spec_assessment(level, None);
            for consequence in [Consequence::Severe, Consequence::Catastrophic] {
                for likelihood in [
                    Likelihood::Hypothetical,
                    Likelihood::Credible,
                    Likelihood::Exposed,
                ] {
                    for blast_radius in [
                        BlastRadius::Limited,
                        BlastRadius::Broad,
                        BlastRadius::Systemic,
                    ] {
                        for rollback in [
                            Rollback::Recoverable,
                            Rollback::Difficult,
                            Rollback::Irreversible,
                        ] {
                            proofs::spec_assessment(
                                level,
                                Some(SevereRisk {
                                    consequence,
                                    likelihood,
                                    blast_radius,
                                    rollback,
                                }),
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_route_pause_and_evidence_combination_obeys_completion() {
        for route in [ReviewRoute::None, ReviewRoute::Cli, ReviewRoute::Pr] {
            for evidence in [
                ReviewEvidence::None,
                ReviewEvidence::Cli,
                ReviewEvidence::Pr,
            ] {
                for [cli, pr, resolved] in crate::domain::flags() {
                    proofs::spec_completion(route, cli, pr, evidence, resolved);
                }
            }
        }
    }
}
