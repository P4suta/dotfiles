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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProblemSeverity {
    Notice,
    Error,
    Invalid,
}

#[must_use]
pub const fn problem_severity(level: u64) -> ProblemSeverity {
    const TRACE: u64 = 10;
    const DEBUG: u64 = 20;
    const INFO: u64 = 30;
    const WARN: u64 = 40;
    const ERROR: u64 = 50;
    const FATAL: u64 = 60;
    match level {
        TRACE | DEBUG | INFO | WARN => ProblemSeverity::Notice,
        ERROR | FATAL => ProblemSeverity::Error,
        _ => ProblemSeverity::Invalid,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupSkip {
    None,
    Excluded,
    InvalidValue,
    UnspecifiedVersion,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionSelection {
    Absent,
    Present,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LookupFacts {
    pub skip: LookupSkip,
    pub selection: VersionSelection,
    pub updates: bool,
    pub warnings: bool,
    pub covered: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupEvidence {
    Verified,
    NotApplicable,
    Missing,
}

#[must_use]
pub const fn lookup_evidence(facts: LookupFacts) -> LookupEvidence {
    if matches!(facts.skip, LookupSkip::Excluded) {
        LookupEvidence::NotApplicable
    } else if facts.warnings {
        LookupEvidence::Missing
    } else if matches!(facts.skip, LookupSkip::None) || (facts.covered && !facts.updates) {
        LookupEvidence::Verified
    } else if matches!(facts.selection, VersionSelection::Absent)
        && !facts.updates
        && matches!(
            facts.skip,
            LookupSkip::InvalidValue | LookupSkip::UnspecifiedVersion
        )
    {
        LookupEvidence::NotApplicable
    } else {
        LookupEvidence::Missing
    }
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

    pub fn spec_lookup(facts: LookupFacts) {
        let result = lookup_evidence(facts);
        if facts.skip == LookupSkip::Excluded {
            assert_eq!(result, LookupEvidence::NotApplicable);
        }
        if facts.skip != LookupSkip::Excluded && !facts.warnings && facts.covered && !facts.updates
        {
            assert_eq!(result, LookupEvidence::Verified);
        }
        if matches!(
            facts.skip,
            LookupSkip::InvalidValue | LookupSkip::UnspecifiedVersion
        ) && facts.selection == VersionSelection::Absent
            && !facts.updates
            && !facts.warnings
            && !facts.covered
        {
            assert_eq!(result, LookupEvidence::NotApplicable);
        }
        if result == LookupEvidence::NotApplicable && facts.skip != LookupSkip::Excluded {
            assert_eq!(facts.selection, VersionSelection::Absent);
            assert!(!facts.updates && !facts.warnings);
            assert!(matches!(
                facts.skip,
                LookupSkip::InvalidValue | LookupSkip::UnspecifiedVersion
            ));
        }
        if facts.warnings && facts.skip != LookupSkip::Excluded {
            assert_eq!(result, LookupEvidence::Missing);
        }
        if facts.skip == LookupSkip::Other && !facts.covered {
            assert_eq!(result, LookupEvidence::Missing);
        }
        if matches!(
            facts.skip,
            LookupSkip::InvalidValue | LookupSkip::UnspecifiedVersion
        ) && (facts.selection == VersionSelection::Present || facts.updates)
            && !facts.covered
        {
            assert_eq!(result, LookupEvidence::Missing);
        }
        if facts.skip == LookupSkip::None && !facts.warnings {
            assert_eq!(result, LookupEvidence::Verified);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn report_severity_rejects_errors_and_unknown_levels() {
        let level: u64 = kani::any();
        let result = problem_severity(level);
        assert_eq!(
            result == ProblemSeverity::Notice,
            matches!(level, 10 | 20 | 30 | 40)
        );
        assert_eq!(result == ProblemSeverity::Error, matches!(level, 50 | 60));
        assert_eq!(
            result == ProblemSeverity::Invalid,
            !matches!(level, 10 | 20 | 30 | 40 | 50 | 60)
        );
        kani::cover!(result == ProblemSeverity::Notice);
        kani::cover!(result == ProblemSeverity::Error);
        kani::cover!(result == ProblemSeverity::Invalid);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn lookup_exemptions_preserve_required_evidence() {
        let (kind, selected, updates, warnings, covered): (u8, bool, bool, bool, bool) =
            kani::any();
        let skip = match kind {
            0 => LookupSkip::None,
            1 => LookupSkip::Excluded,
            2 => LookupSkip::InvalidValue,
            3 => LookupSkip::UnspecifiedVersion,
            _ => LookupSkip::Other,
        };
        let facts = LookupFacts {
            skip,
            selection: if selected {
                VersionSelection::Present
            } else {
                VersionSelection::Absent
            },
            updates,
            warnings,
            covered,
        };
        spec_lookup(facts);
        let result = lookup_evidence(facts);
        kani::cover!(result == LookupEvidence::Verified);
        kani::cover!(result == LookupEvidence::NotApplicable);
        kani::cover!(result == LookupEvidence::Missing);
    }

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
    use super::ProblemSeverity;

    #[test]
    fn problem_severity_accepts_only_the_fixed_level_codes() {
        for (level, expected) in [
            (0, ProblemSeverity::Invalid),
            (9, ProblemSeverity::Invalid),
            (10, ProblemSeverity::Notice),
            (20, ProblemSeverity::Notice),
            (30, ProblemSeverity::Notice),
            (40, ProblemSeverity::Notice),
            (49, ProblemSeverity::Invalid),
            (50, ProblemSeverity::Error),
            (60, ProblemSeverity::Error),
            (61, ProblemSeverity::Invalid),
            (u64::MAX, ProblemSeverity::Invalid),
        ] {
            assert_eq!(super::problem_severity(level), expected);
        }
    }

    #[test]
    fn every_lookup_state_preserves_the_contract() {
        use super::{LookupFacts, LookupSkip, VersionSelection};
        for skip in [
            LookupSkip::None,
            LookupSkip::Excluded,
            LookupSkip::InvalidValue,
            LookupSkip::UnspecifiedVersion,
            LookupSkip::Other,
        ] {
            for [selected, updates, warnings, covered] in crate::domain::flags() {
                super::proofs::spec_lookup(LookupFacts {
                    skip,
                    selection: if selected {
                        VersionSelection::Present
                    } else {
                        VersionSelection::Absent
                    },
                    updates,
                    warnings,
                    covered,
                });
            }
        }
    }
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
