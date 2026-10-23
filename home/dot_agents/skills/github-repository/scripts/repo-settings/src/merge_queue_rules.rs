//! Exempt as disproportionate: this standalone skill checks all sixteen inputs of its four-boolean decision by enumeration instead of joining the workspace-only proof registry.
//! Evidence: `home/dot_agents/skills/github-repository/scripts/repo-settings/src/merge_queue_rules.rs::tests::every_input_has_the_expected_queue_decision`.
//! Reconsider when the decision gains another input or a larger domain.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Unavailable,
    NoChecks,
    Blocked,
    Present,
    Add,
}

pub fn decide(
    organization_owned: bool,
    required_checks: bool,
    workflows_ready: bool,
    queue_present: bool,
) -> Decision {
    match (
        organization_owned,
        required_checks,
        workflows_ready,
        queue_present,
    ) {
        (false, _, _, _) => Decision::Unavailable,
        (true, false, _, _) => Decision::NoChecks,
        (true, true, false, _) => Decision::Blocked,
        (true, true, true, true) => Decision::Present,
        (true, true, true, false) => Decision::Add,
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, decide};

    #[test]
    fn every_input_has_the_expected_queue_decision() {
        for ((organization, checks, ready, present), expected) in [
            ((false, false, false, false), Decision::Unavailable),
            ((false, false, false, true), Decision::Unavailable),
            ((false, false, true, false), Decision::Unavailable),
            ((false, false, true, true), Decision::Unavailable),
            ((false, true, false, false), Decision::Unavailable),
            ((false, true, false, true), Decision::Unavailable),
            ((false, true, true, false), Decision::Unavailable),
            ((false, true, true, true), Decision::Unavailable),
            ((true, false, false, false), Decision::NoChecks),
            ((true, false, false, true), Decision::NoChecks),
            ((true, false, true, false), Decision::NoChecks),
            ((true, false, true, true), Decision::NoChecks),
            ((true, true, false, false), Decision::Blocked),
            ((true, true, false, true), Decision::Blocked),
            ((true, true, true, false), Decision::Add),
            ((true, true, true, true), Decision::Present),
        ] {
            assert_eq!(
                decide(organization, checks, ready, present),
                expected,
                "{organization:?}, {checks:?}, {ready:?}, {present:?}"
            );
        }
    }
}
