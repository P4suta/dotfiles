/// Whether an offered revision changes a dependency manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Need {
    Required,
    NotNeeded,
}

/// The verified facts of one offered revision's dependency check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Facts {
    pub needed: Need,
    pub evidence: bool,
    pub breaking: bool,
    pub touched: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    MissingEvidence,
    Outdated,
    Breaking,
}

#[must_use]
pub fn verdict(facts: Facts) -> Verdict {
    if facts.needed == Need::NotNeeded {
        Verdict::Admit
    } else if !facts.evidence {
        Verdict::MissingEvidence
    } else if facts.breaking {
        Verdict::Breaking
    } else if facts.touched {
        Verdict::Outdated
    } else {
        Verdict::Admit
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_verdict(facts: Facts) {
        let result = verdict(facts);
        assert_eq!(
            result == Verdict::MissingEvidence,
            facts.needed == Need::Required && !facts.evidence
        );
        assert_eq!(
            result == Verdict::Outdated,
            facts.needed == Need::Required && facts.evidence && !facts.breaking && facts.touched
        );
        assert_eq!(
            result == Verdict::Breaking,
            facts.needed == Need::Required && facts.evidence && facts.breaking
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_verified_compatible_touched_updates_block_freshness() {
        let facts: (bool, bool, bool, bool) = kani::any();
        let facts = Facts {
            needed: if facts.0 {
                Need::Required
            } else {
                Need::NotNeeded
            },
            evidence: facts.1,
            breaking: facts.2,
            touched: facts.3,
        };
        spec_verdict(facts);
        let result = verdict(facts);
        kani::cover!(result == Verdict::MissingEvidence);
        kani::cover!(result == Verdict::Outdated);
        kani::cover!(result == Verdict::Breaking);
        kani::cover!(result == Verdict::Admit);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_input_meets_the_specification() {
        for [needed, evidence, breaking, touched] in crate::domain::flags() {
            super::proofs::spec_verdict(super::Facts {
                needed: if needed {
                    super::Need::Required
                } else {
                    super::Need::NotNeeded
                },
                evidence,
                breaking,
                touched,
            });
        }
    }
}
