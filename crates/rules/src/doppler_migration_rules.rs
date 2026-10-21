#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Mode {
    ReadOnly,
    Apply,
}

#[must_use]
pub const fn may_copy(
    mode: Mode,
    source_matches: bool,
    destination_matches: bool,
    distinct: bool,
) -> bool {
    matches!(mode, Mode::Apply) && source_matches && destination_matches && distinct
}

#[cfg(kani)]
mod proofs {
    #[kani::proof]
    fn copy_requires_explicit_apply_and_distinct_verified_workplaces() {
        let mode: super::Mode = kani::any();
        let source_matches: bool = kani::any();
        let destination_matches: bool = kani::any();
        let distinct: bool = kani::any();
        let allowed = super::may_copy(mode, source_matches, destination_matches, distinct);
        assert_eq!(
            allowed,
            matches!(mode, super::Mode::Apply) && source_matches && destination_matches && distinct
        );
        kani::cover!(allowed);
        kani::cover!(!allowed);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mutations_require_apply_and_two_distinct_verified_workplaces() {
        for mode in [super::Mode::ReadOnly, super::Mode::Apply] {
            for source_matches in [false, true] {
                for destination_matches in [false, true] {
                    for distinct in [false, true] {
                        assert_eq!(
                            super::may_copy(mode, source_matches, destination_matches, distinct),
                            mode == super::Mode::Apply
                                && source_matches
                                && destination_matches
                                && distinct
                        );
                    }
                }
            }
        }
    }
}
