/// How the build of an installed tool compares with the sources of the checkout it runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    Current,
    Stale,
    /// The search found no checkout, so nothing shows that the installation lags.
    Unverified,
}

/// A build lags only when the search found a checkout and the files that build the tool differ from the hash the tool embeds.
#[must_use]
pub const fn freshness(checkout_found: bool, hashes_match: bool) -> Freshness {
    match (checkout_found, hashes_match) {
        (false, _) => Freshness::Unverified,
        (true, true) => Freshness::Current,
        (true, false) => Freshness::Stale,
    }
}

/// A stale tool refuses to run.
/// Every other state runs.
#[must_use]
pub const fn runs(freshness: Freshness) -> bool {
    !matches!(freshness, Freshness::Stale)
}

/// The origin of the checkout to compare with, most specific first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The owner or a caller named it.
    Explicit,
    /// The dotfiles checkout that contains the working directory of the tool.
    WorkingCopy,
    /// The checkout that built the tool, while it still exists.
    BuildCheckout,
}

/// The default chezmoi source forms a bare repository whose head lags, so no origin ever names it.
#[must_use]
pub const fn origin(explicit: bool, working_copy: bool, build_checkout: bool) -> Option<Origin> {
    if explicit {
        Some(Origin::Explicit)
    } else if working_copy {
        Some(Origin::WorkingCopy)
    } else if build_checkout {
        Some(Origin::BuildCheckout)
    } else {
        None
    }
}

/// The rebuild touches only a recorded installation from this checkout whose content no longer matches.
/// A tool that the owner never installed stays absent.
#[must_use]
pub const fn reinstalls(recorded: bool, source_matches: bool, recorded_matches: bool) -> bool {
    recorded && source_matches && !recorded_matches
}

/// A stale tool that runs inside a dotfiles checkout builds that checkout's sources and runs the build once, instead of refusing.
/// Parallel worktrees then never reinstall one shared binary over each other.
/// A checkout named by a caller, the checkout that built the tool, and a run that already came from a checkout's build keep the refusal.
#[must_use]
pub const fn delegates(freshness: Freshness, origin: Origin, delegated: bool) -> bool {
    matches!(freshness, Freshness::Stale) && matches!(origin, Origin::WorkingCopy) && !delegated
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_delegates(state: Freshness, chosen: Origin, delegated: bool) {
        let handed = delegates(state, chosen, delegated);
        assert_eq!(
            handed,
            state == Freshness::Stale && chosen == Origin::WorkingCopy && !delegated
        );
        assert!(!handed || !runs(state));
        assert!(!delegates(state, Origin::Explicit, delegated));
        assert!(!delegates(state, Origin::BuildCheckout, delegated));
        assert!(!delegates(state, chosen, true));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_stale_build_inside_its_working_copy_hands_the_run_to_that_checkout_once() {
        let state = freshness(kani::any(), kani::any());
        let Some(chosen) = origin(kani::any(), kani::any(), true) else {
            return;
        };
        let delegated: bool = kani::any();
        spec_delegates(state, chosen, delegated);
        kani::cover!(delegates(state, chosen, delegated));
        kani::cover!(!delegates(state, chosen, delegated));
    }

    pub fn spec_freshness(found: bool, matching: bool) {
        let state = freshness(found, matching);
        assert_eq!(state == Freshness::Stale, found && !matching);
        assert_eq!(state == Freshness::Unverified, !found);
        assert_eq!(state == Freshness::Current, found && matching);
        assert_eq!(runs(state), !found || matching);
    }

    pub fn spec_origin(explicit: bool, working: bool, built: bool) {
        let chosen = origin(explicit, working, built);
        assert_eq!(chosen.is_some(), explicit || working || built);
        assert_eq!(chosen == Some(Origin::Explicit), explicit);
        assert_eq!(chosen == Some(Origin::WorkingCopy), !explicit && working);
        assert_eq!(
            chosen == Some(Origin::BuildCheckout),
            !explicit && !working && built
        );
    }

    pub fn spec_reinstalls(recorded: bool, source_matches: bool, matching: bool) {
        let rebuild = reinstalls(recorded, source_matches, matching);
        assert_eq!(rebuild, recorded && source_matches && !matching);
        assert!(!rebuild || source_matches);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_found_differing_build_is_stale_and_refuses_to_run() {
        let found: bool = kani::any();
        let matching: bool = kani::any();
        spec_freshness(found, matching);
        let state = freshness(found, matching);
        kani::cover!(state == Freshness::Stale);
        kani::cover!(state == Freshness::Current);
        kani::cover!(state == Freshness::Unverified);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn the_nearest_named_checkout_wins() {
        let explicit: bool = kani::any();
        let working: bool = kani::any();
        let built: bool = kani::any();
        spec_origin(explicit, working, built);
        let chosen = origin(explicit, working, built);
        kani::cover!(chosen == Some(Origin::WorkingCopy));
        kani::cover!(chosen == Some(Origin::BuildCheckout));
        kani::cover!(chosen.is_none());
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_recorded_lagging_installation_is_rebuilt() {
        let recorded: bool = kani::any();
        let source_matches: bool = kani::any();
        let matching: bool = kani::any();
        spec_reinstalls(recorded, source_matches, matching);
        let reinstall = reinstalls(recorded, source_matches, matching);
        kani::cover!(reinstall);
        kani::cover!(!reinstall && recorded && !source_matches);
        kani::cover!(!reinstall && recorded && source_matches);
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::flags;

    #[test]
    fn every_input_meets_the_specification() {
        for [first, second] in flags() {
            super::proofs::spec_freshness(first, second);
        }
        for [explicit, working, built] in flags() {
            super::proofs::spec_origin(explicit, working, built);
            super::proofs::spec_reinstalls(explicit, working, built);
        }
        for [found, matching, explicit, working, delegated] in flags() {
            if let Some(chosen) = super::origin(explicit, working, true) {
                super::proofs::spec_delegates(super::freshness(found, matching), chosen, delegated);
            }
        }
    }
}
