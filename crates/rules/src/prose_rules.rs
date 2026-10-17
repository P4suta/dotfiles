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

pub const HOOK_PREFIX: &[u8] = b"prose reply --client ";

/// The command prefix that the retired prose installer registered.
#[must_use]
pub fn reply_hook(command: &[u8]) -> bool {
    command.starts_with(HOOK_PREFIX)
}

/// A hook group stays unless removing its reply handlers emptied it.
/// An existing empty group stays unchanged.
#[must_use]
pub fn keep_hook_group(before: usize, after: usize) -> bool {
    before == 0 || after != 0
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

    pub fn spec_hook_group(before: usize, after: usize) {
        assert_eq!(!keep_hook_group(before, after), before != 0 && after == 0);
    }

    pub fn spec_reply_hook(command: &[u8]) {
        assert_eq!(
            reply_hook(command),
            command.get(..HOOK_PREFIX.len()) == Some(HOOK_PREFIX)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(23)]
    fn reply_hook_matching_checks_the_complete_prefix_and_preserves_other_commands() {
        const LENGTH: usize = HOOK_PREFIX.len() + 1;
        let command: [u8; LENGTH] = kani::any();
        let length = usize::from(kani::any::<u8>()) % (LENGTH + 1);
        spec_reply_hook(&command[..length]);
        kani::cover!(reply_hook(&command));
        kani::cover!(!reply_hook(&command));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_personal_non_fork_destinations_take_the_standard_and_exemptions_need_a_reason_and_a_match()
     {
        let first: bool = kani::any();
        let second: bool = kani::any();
        spec_standard_applies(first, second);
        spec_exemption_valid(first, second);
        let applies = standard_applies(first, second);
        let exempt = exemption_valid(first, second);
        kani::cover!(applies);
        kani::cover!(!exempt & first);
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
    fn only_groups_emptied_by_reply_removal_are_dropped() {
        let before: usize = kani::any();
        let after: usize = kani::any();
        spec_hook_group(before, after);
        let keep = keep_hook_group(before, after);
        let has_handlers = keep & (after != 0);
        let originally_empty = keep & (before == 0);
        kani::cover!(has_handlers);
        kani::cover!(originally_empty);
        kani::cover!(!keep);
    }
}

#[cfg(test)]
mod tests {
    use super::{HOOK_PREFIX, proofs, reply_hook};
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
            }
        }
        for before in [0, 1, usize::MAX] {
            for after in [0, 1, usize::MAX] {
                proofs::spec_hook_group(before, after);
            }
        }
        for command in [
            &b"prose reply --client claude"[..],
            &b"prose reply --client codex"[..],
            &b"skill-ops stop"[..],
            &b"prose check --channel document guide.md"[..],
            &b"echo prose reply --client claude"[..],
        ] {
            proofs::spec_reply_hook(command);
        }
        for length in 0..HOOK_PREFIX.len() {
            assert!(!reply_hook(&HOOK_PREFIX[..length]));
        }
        assert!(reply_hook(HOOK_PREFIX));
    }
}
