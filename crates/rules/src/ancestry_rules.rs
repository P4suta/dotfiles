//! How `git merge-base --is-ancestor` answers whether one commit contains another.

/// The exit status when the first commit is an ancestor of the second.
const ANCESTOR: i32 = 0;
/// The exit status when the first commit isn't an ancestor of the second.
const NOT_ANCESTOR: i32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ancestry {
    Ancestor,
    NotAncestor,
    /// Any other status, or a process that a signal ended, means Git couldn't compare the commits.
    Failed,
}

/// Classifies the exit status of `git merge-base --is-ancestor`, which is `None` when a signal ended Git.
#[must_use]
pub const fn ancestry(status: Option<i32>) -> Ancestry {
    match status {
        Some(ANCESTOR) => Ancestry::Ancestor,
        Some(NOT_ANCESTOR) => Ancestry::NotAncestor,
        Some(_) | None => Ancestry::Failed,
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_ancestry(status: Option<i32>) {
        let answer = ancestry(status);
        assert_eq!(answer == Ancestry::Ancestor, status == Some(ANCESTOR));
        assert_eq!(
            answer == Ancestry::NotAncestor,
            status == Some(NOT_ANCESTOR)
        );
        assert_eq!(
            answer == Ancestry::Failed,
            status != Some(ANCESTOR) && status != Some(NOT_ANCESTOR)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_the_two_documented_statuses_answer_the_ancestry_question() {
        let status: Option<i32> = kani::any();
        spec_ancestry(status);
        let answer = ancestry(status);
        kani::cover!(answer == Ancestry::Ancestor);
        kani::cover!(answer == Ancestry::NotAncestor);
        kani::cover!(answer == Ancestry::Failed && status.is_some());
        kani::cover!(answer == Ancestry::Failed && status.is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_boundary_status_meets_the_specification() {
        for status in [
            None,
            Some(i32::MIN),
            Some(-1),
            Some(0),
            Some(1),
            Some(2),
            Some(128),
            Some(i32::MAX),
        ] {
            proofs::spec_ancestry(status);
        }
    }

    #[test]
    fn a_missing_ancestor_is_an_answer_and_a_bad_object_is_a_failure() {
        assert_eq!(ancestry(Some(0)), Ancestry::Ancestor);
        assert_eq!(ancestry(Some(1)), Ancestry::NotAncestor);
        assert_eq!(ancestry(Some(128)), Ancestry::Failed);
        assert_eq!(ancestry(None), Ancestry::Failed);
    }
}
