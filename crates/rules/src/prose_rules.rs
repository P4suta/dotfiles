/// An exemption needs a stated reason, and an exemption that matches nothing fails as stale.
#[must_use]
pub fn exemption_valid(reason_present: bool, used: bool) -> bool {
    reason_present && used
}

/// The personal writing standard governs a destination that a personal owner holds, unless that destination forks another repository.
/// Other destinations follow their own rules.
#[must_use]
pub fn standard_applies(personal_owner: bool, fork: bool) -> bool {
    personal_owner && !fork
}

/// The owner a remote names: a personal or foreign GitHub owner, or an address that names no GitHub owner, such as a bare-repository hub.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Owner {
    Personal,
    Foreign,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Standard {
    Applies,
    Skips,
    /// The remotes name no destination, so the gate reports the setting that decides it.
    Undetermined,
}

/// Whether a repository's commits take the personal standard.
/// The repository's `prose.standard` setting decides when present.
/// Otherwise another remote with a foreign owner marks a fork, `origin` names the destination, and a personal GitHub remote stands in for a missing `origin` or one that names no GitHub owner.
#[must_use]
pub fn repository_standard(
    declared: Option<bool>,
    origin: Option<Owner>,
    other_personal: bool,
    other_foreign: bool,
) -> Standard {
    match (declared, origin) {
        (None, _) if other_foreign => Standard::Skips,
        (Some(true), _) | (None, Some(Owner::Personal)) => Standard::Applies,
        (Some(false), _) | (None, Some(Owner::Foreign)) => Standard::Skips,
        (None, Some(Owner::Unresolved) | None) if other_personal => Standard::Applies,
        (None, Some(Owner::Unresolved) | None) => Standard::Undetermined,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reply {
    Allow,
    Rewrite,
    EndUnchecked,
}

/// A continued turn counts as this hook's rewrite only when this hook marked the session.
/// Another hook's continuation still earns one rewrite request.
/// Without a session to mark, the client's continuation flag decides, which keeps the loop bounded.
#[must_use]
pub fn rewritten_by_this_hook(stop_hook_active: bool, marked: Option<bool>) -> bool {
    stop_hook_active && marked.unwrap_or(true)
}

/// A failing reply gets one rewrite request, and a second failure ends the turn with the findings instead of looping.
#[must_use]
pub fn reply(passed: bool, already_continued: bool) -> Reply {
    if passed {
        Reply::Allow
    } else if already_continued {
        Reply::EndUnchecked
    } else {
        Reply::Rewrite
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const ORIGINS: [Option<Owner>; 4] = [
        Some(Owner::Personal),
        Some(Owner::Foreign),
        Some(Owner::Unresolved),
        None,
    ];

    pub fn spec_standard_applies(personal: bool, fork: bool) {
        assert_eq!(standard_applies(personal, fork), personal && !fork);
    }

    pub fn spec_exemption_valid(reason: bool, used: bool) {
        assert_eq!(exemption_valid(reason, used), reason && used);
    }

    /// A declared standard wins, a foreign remote marks a fork, and only remotes that name no destination leave it undetermined.
    pub fn spec_repository_standard(
        declared: Option<bool>,
        origin: Option<Owner>,
        personal: bool,
        foreign: bool,
    ) {
        let standard = repository_standard(declared, origin, personal, foreign);
        if let Some(declared) = declared {
            assert_eq!(
                standard,
                if declared {
                    Standard::Applies
                } else {
                    Standard::Skips
                }
            );
        } else {
            let unresolved = matches!(origin, Some(Owner::Unresolved) | None);
            assert_eq!(
                standard == Standard::Applies,
                !foreign && (origin == Some(Owner::Personal) || (unresolved && personal))
            );
            assert_eq!(
                standard == Standard::Undetermined,
                unresolved && !personal && !foreign
            );
            assert!(origin != Some(Owner::Foreign) || standard == Standard::Skips);
        }
    }

    /// A failing reply gets one rewrite request, and a continuation this hook marked ends the turn instead.
    pub fn spec_reply(passed: bool, active: bool, marked: Option<bool>) {
        let continued = rewritten_by_this_hook(active, marked);
        assert_eq!(continued, active && marked != Some(false));
        let action = reply(passed, continued);
        assert_eq!(action == Reply::Allow, passed);
        assert_eq!(action == Reply::Rewrite, !passed && !continued);
        assert_eq!(action == Reply::EndUnchecked, !passed && continued);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_personal_non_fork_destinations_take_the_standard_and_exemptions_need_a_reason_and_a_match()
     {
        let first: bool = kani::any();
        let second: bool = kani::any();
        spec_standard_applies(first, second);
        spec_exemption_valid(first, second);
        kani::cover!(standard_applies(first, second));
        kani::cover!(!exemption_valid(first, second) && first);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_declared_standard_wins_and_an_unresolved_destination_is_reported() {
        let declared: Option<bool> = kani::any();
        let origin = ORIGINS[usize::from(kani::any::<u8>() % 4)];
        let personal: bool = kani::any();
        let foreign: bool = kani::any();
        spec_repository_standard(declared, origin, personal, foreign);
        let standard = repository_standard(declared, origin, personal, foreign);
        kani::cover!(standard == Standard::Applies && origin.is_none());
        kani::cover!(standard == Standard::Skips && declared.is_none());
        kani::cover!(standard == Standard::Undetermined);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn failing_replies_are_rewritten_once_and_never_allowed() {
        let passed: bool = kani::any();
        let active: bool = kani::any();
        let marked: Option<bool> = kani::any();
        spec_reply(passed, active, marked);
        let action = reply(passed, rewritten_by_this_hook(active, marked));
        kani::cover!(action == Reply::Allow);
        kani::cover!(action == Reply::Rewrite && active);
        kani::cover!(action == Reply::EndUnchecked);
    }
}

#[cfg(test)]
mod tests {
    use super::proofs;
    use crate::domain::flags;

    #[test]
    fn every_input_meets_the_specifications() {
        let declarations = [None, Some(false), Some(true)];
        for [first, second] in flags() {
            proofs::spec_standard_applies(first, second);
            proofs::spec_exemption_valid(first, second);
            for declared in declarations {
                for origin in proofs::ORIGINS {
                    proofs::spec_repository_standard(declared, origin, first, second);
                }
                proofs::spec_reply(first, second, declared);
            }
        }
    }
}
