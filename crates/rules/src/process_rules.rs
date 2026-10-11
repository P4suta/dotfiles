#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallPath {
    Direct,
    Shim,
}

pub const CMD_PATH_LIMIT: usize = 8_191;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellPath {
    Usable,
    Discarded,
}

/// Whether cmd retains an inherited PATH of this many UTF-16 code units.
#[must_use]
pub const fn shell_path(code_units: usize) -> ShellPath {
    if code_units <= CMD_PATH_LIMIT {
        ShellPath::Usable
    } else {
        ShellPath::Discarded
    }
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

    pub fn spec_shell_path(code_units: usize) {
        assert_eq!(
            shell_path(code_units) == ShellPath::Usable,
            code_units < 8_192
        );
        assert_eq!(
            shell_path(code_units) == ShellPath::Discarded,
            code_units >= 8_192
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_shell_path_respects_the_command_interpreters_limit() {
        let code_units = kani::any();
        spec_shell_path(code_units);
        kani::cover!(shell_path(code_units) == ShellPath::Usable);
        kani::cover!(shell_path(code_units) == ShellPath::Discarded);
    }

    #[cfg(test)]
    #[test]
    fn shell_paths_cover_empty_boundary_and_overflow_lengths() {
        for code_units in [
            0,
            CMD_PATH_LIMIT - 1,
            CMD_PATH_LIMIT,
            CMD_PATH_LIMIT + 1,
            usize::MAX,
        ] {
            spec_shell_path(code_units);
        }
    }

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
