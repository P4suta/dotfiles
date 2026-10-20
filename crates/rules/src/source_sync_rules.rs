//! Admission of an unattended fast-forward of the applied dotfiles source.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Checkout {
    pub on_main: bool,
    pub clean: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Remote {
    FetchFailed,
    Diverged,
    FastForward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkipReason {
    Dirty,
    OffMain,
    Diverged,
    FetchFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    Move,
    Skip(SkipReason),
}

#[must_use]
pub fn preflight(checkout: Checkout) -> Option<SkipReason> {
    if !checkout.clean {
        Some(SkipReason::Dirty)
    } else if !checkout.on_main {
        Some(SkipReason::OffMain)
    } else {
        None
    }
}

#[must_use]
pub fn decide(checkout: Checkout, remote: Remote) -> Decision {
    if let Some(reason) = preflight(checkout) {
        return Decision::Skip(reason);
    }
    match remote {
        Remote::FetchFailed => Decision::Skip(SkipReason::FetchFailed),
        Remote::Diverged => Decision::Skip(SkipReason::Diverged),
        Remote::FastForward => Decision::Move,
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_decide(on_main: bool, clean: bool, remote: Remote) {
        let checkout = Checkout { on_main, clean };
        assert_eq!(
            preflight(checkout),
            match (clean, on_main) {
                (false, _) => Some(SkipReason::Dirty),
                (true, false) => Some(SkipReason::OffMain),
                (true, true) => None,
            }
        );
        let expected = match (clean, on_main, remote) {
            (false, _, _) => Decision::Skip(SkipReason::Dirty),
            (true, false, _) => Decision::Skip(SkipReason::OffMain),
            (true, true, Remote::FetchFailed) => Decision::Skip(SkipReason::FetchFailed),
            (true, true, Remote::Diverged) => Decision::Skip(SkipReason::Diverged),
            (true, true, Remote::FastForward) => Decision::Move,
        };
        assert_eq!(decide(checkout, remote), expected);
        assert_eq!(
            decide(checkout, remote) == Decision::Move,
            clean && on_main && remote == Remote::FastForward
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(1)]
    fn only_a_clean_main_with_a_fetched_fast_forward_can_move() {
        let on_main = kani::any();
        let clean = kani::any();
        let remote = kani::any();
        spec_decide(on_main, clean, remote);
        let decision = decide(Checkout { on_main, clean }, remote);
        kani::cover!(decision == Decision::Move);
        kani::cover!(decision == Decision::Skip(SkipReason::Dirty));
        kani::cover!(decision == Decision::Skip(SkipReason::OffMain));
        kani::cover!(decision == Decision::Skip(SkipReason::Diverged));
        kani::cover!(decision == Decision::Skip(SkipReason::FetchFailed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_checkout_and_remote_state_meets_the_specification() {
        for on_main in [false, true] {
            for clean in [false, true] {
                for remote in [Remote::FetchFailed, Remote::Diverged, Remote::FastForward] {
                    proofs::spec_decide(on_main, clean, remote);
                }
            }
        }
    }

    #[test]
    fn a_clean_main_can_move_even_when_already_current() {
        assert_eq!(
            decide(
                Checkout {
                    on_main: true,
                    clean: true
                },
                Remote::FastForward
            ),
            Decision::Move
        );
    }

    #[test]
    fn dirty_work_is_preserved_before_any_fetch() {
        assert_eq!(
            preflight(Checkout {
                on_main: true,
                clean: false
            }),
            Some(SkipReason::Dirty)
        );
    }

    #[test]
    fn another_branch_or_detached_head_is_preserved() {
        assert_eq!(
            preflight(Checkout {
                on_main: false,
                clean: true
            }),
            Some(SkipReason::OffMain)
        );
    }

    #[test]
    fn a_failed_fetch_cannot_use_a_stale_tracking_ref() {
        assert_eq!(
            decide(
                Checkout {
                    on_main: true,
                    clean: true
                },
                Remote::FetchFailed
            ),
            Decision::Skip(SkipReason::FetchFailed)
        );
    }

    #[test]
    fn a_local_commit_is_not_discarded() {
        assert_eq!(
            decide(
                Checkout {
                    on_main: true,
                    clean: true
                },
                Remote::Diverged
            ),
            Decision::Skip(SkipReason::Diverged)
        );
    }
}
