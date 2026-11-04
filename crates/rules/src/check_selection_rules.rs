//! Which check runs on one head a newer run of the same check replaces.
//!
//! GitHub keeps every check suite of a head commit in its rollup.
//! A cancelled duplicate run or a failed attempt before a rerun then stays beside the result that replaced it.
//! Only a newer run with the same app, workflow, event, and check name supersedes a run.
//! A run without that identity supersedes nothing, nothing supersedes it, and its result stays counted.

use alloc::string::String;
use alloc::vec::Vec;

/// The provider identity of a check suite: its app, and for a workflow its workflow and triggering event.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identity {
    pub app: u64,
    pub workflow: Option<u64>,
    pub event: Option<String>,
}

/// One check run of a head with the order of its run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// `None` when the provider gave no complete identity for the run's suite.
    pub identity: Option<Identity>,
    pub name: String,
    /// The workflow run for a workflow suite, or the suite itself for another app, where a later run has a greater number.
    pub order: u64,
    /// The check run, which a rerun attempt of the same workflow run replaces with a greater number.
    pub id: u64,
}

impl Run {
    fn key(&self) -> (u64, u64) {
        (self.order, self.id)
    }
}

/// Whether `run` has a newer run of the same identity and check name among `runs`.
#[must_use]
pub fn superseded(run: &Run, runs: &[Run]) -> bool {
    let Some(identity) = &run.identity else {
        return false;
    };
    runs.iter().any(|other| {
        other.identity.as_ref() == Some(identity)
            && other.name == run.name
            && other.key() > run.key()
    })
}

/// The check run numbers among `runs` that a newer run of the same check replaces.
#[must_use]
pub fn superseded_ids(runs: &[Run]) -> Vec<u64> {
    runs.iter()
        .filter(|run| superseded(run, runs))
        .map(|run| run.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Identity, Run, superseded, superseded_ids};
    use alloc::vec;
    use alloc::vec::Vec;

    fn identity(workflow: u64, event: &str) -> Identity {
        Identity {
            app: 15368,
            workflow: Some(workflow),
            event: Some(event.into()),
        }
    }

    fn run(identity: Option<Identity>, name: &str, order: u64, id: u64) -> Run {
        Run {
            identity,
            name: name.into(),
            order,
            id,
        }
    }

    #[test]
    fn a_newer_run_of_the_same_check_replaces_a_cancelled_duplicate() {
        let runs = [
            run(Some(identity(1, "pull_request")), "Checks", 100, 10),
            run(Some(identity(1, "pull_request")), "Checks", 101, 11),
        ];
        assert_eq!(superseded_ids(&runs), vec![10]);
    }

    #[test]
    fn a_rerun_attempt_replaces_its_earlier_attempt() {
        let runs = [
            run(Some(identity(1, "pull_request")), "Checks", 100, 10),
            run(Some(identity(1, "pull_request")), "Checks", 100, 12),
        ];
        assert_eq!(superseded_ids(&runs), vec![10]);
    }

    #[test]
    fn another_workflow_event_app_or_check_never_replaces_a_run() {
        let older = run(Some(identity(1, "pull_request")), "Checks", 100, 10);
        for newer in [
            run(Some(identity(2, "pull_request")), "Checks", 101, 11),
            run(Some(identity(1, "push")), "Checks", 101, 11),
            run(
                Some(Identity {
                    app: 1,
                    workflow: Some(1),
                    event: Some("pull_request".into()),
                }),
                "Checks",
                101,
                11,
            ),
            run(Some(identity(1, "pull_request")), "Rust", 101, 11),
            run(None, "Checks", 101, 11),
        ] {
            assert!(
                !superseded(&older, &[older.clone(), newer.clone()]),
                "{newer:?}"
            );
        }
    }

    #[test]
    fn the_newest_run_and_a_run_without_identity_stay_counted() {
        let runs = [
            run(None, "Checks", 100, 10),
            run(None, "Checks", 101, 11),
            run(Some(identity(1, "pull_request")), "Checks", 101, 12),
        ];
        assert_eq!(superseded_ids(&runs), Vec::<u64>::new());
    }
}

#[cfg(kani)]
mod proofs {
    use super::{Identity, Run, superseded};

    fn any_run() -> Run {
        let identified: bool = kani::any();
        Run {
            identity: identified.then(|| Identity {
                app: kani::any::<u8>().into(),
                workflow: Some(kani::any::<u8>().into()),
                event: None,
            }),
            name: if kani::any() { "a".into() } else { "b".into() },
            order: kani::any::<u8>().into(),
            id: kani::any::<u8>().into(),
        }
    }

    /// A superseded run always has a newer run of the same identity and check name, so no other check or provider hides its result.
    #[kani::proof]
    #[kani::unwind(4)]
    fn a_superseded_run_has_a_newer_run_of_the_same_check() {
        let runs = [any_run(), any_run(), any_run()];
        let index: usize = kani::any();
        kani::assume(index < runs.len());
        let run = &runs[index];
        if superseded(run, &runs) {
            assert!(run.identity.is_some());
            assert!(runs.iter().any(|other| other.identity == run.identity
                && other.name == run.name
                && (other.order, other.id) > (run.order, run.id)));
        }
    }

    /// The newest run of each identity and check name is never superseded, so the current result always stays counted.
    #[kani::proof]
    #[kani::unwind(4)]
    fn the_newest_run_of_a_check_is_never_superseded() {
        let runs = [any_run(), any_run(), any_run()];
        let index: usize = kani::any();
        kani::assume(index < runs.len());
        let run = &runs[index];
        kani::assume(runs.iter().all(|other| {
            other.identity != run.identity
                || other.name != run.name
                || (other.order, other.id) <= (run.order, run.id)
        }));
        assert!(!superseded(run, &runs));
    }
}
