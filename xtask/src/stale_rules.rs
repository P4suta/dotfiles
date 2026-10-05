/// How the build of an installed tool compares with the sources of the checkout it runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    Current,
    Stale,
    /// The search found no checkout, so nothing shows that the installation lags.
    Unverified,
}

/// A build lags only when the search found a checkout and the files that build the tool differ from the hash the tool embeds.
pub const fn freshness(checkout_found: bool, hashes_match: bool) -> Freshness {
    match (checkout_found, hashes_match) {
        (false, _) => Freshness::Unverified,
        (true, true) => Freshness::Current,
        (true, false) => Freshness::Stale,
    }
}

/// A stale tool refuses to run.
/// Every other state runs.
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

/// The rebuild touches a recorded installation only when it exists and no longer matches the sources.
/// A tool that the owner never installed stays absent.
pub const fn reinstalls(recorded: bool, recorded_matches: bool) -> bool {
    recorded && !recorded_matches
}

#[cfg(kani)]
#[kani::proof]
fn only_a_found_differing_build_is_stale() {
    let found: bool = kani::any();
    let matching: bool = kani::any();
    let state = freshness(found, matching);
    assert_eq!(state == Freshness::Stale, found && !matching);
    assert_eq!(state == Freshness::Unverified, !found);
    assert_eq!(state == Freshness::Current, found && matching);
    kani::cover!(state == Freshness::Stale);
    kani::cover!(state == Freshness::Current);
    kani::cover!(state == Freshness::Unverified);
}

#[cfg(kani)]
#[kani::proof]
fn a_stale_build_never_runs() {
    let found: bool = kani::any();
    let matching: bool = kani::any();
    let accepted = runs(freshness(found, matching));
    assert_eq!(accepted, !found || matching);
    assert!(!(found && !matching) || !accepted);
    kani::cover!(accepted);
    kani::cover!(!accepted);
}

#[cfg(kani)]
#[kani::proof]
fn the_nearest_named_checkout_wins() {
    let explicit: bool = kani::any();
    let working: bool = kani::any();
    let built: bool = kani::any();
    let chosen = origin(explicit, working, built);
    assert_eq!(chosen.is_some(), explicit || working || built);
    assert_eq!(chosen == Some(Origin::Explicit), explicit);
    assert_eq!(chosen == Some(Origin::WorkingCopy), !explicit && working);
    assert_eq!(
        chosen == Some(Origin::BuildCheckout),
        !explicit && !working && built
    );
    kani::cover!(chosen == Some(Origin::Explicit));
    kani::cover!(chosen == Some(Origin::WorkingCopy));
    kani::cover!(chosen == Some(Origin::BuildCheckout));
    kani::cover!(chosen.is_none());
}

#[cfg(kani)]
#[kani::proof]
fn only_a_recorded_lagging_installation_is_rebuilt() {
    let recorded: bool = kani::any();
    let matching: bool = kani::any();
    let rebuild = reinstalls(recorded, matching);
    assert_eq!(rebuild, recorded && !matching);
    assert!(recorded || !rebuild);
    kani::cover!(rebuild);
    kani::cover!(!rebuild && recorded);
    kani::cover!(!rebuild && !recorded);
}
