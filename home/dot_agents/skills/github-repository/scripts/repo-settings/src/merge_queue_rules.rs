//! Exempt as disproportionate: this standalone skill checks all eight inputs of its three-boolean decision by enumeration instead of joining the workspace-only proof registry.
//! Evidence: `home/dot_agents/skills/github-repository/scripts/repo-settings/src/merge_queue_rules.rs::tests::every_input_has_the_expected_queue_decision`.
//! Reconsider when the decision gains another input or a larger domain.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    NoChecks,
    Blocked,
    Present,
    Add,
}

pub fn decide(required_checks: bool, workflows_ready: bool, queue_present: bool) -> Decision {
    match (required_checks, workflows_ready, queue_present) {
        (false, _, _) => Decision::NoChecks,
        (true, false, _) => Decision::Blocked,
        (true, true, true) => Decision::Present,
        (true, true, false) => Decision::Add,
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, decide};

    #[test]
    fn every_input_has_the_expected_queue_decision() {
        for ((checks, ready, present), expected) in [
            ((false, false, false), Decision::NoChecks),
            ((false, false, true), Decision::NoChecks),
            ((false, true, false), Decision::NoChecks),
            ((false, true, true), Decision::NoChecks),
            ((true, false, false), Decision::Blocked),
            ((true, false, true), Decision::Blocked),
            ((true, true, false), Decision::Add),
            ((true, true, true), Decision::Present),
        ] {
            assert_eq!(
                decide(checks, ready, present),
                expected,
                "{checks:?}, {ready:?}, {present:?}"
            );
        }
    }
}
