/// An operating-system family a CI matrix can require and a managed host can provide.
/// Each variant's value gives the family's position in a per-family array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Family {
    Linux = 0,
    Mac = 1,
    Windows = 2,
}

impl Family {
    pub const ALL: [Self; 3] = [Self::Linux, Self::Mac, Self::Windows];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
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

/// Admit only an installed active version matching the checkout's exact pin.
#[must_use]
pub const fn pinned_tool(facts: [bool; 4]) -> bool {
    let [installed, active, source_matches, version_matches] = facts;
    installed & active & source_matches & version_matches
}

/// Fold the next record in note order.
/// The newest record for the own tree of the commit decides, and a record for another tree says nothing.
#[must_use]
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
#[must_use]
pub const fn admit(required: [bool; 3], evidence: [Evidence; 3]) -> Verdict {
    let [linux_required, mac_required, windows_required] = required;
    let [linux_evidence, mac_evidence, windows_evidence] = evidence;
    let linux = verdict(Family::Linux, linux_required, linux_evidence);
    if !matches!(linux, Verdict::Admit) {
        return linux;
    }
    let mac = verdict(Family::Mac, mac_required, mac_evidence);
    if !matches!(mac, Verdict::Admit) {
        return mac;
    }
    verdict(Family::Windows, windows_required, windows_evidence)
}

/// The gate covers only a branch update on GitHub.
/// It skips deletions, tags, and pushes to the hubs of the owner.
#[must_use]
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
#[must_use]
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
#[must_use]
pub const fn start(clean: bool, required: [bool; 3], ready: [bool; 3]) -> Start {
    let [linux_required, mac_required, windows_required] = required;
    let [linux_ready, mac_ready, windows_ready] = ready;
    if !clean {
        Start::Dirty
    } else if linux_required && !linux_ready {
        Start::Unready(Family::Linux)
    } else if mac_required && !mac_ready {
        Start::Unready(Family::Mac)
    } else if windows_required && !windows_ready {
        Start::Unready(Family::Windows)
    } else {
        Start::Run
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const EVIDENCE: [Evidence; 3] = [Evidence::Missing, Evidence::Failed, Evidence::Passed];
    pub const KEYS: [Key; 3] = [Key::New, Key::Same, Key::Changed];

    pub fn spec_pinned_tool(facts: [bool; 4]) {
        assert_eq!(pinned_tool(facts), facts.iter().all(|fact| *fact));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_an_installed_active_project_pin_is_usable() {
        let facts: [bool; 4] = kani::any();
        spec_pinned_tool(facts);
        let [installed, active, source, version] = facts;
        let result = pinned_tool(facts);
        kani::cover!(result);
        kani::cover!(!result & installed & active & !source & version);
    }

    /// The families from last to first, so the gap a reverse walk records last matches the one a check names first.
    const LAST_FIRST: [Family; 3] = [Family::Windows, Family::Mac, Family::Linux];

    /// A push passes only when every required family passed, and otherwise names the first gap with what its notes say.
    pub fn spec_admit(required: [bool; 3], evidence: [Evidence; 3]) {
        let result = admit(required, evidence);
        let mut expected = Verdict::Admit;
        let mut position = 0;
        while let Some(&family) = LAST_FIRST.get(position) {
            let index = family.index();
            let needed = required.get(index).copied().unwrap_or(false);
            match evidence.get(index) {
                Some(Evidence::Missing) if needed => expected = Verdict::Missing(family),
                Some(Evidence::Failed) if needed => expected = Verdict::Failed(family),
                _ => {}
            }
            position += 1;
        }
        assert_eq!(result, expected);
    }

    /// The newest record for the commit's own tree decides a family.
    pub fn spec_record([first_tree, first_passed, second_tree, second_passed]: [bool; 4]) {
        let evidence = record(
            record(Evidence::Missing, first_tree, first_passed),
            second_tree,
            second_passed,
        );
        let outcome = |passed| {
            if passed {
                Evidence::Passed
            } else {
                Evidence::Failed
            }
        };
        let expected = if second_tree {
            outcome(second_passed)
        } else if first_tree {
            outcome(first_passed)
        } else {
            Evidence::Missing
        };
        assert_eq!(evidence, expected);
    }

    pub fn spec_gated(github: bool, branch: bool, deletion: bool) {
        assert_eq!(
            gated(github, branch, deletion),
            github && branch && !deletion
        );
    }

    pub fn spec_pin(key: Key, accepted: bool) {
        assert_eq!(pin(key, accepted), key != Key::Changed || accepted);
    }

    /// Checks start only from a clean tree with every required host ready, and otherwise name the first unready one.
    pub fn spec_start(clean: bool, required: [bool; 3], ready: [bool; 3]) {
        let result = start(clean, required, ready);
        let mut expected = if clean { Start::Run } else { Start::Dirty };
        let mut position = 0;
        while let Some(&family) = LAST_FIRST.get(position) {
            let index = family.index();
            if clean && required.get(index) == Some(&true) && ready.get(index) == Some(&false) {
                expected = Start::Unready(family);
            }
            position += 1;
        }
        assert_eq!(result, expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_push_is_admitted_only_when_every_required_family_passed() {
        let required: [bool; 3] = [kani::any(), kani::any(), kani::any()];
        let evidence = [
            EVIDENCE[usize::from(kani::any::<u8>() % 3)],
            EVIDENCE[usize::from(kani::any::<u8>() % 3)],
            EVIDENCE[usize::from(kani::any::<u8>() % 3)],
        ];
        spec_admit(required, evidence);
        let result = admit(required, evidence);
        let passed = matches!(result, Verdict::Admit) & required[0] & required[1] & required[2];
        let failed = matches!(result, Verdict::Failed(Family::Mac));
        let missing = matches!(result, Verdict::Missing(Family::Windows));
        kani::cover!(passed);
        kani::cover!(failed);
        kani::cover!(missing);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn the_latest_record_for_the_commit_tree_decides_a_family() {
        let flags: [bool; 4] = [kani::any(), kani::any(), kani::any(), kani::any()];
        let [first_tree, first_passed, second_tree, second_passed] = flags;
        spec_record(flags);
        let evidence = record(
            record(Evidence::Missing, first_tree, first_passed),
            second_tree,
            second_passed,
        );
        kani::cover!(first_tree & first_passed & second_tree & (evidence == Evidence::Failed));
        kani::cover!(!first_tree & first_passed & !second_tree & (evidence == Evidence::Missing));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_github_branch_updates_are_gated_and_a_changed_key_needs_acceptance() {
        let [github, branch, deletion]: [bool; 3] = [kani::any(), kani::any(), kani::any()];
        let key = KEYS[usize::from(kani::any::<u8>() % 3)];
        spec_gated(github, branch, deletion);
        spec_pin(key, deletion);
        kani::cover!(gated(github, branch, deletion));
        kani::cover!((key == Key::Changed) & !pin(key, deletion));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn checks_start_only_from_a_clean_tree_with_every_required_host_ready() {
        let clean: bool = kani::any();
        let required: [bool; 3] = [kani::any(), kani::any(), kani::any()];
        let ready: [bool; 3] = [kani::any(), kani::any(), kani::any()];
        spec_start(clean, required, ready);
        let result = start(clean, required, ready);
        kani::cover!((result == Start::Run) & required[0] & required[1] & required[2]);
        kani::cover!(result == Start::Unready(Family::Windows));
        kani::cover!(result == Start::Dirty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flags;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every input")]
    fn every_input_meets_the_specifications() {
        for required in flags() {
            for linux in proofs::EVIDENCE {
                for mac in proofs::EVIDENCE {
                    for windows in proofs::EVIDENCE {
                        proofs::spec_admit(required, [linux, mac, windows]);
                    }
                }
            }
            for ready in flags() {
                proofs::spec_start(false, required, ready);
                proofs::spec_start(true, required, ready);
            }
        }
        for records in flags() {
            proofs::spec_record(records);
            proofs::spec_pinned_tool(records);
        }
        for [github, branch, deletion] in flags() {
            proofs::spec_gated(github, branch, deletion);
            for key in proofs::KEYS {
                proofs::spec_pin(key, deletion);
            }
        }
    }

    #[test]
    fn each_family_has_its_own_index_and_name() {
        assert_eq!(Family::ALL.map(Family::index), [0, 1, 2]);
        assert_eq!(Family::ALL.map(Family::name), ["linux", "macos", "windows"]);
    }
}
