/// An operating-system family a CI matrix can require and a managed host can provide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Linux,
    Mac,
    Windows,
}

impl Family {
    pub const ALL: [Self; 3] = [Self::Linux, Self::Mac, Self::Windows];

    pub const fn index(self) -> usize {
        match self {
            Self::Linux => 0,
            Self::Mac => 1,
            Self::Windows => 2,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Mac => "macos",
            Self::Windows => "windows",
        }
    }
}

/// What a commit's notes say about one family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    Missing,
    Failed,
    Passed,
}

/// Fold the next record in note order.
/// The newest record for the own tree of the commit decides, and a record for another tree says nothing.
pub const fn record(previous: Evidence, same_tree: bool, passed: bool) -> Evidence {
    match (same_tree, passed) {
        (false, _) => previous,
        (true, true) => Evidence::Passed,
        (true, false) => Evidence::Failed,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Admit,
    Missing(Family),
    Failed(Family),
}

const fn verdict(family: Family, required: bool, evidence: Evidence) -> Verdict {
    match (required, evidence) {
        (true, Evidence::Missing) => Verdict::Missing(family),
        (true, Evidence::Failed) => Verdict::Failed(family),
        _ => Verdict::Admit,
    }
}

/// Admit only when every required family passed.
/// Otherwise name the first one to check.
pub const fn admit(required: [bool; 3], evidence: [Evidence; 3]) -> Verdict {
    let linux = verdict(Family::Linux, required[0], evidence[0]);
    if !matches!(linux, Verdict::Admit) {
        return linux;
    }
    let mac = verdict(Family::Mac, required[1], evidence[1]);
    if !matches!(mac, Verdict::Admit) {
        return mac;
    }
    verdict(Family::Windows, required[2], evidence[2])
}

/// The gate covers only a branch update on GitHub.
/// It skips deletions, tags, and pushes to the hubs of the owner.
pub const fn gated(github: bool, branch: bool, deletion: bool) -> bool {
    github && branch && !deletion
}

/// How a scanned host key relates to the pinned one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    New,
    Same,
    Changed,
}

