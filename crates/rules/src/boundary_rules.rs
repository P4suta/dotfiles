//! What keeps the decision core pure: no dependency, no standard library, the panic lints denied, and every rule module inside it.

/// The lints the core denies, so it returns each failure as a value.
pub const DENIED: [&str; 10] = [
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::panic",
    "clippy::todo",
    "clippy::unimplemented",
    "clippy::unreachable",
    "clippy::indexing_slicing",
    "clippy::string_slice",
    "clippy::exit",
    "clippy::mem_forget",
];

/// The facts the boundary gate reads from the core crate and the workspace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Core {
    /// Dependencies the core's manifest declares, of any kind.
    pub dependencies: usize,
    /// The core's root declares `#![no_std]`.
    pub no_std: bool,
    /// One bit per entry of `DENIED` that the core's root denies.
    pub denied: u16,
    /// Rule modules that live outside the core.
    pub strays: usize,
}

/// The first way a core breaks its boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Breach {
    /// A dependency could carry an adapter or an effect into the core.
    Dependency,
    /// The standard library reaches files, processes, the network, clocks, and the environment.
    Std,
    /// The core may panic through this lint, which its root no longer denies.
    Undenied(usize),
    /// A rule module outside the core escapes these checks.
    Stray,
}

impl Breach {
    /// The corrective next action for the breach.
    #[must_use]
    pub const fn next_action(self) -> &'static str {
        match self {
            Self::Dependency => {
                "Remove the dependency from crates/rules/Cargo.toml and move the code that needs it to the adapter that calls the rule."
            }
            Self::Std => {
                "Restore `#![no_std]` in crates/rules/src/lib.rs and pass the fact the rule needed from its adapter."
            }
            Self::Undenied(_) => {
                "Restore the lint in the `#![deny(...)]` list of crates/rules/src/lib.rs and return the failure as an Option or a Result."
            }
            Self::Stray => {
                "Move the rule module into crates/rules/src/ and re-export it from the crate that calls it."
            }
        }
    }
}

/// The breach a core shows, or `None` when it keeps its boundary.
#[must_use]
pub fn breach(core: Core) -> Option<Breach> {
    if core.dependencies > 0 {
        return Some(Breach::Dependency);
    }
    if !core.no_std {
        return Some(Breach::Std);
    }
    let missing = !core.denied & ((1 << DENIED.len()) - 1);
    if missing != 0 {
        return Some(Breach::Undenied(missing.trailing_zeros() as usize));
    }
    if core.strays > 0 {
        return Some(Breach::Stray);
    }
    None
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// A core passes only with no dependency, `no_std`, every lint, and no stray, and otherwise names its first breach in that order.
    pub fn spec_breach(core: Core) {
        let every = (1u16 << DENIED.len()) - 1;
        let result = breach(core);
        assert_eq!(
            result.is_none(),
            core.dependencies == 0
                && core.no_std
                && core.denied & every == every
                && core.strays == 0
        );
        assert_eq!(
            matches!(result, Some(Breach::Dependency)),
            core.dependencies > 0
        );
        assert_eq!(
            matches!(result, Some(Breach::Std)),
            core.dependencies == 0 && !core.no_std
        );
        if let Some(Breach::Undenied(index)) = result {
            assert!(index < DENIED.len());
            assert_eq!(core.denied & (1 << index), 0);
            assert_eq!(core.denied & ((1 << index) - 1), (1 << index) - 1);
        }
        assert_eq!(
            matches!(result, Some(Breach::Stray)),
            core.dependencies == 0
                && core.no_std
                && core.denied & every == every
                && core.strays > 0
        );
    }

    #[cfg(kani)]
    fn any_core() -> Core {
        Core {
            dependencies: kani::any(),
            no_std: kani::any(),
            denied: kani::any(),
            strays: kani::any(),
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(11)]
    fn a_core_passes_only_with_no_dependency_no_std_every_lint_and_no_stray() {
        let core = any_core();
        spec_breach(core);
        let result = breach(core);
        kani::cover!(result.is_none());
        kani::cover!(matches!(result, Some(Breach::Stray)));
        kani::cover!(result == Some(Breach::Undenied(DENIED.len() - 1)));
        kani::cover!((core.dependencies > 0) & !core.no_std);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every lint set")]
    fn every_lint_set_meets_the_boundary_specification() {
        for dependencies in [0, 1, usize::MAX] {
            for no_std in [false, true] {
                for denied in 0..=u16::MAX {
                    for strays in [0, 1, usize::MAX] {
                        proofs::spec_breach(Core {
                            dependencies,
                            no_std,
                            denied,
                            strays,
                        });
                    }
                }
            }
        }
    }

    const PURE: Core = Core {
        dependencies: 0,
        no_std: true,
        denied: (1 << DENIED.len()) - 1,
        strays: 0,
    };

    #[test]
    fn a_pure_core_passes_and_each_breach_names_its_cause() {
        assert_eq!(breach(PURE), None);
        assert_eq!(
            breach(Core {
                dependencies: 1,
                ..PURE
            }),
            Some(Breach::Dependency)
        );
        assert_eq!(
            breach(Core {
                no_std: false,
                ..PURE
            }),
            Some(Breach::Std)
        );
        assert_eq!(
            breach(Core {
                denied: PURE.denied & !(1 << 6),
                ..PURE
            }),
            Some(Breach::Undenied(6))
        );
        assert_eq!(breach(Core { strays: 2, ..PURE }), Some(Breach::Stray));
    }

    #[test]
    fn every_breach_names_a_next_action() {
        for found in [
            Breach::Dependency,
            Breach::Std,
            Breach::Undenied(0),
            Breach::Stray,
        ] {
            assert!(found.next_action().contains("crates/rules"));
        }
    }
}
