#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ledger {
    Within,
    Exceeded,
    Stale,
}

/// Compares how often one finding occurs with how often its legacy ledger entry records it.
pub fn ledger(found: u32, allowed: u32) -> Ledger {
    if found > allowed {
        Ledger::Exceeded
    } else if found < allowed {
        Ledger::Stale
    } else {
        Ledger::Within
    }
}

/// How often `key` occurs in `items`, which counts one fingerprint in a multiset of findings or ledger entries.
pub fn occurrences<T: PartialEq>(items: &[T], key: &T) -> u32 {
    let mut count: u32 = 0;
    let mut index = 0;
    while index < items.len() {
        if items[index] == *key {
            count += 1;
        }
        index += 1;
    }
    count
}

/// The count a ledger entry keeps after a fix: it falls with the findings and never rises.
pub fn tightened(allowed: u32, found: u32) -> u32 {
    allowed.min(found)
}

/// An exemption needs a stated reason, and an exemption that matches nothing fails as stale.
pub fn exemption_valid(reason_present: bool, used: bool) -> bool {
    reason_present && used
}

/// The personal writing standard governs a destination that a personal owner holds, unless that destination forks another repository.
/// Other destinations follow their own rules.
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
pub fn repository_standard(
    declared: Option<bool>,
    origin: Option<Owner>,
    other_personal: bool,
    other_foreign: bool,
) -> Standard {
    match (declared, origin) {
        (Some(true), _) => Standard::Applies,
        (Some(false), _) => Standard::Skips,
        (None, _) if other_foreign => Standard::Skips,
        (None, Some(Owner::Personal)) => Standard::Applies,
        (None, Some(Owner::Foreign)) => Standard::Skips,
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
pub fn rewritten_by_this_hook(stop_hook_active: bool, marked: Option<bool>) -> bool {
    stop_hook_active && marked.unwrap_or(true)
}

/// A failing reply gets one rewrite request, and a second failure ends the turn with the findings instead of looping.
pub fn reply(passed: bool, already_continued: bool) -> Reply {
    if passed {
        Reply::Allow
    } else if already_continued {
        Reply::EndUnchecked
    } else {
        Reply::Rewrite
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_ledger_never_admits_more_findings_than_it_allows() {
    let found: u32 = kani::any();
    let allowed: u32 = kani::any();
    let verdict = ledger(found, allowed);
    assert_eq!(verdict == Ledger::Within, found == allowed);
    assert!(verdict != Ledger::Exceeded || found > allowed);
    assert!(verdict != Ledger::Within || found <= allowed);
    kani::cover!(verdict == Ledger::Within);
    kani::cover!(verdict == Ledger::Exceeded);
    kani::cover!(verdict == Ledger::Stale);
}

#[cfg(kani)]
#[kani::proof]
fn a_replaced_sentence_is_never_admitted_by_the_ledger() {
    let found: [u8; 2] = kani::any();
    let allowed: [u8; 2] = kani::any();
    let key: u8 = kani::any();
    let verdict = ledger(occurrences(&found, &key), occurrences(&allowed, &key));
    let in_found = found[0] == key || found[1] == key;
    let in_allowed = allowed[0] == key || allowed[1] == key;
    if in_found && !in_allowed {
        assert_eq!(verdict, Ledger::Exceeded);
    }
    if !in_found && in_allowed {
        assert_eq!(verdict, Ledger::Stale);
    }
    if verdict == Ledger::Within {
        assert_eq!(occurrences(&found, &key), occurrences(&allowed, &key));
    }
    assert!(occurrences(&found, &key) <= 2);
    kani::cover!(verdict == Ledger::Within && in_found);
    kani::cover!(verdict == Ledger::Exceeded);
    kani::cover!(verdict == Ledger::Stale);
}

#[cfg(kani)]
#[kani::proof]
fn only_personal_non_fork_destinations_take_the_standard() {
    let personal: bool = kani::any();
    let fork: bool = kani::any();
    let applies = standard_applies(personal, fork);
    assert!(!applies || personal);
    assert!(!applies || !fork);
    assert_eq!(applies, personal && !fork);
    kani::cover!(applies);
    kani::cover!(!applies && personal);
}

#[cfg(kani)]
fn any_owner() -> Option<Owner> {
    let choice: u8 = kani::any();
    kani::assume(choice < 4);
    match choice {
        0 => Some(Owner::Personal),
        1 => Some(Owner::Foreign),
        2 => Some(Owner::Unresolved),
        _ => None,
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_declared_standard_wins_and_an_unresolved_destination_is_reported() {
    let declared: Option<bool> = kani::any();
    let origin = any_owner();
    let personal: bool = kani::any();
    let foreign: bool = kani::any();
    let standard = repository_standard(declared, origin, personal, foreign);
    if let Some(declared) = declared {
        assert_eq!(standard == Standard::Applies, declared);
        assert!(standard != Standard::Undetermined);
    } else {
        assert!(standard != Standard::Applies || !foreign);
        assert!(origin != Some(Owner::Foreign) || standard == Standard::Skips);
        assert_eq!(
            standard == Standard::Undetermined,
            matches!(origin, Some(Owner::Unresolved) | None) && !personal && !foreign
        );
        if origin != Some(Owner::Foreign) && personal && !foreign {
            assert_eq!(standard, Standard::Applies);
        }
        if origin == Some(Owner::Personal) {
            assert_eq!(
                standard == Standard::Applies,
                standard_applies(true, foreign)
            );
        }
    }
    kani::cover!(standard == Standard::Applies && declared.is_none());
    kani::cover!(standard == Standard::Skips && declared.is_none());
    kani::cover!(standard == Standard::Undetermined);
    kani::cover!(standard == Standard::Applies && origin.is_none());
    kani::cover!(standard == Standard::Undetermined && origin.is_none());
}

#[cfg(kani)]
#[kani::proof]
fn tightening_never_raises_an_allowance() {
    let allowed: u32 = kani::any();
    let found: u32 = kani::any();
    let next = tightened(allowed, found);
    assert!(next <= allowed);
    assert!(next <= found);
    assert!(next == allowed || next == found);
    assert_eq!(ledger(found, next) == Ledger::Exceeded, found > allowed);
    kani::cover!(next < allowed);
    kani::cover!(next == allowed);
}

#[cfg(kani)]
#[kani::proof]
fn exemptions_require_a_reason_and_a_match() {
    let reason: bool = kani::any();
    let used: bool = kani::any();
    let valid = exemption_valid(reason, used);
    assert_eq!(valid, reason && used);
    kani::cover!(valid);
    kani::cover!(!valid);
}

#[cfg(kani)]
#[kani::proof]
fn failing_replies_are_rewritten_once_and_never_allowed() {
    let passed: bool = kani::any();
    let active: bool = kani::any();
    let marked: Option<bool> = kani::any();
    let continued = rewritten_by_this_hook(active, marked);
    assert!(!continued || active);
    assert!(continued || !active || marked == Some(false));
    let action = reply(passed, continued);
    assert_eq!(action == Reply::Allow, passed);
    assert!(!continued || action != Reply::Rewrite);
    if action == Reply::Rewrite {
        assert_eq!(
            reply(false, rewritten_by_this_hook(true, Some(true))),
            Reply::EndUnchecked
        );
    }
    if !passed && marked == Some(false) {
        assert_eq!(action, Reply::Rewrite);
    }
    kani::cover!(action == Reply::Allow);
    kani::cover!(action == Reply::Rewrite);
    kani::cover!(action == Reply::EndUnchecked);
}
