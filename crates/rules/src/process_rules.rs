#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallPath {
    Direct,
    Shim,
}

/// Classify a configured shim or a native mise/shims directory before installation.
#[must_use]
pub const fn install_path(facts: [bool; 3]) -> InstallPath {
    let [configured, mise_parent, shims_name] = facts;
    if configured || (mise_parent && shims_name) {
        InstallPath::Shim
    } else {
        InstallPath::Direct
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_install_path(facts: [bool; 3]) {
        let result = install_path(facts);
        assert_eq!(
            result == InstallPath::Shim,
            facts[0] || (facts[1] && facts[2])
        );
        assert_eq!(
            result == InstallPath::Direct,
            !facts[0] && (!facts[1] || !facts[2])
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_installation_path_excludes_mise_shims() {
        let facts = kani::any();
        spec_install_path(facts);
        kani::cover!(install_path(facts) == InstallPath::Direct);
        kani::cover!(install_path(facts) == InstallPath::Shim);
    }

    #[cfg(test)]
    #[test]
    fn configured_and_standard_shims_are_excluded_for_every_fact_combination() {
        for configured in [false, true] {
            for mise_parent in [false, true] {
                for shims_name in [false, true] {
                    spec_install_path([configured, mise_parent, shims_name]);
                }
            }
        }
    }
}