/// A changed key replaces the pinned one only when the owner accepted that host.
pub const fn pin(key: Key, accepted: bool) -> bool {
    match key {
        Key::New | Key::Same => true,
        Key::Changed => accepted,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    Dirty,
    Unready(Family),
    Run,
}

/// Gates run only for a clean tree, and the first required host that lacks readiness stops the check.
pub const fn start(clean: bool, required: [bool; 3], ready: [bool; 3]) -> Start {
    if !clean {
        Start::Dirty
    } else if required[0] && !ready[0] {
        Start::Unready(Family::Linux)
    } else if required[1] && !ready[1] {
        Start::Unready(Family::Mac)
    } else if required[2] && !ready[2] {
        Start::Unready(Family::Windows)
    } else {
        Start::Run
    }
}

#[cfg(kani)]
fn any_evidence() -> Evidence {
    match kani::any::<u8>() % 3 {
        0 => Evidence::Missing,
        1 => Evidence::Failed,
        _ => Evidence::Passed,
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_push_is_admitted_only_when_every_required_family_passed() {
    let required: [bool; 3] = [kani::any(), kani::any(), kani::any()];
    let evidence = [any_evidence(), any_evidence(), any_evidence()];
    let verdict = admit(required, evidence);
    let complete = (!required[0] || evidence[0] == Evidence::Passed)
        && (!required[1] || evidence[1] == Evidence::Passed)
        && (!required[2] || evidence[2] == Evidence::Passed);
    assert_eq!(verdict == Verdict::Admit, complete);
    let gap = [
        required[0] && evidence[0] != Evidence::Passed,
        required[1] && evidence[1] != Evidence::Passed,
        required[2] && evidence[2] != Evidence::Passed,
    ];
    assert_eq!(
        verdict == Verdict::Missing(Family::Linux) || verdict == Verdict::Failed(Family::Linux),
        gap[0]
    );
    assert_eq!(
        verdict == Verdict::Missing(Family::Mac) || verdict == Verdict::Failed(Family::Mac),
        !gap[0] && gap[1]
    );
    assert_eq!(
        verdict == Verdict::Missing(Family::Windows) || verdict == Verdict::Failed(Family::Windows),
        !gap[0] && !gap[1] && gap[2]
    );
    assert!(verdict != Verdict::Missing(Family::Linux) || evidence[0] == Evidence::Missing);
    assert!(verdict != Verdict::Missing(Family::Mac) || evidence[1] == Evidence::Missing);
    assert!(verdict != Verdict::Missing(Family::Windows) || evidence[2] == Evidence::Missing);
    kani::cover!(verdict == Verdict::Admit && required[0] && required[1] && required[2]);
    kani::cover!(verdict == Verdict::Failed(Family::Mac));
    kani::cover!(verdict == Verdict::Missing(Family::Windows));
}

#[cfg(kani)]
#[kani::proof]
fn the_latest_record_for_the_commit_tree_decides_a_family() {
    let first_tree: bool = kani::any();
    let first_passed: bool = kani::any();
    let second_tree: bool = kani::any();
    let second_passed: bool = kani::any();
    let evidence = record(
        record(Evidence::Missing, first_tree, first_passed),
        second_tree,
        second_passed,
    );
    let expected = if second_tree {
        if second_passed {
            Evidence::Passed
        } else {
            Evidence::Failed
        }
    } else if first_tree {
        if first_passed {
            Evidence::Passed
        } else {
            Evidence::Failed
        }
    } else {
        Evidence::Missing
    };
    assert_eq!(evidence, expected);
    assert!(
        evidence != Evidence::Passed
            || (first_tree && first_passed)
            || (second_tree && second_passed)
    );
    kani::cover!(first_tree && first_passed && second_tree && evidence == Evidence::Failed);
    kani::cover!(!first_tree && first_passed && !second_tree && evidence == Evidence::Missing);
    kani::cover!(evidence == Evidence::Passed);
}

#[cfg(kani)]
#[kani::proof]
fn only_github_branch_updates_are_gated() {
    let github: bool = kani::any();
    let branch: bool = kani::any();
    let deletion: bool = kani::any();
    let result = gated(github, branch, deletion);
    assert_eq!(result, github && branch && !deletion);
    kani::cover!(result);
    kani::cover!(!result && github && branch);
}

#[cfg(kani)]
#[kani::proof]
fn a_changed_host_key_is_pinned_only_with_acceptance() {
    let key = match kani::any::<u8>() % 3 {
        0 => Key::New,
        1 => Key::Same,
        _ => Key::Changed,
    };
    let accepted: bool = kani::any();
    let result = pin(key, accepted);
    assert!(key != Key::Changed || result == accepted);
    assert!(key == Key::Changed || result);
    kani::cover!(key == Key::Changed && !result);
    kani::cover!(key == Key::Changed && result);
}

#[cfg(kani)]
#[kani::proof]
fn checks_start_only_from_a_clean_tree_with_every_required_host_ready() {
    let clean: bool = kani::any();
    let required: [bool; 3] = [kani::any(), kani::any(), kani::any()];
    let ready: [bool; 3] = [kani::any(), kani::any(), kani::any()];
    let result = start(clean, required, ready);
    let all_ready =
        (!required[0] || ready[0]) && (!required[1] || ready[1]) && (!required[2] || ready[2]);
    assert_eq!(result == Start::Run, clean && all_ready);
    let gap = [
        required[0] && !ready[0],
        required[1] && !ready[1],
        required[2] && !ready[2],
    ];
    assert_eq!(result == Start::Unready(Family::Linux), clean && gap[0]);
    assert_eq!(
        result == Start::Unready(Family::Mac),
        clean && !gap[0] && gap[1]
    );
    assert_eq!(
        result == Start::Unready(Family::Windows),
        clean && !gap[0] && !gap[1] && gap[2]
    );
    kani::cover!(result == Start::Run && required[0] && required[1] && required[2]);
    kani::cover!(result == Start::Unready(Family::Linux));
    kani::cover!(result == Start::Dirty);
}
