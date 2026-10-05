#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ledger {
    Within,
    Exceeded,
    Stale,
}

/// Compares the findings of one rule in one file with the count of its legacy ledger entry.
pub fn ledger(found: u32, allowed: u32) -> Ledger {
    if found > allowed {
        Ledger::Exceeded
    } else if found < allowed {
        Ledger::Stale
    } else {
        Ledger::Within
    }
}

/// The count a ledger entry keeps after a fix: it falls with the findings and never rises.
pub fn tightened(allowed: u32, found: u32) -> u32 {
    allowed.min(found)
}

/// An exemption needs a stated reason, and an exemption that matches nothing fails as stale.
pub fn exemption_valid(reason_present: bool, used: bool) -> bool {
    reason_present && used
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reply {
    Allow,
    Rewrite,
    EndUnchecked,
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
    let continued: bool = kani::any();
    let action = reply(passed, continued);
    assert_eq!(action == Reply::Allow, passed);
    assert!(!continued || action != Reply::Rewrite);
    if action == Reply::Rewrite {
        assert_eq!(reply(false, true), Reply::EndUnchecked);
    }
    kani::cover!(action == Reply::Allow);
    kani::cover!(action == Reply::Rewrite);
    kani::cover!(action == Reply::EndUnchecked);
}
