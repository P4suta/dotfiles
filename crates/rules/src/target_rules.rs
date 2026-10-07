/// Where a hook gate builds its Cargo artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// The absolute `CARGO_TARGET_DIR` the caller already chose.
    Configured,
    /// One shared directory on the volume that holds `TEMP`, the development drive on Windows.
    Development,
    /// One shared directory in the home directory, for hosts without such a volume.
    Home,
}

/// A relative `CARGO_TARGET_DIR` resolves relative to the worktree, so the rule honors only an absolute one.
#[must_use]
pub fn target(configured_absolute: bool, temp_volume: bool) -> Target {
    if configured_absolute {
        Target::Configured
    } else if temp_volume {
        Target::Development
    } else {
        Target::Home
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_target(configured_absolute: bool, temp_volume: bool) {
        let decided = target(configured_absolute, temp_volume);
        assert_eq!(decided == Target::Configured, configured_absolute);
        assert_eq!(
            decided == Target::Development,
            !configured_absolute && temp_volume
        );
        assert_eq!(
            decided == Target::Home,
            !configured_absolute && !temp_volume
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn an_absolute_configured_target_is_always_honored() {
        let temp_volume: bool = kani::any();
        spec_target(true, temp_volume);
        kani::cover!(temp_volume);
        kani::cover!(!temp_volume);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn an_unconfigured_target_is_shared_and_never_inside_the_worktree() {
        let temp_volume: bool = kani::any();
        spec_target(false, temp_volume);
        let decided = target(false, temp_volume);
        kani::cover!(decided == Target::Development);
        kani::cover!(decided == Target::Home);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_input_meets_the_specification() {
        for [configured_absolute, temp_volume] in crate::domain::flags() {
            super::proofs::spec_target(configured_absolute, temp_volume);
        }
    }
}
