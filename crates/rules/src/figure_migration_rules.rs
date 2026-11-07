#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Mode {
    Plan,
    Apply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Verification {
    Missing,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum BranchIdentity {
    Changed,
    Unchanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Evidence {
    pub archive: Verification,
    pub inventory: Verification,
    pub documents: Verification,
    pub remaining_references: u64,
    pub branch: BranchIdentity,
}

#[must_use]
pub const fn may_retire(mode: Mode, evidence: Evidence) -> bool {
    matches!(mode, Mode::Apply)
        && matches!(evidence.archive, Verification::Complete)
        && matches!(evidence.inventory, Verification::Complete)
        && matches!(evidence.documents, Verification::Complete)
        && evidence.remaining_references == 0
        && matches!(evidence.branch, BranchIdentity::Unchanged)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum ScopeMatch {
    Different,
    Exact,
}

#[must_use]
pub const fn permits_retirement(token: ScopeMatch, request: ScopeMatch) -> bool {
    matches!(token, ScopeMatch::Exact) && matches!(request, ScopeMatch::Exact)
}

#[cfg(kani)]
mod proofs {
    #[kani::proof]
    fn retirement_permission_requires_matching_token_and_request() {
        let token: super::ScopeMatch = kani::any();
        let request: super::ScopeMatch = kani::any();
        let allowed = super::permits_retirement(token, request);
        assert_eq!(
            allowed,
            token == super::ScopeMatch::Exact && request == super::ScopeMatch::Exact
        );
        kani::cover!(allowed);
        kani::cover!(!allowed);
    }

    #[kani::proof]
    fn retirement_requires_complete_preservation_and_no_remaining_references() {
        let mode: super::Mode = kani::any();
        let evidence: super::Evidence = kani::any();
        let allowed = super::may_retire(mode, evidence);
        assert_eq!(
            allowed,
            mode == super::Mode::Apply
                && evidence.archive == super::Verification::Complete
                && evidence.inventory == super::Verification::Complete
                && evidence.documents == super::Verification::Complete
                && evidence.remaining_references == 0
                && evidence.branch == super::BranchIdentity::Unchanged
        );
        kani::cover!(allowed);
        kani::cover!(!allowed);
    }
}

#[cfg(test)]
mod tests {
    use super::{BranchIdentity, Evidence, Mode, Verification, may_retire};

    #[test]
    fn every_permission_scope_requires_both_matches() {
        use super::{ScopeMatch, permits_retirement};
        for token in [ScopeMatch::Different, ScopeMatch::Exact] {
            for request in [ScopeMatch::Different, ScopeMatch::Exact] {
                assert_eq!(
                    permits_retirement(token, request),
                    token == ScopeMatch::Exact && request == ScopeMatch::Exact
                );
            }
        }
    }

    #[test]
    fn every_missing_preservation_condition_blocks_retirement() {
        for mode in [Mode::Plan, Mode::Apply] {
            for bits in 0_u8..16 {
                for remaining_references in [0, 1, u64::MAX] {
                    let evidence = Evidence {
                        archive: if bits & 1 != 0 {
                            Verification::Complete
                        } else {
                            Verification::Missing
                        },
                        inventory: if bits & 2 != 0 {
                            Verification::Complete
                        } else {
                            Verification::Missing
                        },
                        documents: if bits & 4 != 0 {
                            Verification::Complete
                        } else {
                            Verification::Missing
                        },
                        remaining_references,
                        branch: if bits & 8 != 0 {
                            BranchIdentity::Unchanged
                        } else {
                            BranchIdentity::Changed
                        },
                    };
                    assert_eq!(
                        may_retire(mode, evidence),
                        mode == Mode::Apply && bits == 15 && remaining_references == 0
                    );
                }
            }
        }
    }
}
