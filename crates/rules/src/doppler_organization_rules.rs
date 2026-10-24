#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Mode {
    ReadOnly,
    Apply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Change {
    Prepare,
    DisableSourceSync,
    RetireConsumer,
    RemoveUnused,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Scope {
    pub workplace: bool,
    pub repository: bool,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Cutover {
    pub destination: bool,
    pub consumer: bool,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Evidence {
    pub scope: Scope,
    pub capacity: bool,
    pub cutover: Cutover,
    pub unused: bool,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum ConfigInventory {
    Distinct { legacy: bool, canonical: bool },
    Shared { canonical: bool, combined: bool },
}

#[must_use]
pub const fn config_inventory_matches(inventory: ConfigInventory) -> bool {
    match inventory {
        ConfigInventory::Distinct { legacy, canonical } => legacy && canonical,
        ConfigInventory::Shared {
            canonical,
            combined,
        } => canonical || combined,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Allowed,
    ReadOnly,
    ScopeMismatch,
    CapacityExceeded,
    DestinationUnverified,
    ConsumerUnverified,
    StillUsed,
}

#[must_use]
pub const fn decide(mode: Mode, change: Change, evidence: Evidence) -> Verdict {
    if matches!(mode, Mode::ReadOnly) {
        return Verdict::ReadOnly;
    }
    if !evidence.scope.workplace || !evidence.scope.repository {
        return Verdict::ScopeMismatch;
    }
    match change {
        Change::Prepare if !evidence.capacity => Verdict::CapacityExceeded,
        Change::DisableSourceSync | Change::RetireConsumer if !evidence.cutover.destination => {
            Verdict::DestinationUnverified
        }
        Change::RetireConsumer if !evidence.cutover.consumer => Verdict::ConsumerUnverified,
        Change::RemoveUnused if !evidence.unused => Verdict::StillUsed,
        Change::Prepare
        | Change::DisableSourceSync
        | Change::RetireConsumer
        | Change::RemoveUnused => Verdict::Allowed,
    }
}

#[cfg(kani)]
mod proofs {
    #[kani::proof]
    fn config_inventories_require_complete_declared_states() {
        let inventory: super::ConfigInventory = kani::any();
        let accepted = super::config_inventory_matches(inventory);
        match inventory {
            super::ConfigInventory::Distinct { legacy, canonical } => {
                assert_eq!(accepted, legacy && canonical);
            }
            super::ConfigInventory::Shared {
                canonical,
                combined,
            } => {
                assert_eq!(accepted, canonical || combined);
            }
        }
        kani::cover!(accepted);
        kani::cover!(!accepted);
        kani::cover!(matches!(
            inventory,
            super::ConfigInventory::Shared {
                canonical: false,
                combined: true
            }
        ));
        kani::cover!(matches!(
            inventory,
            super::ConfigInventory::Shared {
                canonical: true,
                combined: false
            }
        ));
    }

    #[kani::proof]
    fn changes_require_explicit_apply_and_verified_scope() {
        let mode: super::Mode = kani::any();
        let change: super::Change = kani::any();
        let evidence: super::Evidence = kani::any();
        let allowed = super::decide(mode, change, evidence) == super::Verdict::Allowed;
        if allowed {
            assert!(matches!(mode, super::Mode::Apply));
            assert!(evidence.scope.workplace && evidence.scope.repository);
        }
        kani::cover!(allowed);
        kani::cover!(!allowed);
    }

    #[kani::proof]
    fn consumer_retirement_needs_live_cutover_evidence() {
        let evidence: super::Evidence = kani::any();
        let allowed = super::decide(super::Mode::Apply, super::Change::RetireConsumer, evidence)
            == super::Verdict::Allowed;
        if allowed {
            assert!(evidence.cutover.destination && evidence.cutover.consumer);
        }
        kani::cover!(allowed);
        kani::cover!(!allowed);
    }

    #[kani::proof]
    fn preparation_and_cleanup_require_their_own_evidence() {
        let evidence: super::Evidence = kani::any();
        let prepare = super::decide(super::Mode::Apply, super::Change::Prepare, evidence);
        let remove = super::decide(super::Mode::Apply, super::Change::RemoveUnused, evidence);
        let disable = super::decide(
            super::Mode::Apply,
            super::Change::DisableSourceSync,
            evidence,
        );
        if prepare == super::Verdict::Allowed {
            assert!(evidence.capacity);
        }
        if remove == super::Verdict::Allowed {
            assert!(evidence.unused);
        }
        if disable == super::Verdict::Allowed {
            assert!(evidence.cutover.destination);
        }
        kani::cover!(prepare == super::Verdict::Allowed);
        kani::cover!(remove == super::Verdict::Allowed);
        kani::cover!(disable == super::Verdict::Allowed);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn distinct_inventories_require_both_matches_and_shared_inventories_require_one_complete_state()
    {
        use super::ConfigInventory::{Distinct, Shared};
        for (inventory, expected) in [
            (
                Distinct {
                    legacy: false,
                    canonical: false,
                },
                false,
            ),
            (
                Distinct {
                    legacy: false,
                    canonical: true,
                },
                false,
            ),
            (
                Distinct {
                    legacy: true,
                    canonical: false,
                },
                false,
            ),
            (
                Distinct {
                    legacy: true,
                    canonical: true,
                },
                true,
            ),
            (
                Shared {
                    canonical: false,
                    combined: false,
                },
                false,
            ),
            (
                Shared {
                    canonical: false,
                    combined: true,
                },
                true,
            ),
            (
                Shared {
                    canonical: true,
                    combined: false,
                },
                true,
            ),
            (
                Shared {
                    canonical: true,
                    combined: true,
                },
                true,
            ),
        ] {
            assert_eq!(super::config_inventory_matches(inventory), expected);
        }
    }

    #[test]
    fn unavailable_destination_preserves_the_source_writer() {
        let evidence = super::Evidence {
            scope: super::Scope {
                workplace: true,
                repository: true,
            },
            capacity: true,
            cutover: super::Cutover {
                destination: false,
                consumer: false,
            },
            unused: false,
        };
        assert_eq!(
            super::decide(
                super::Mode::Apply,
                super::Change::DisableSourceSync,
                evidence
            ),
            super::Verdict::DestinationUnverified
        );
    }
}
