//! What the post-commit signing gate does with the commit Git just recorded, and how it rolls back a commit it can't sign.
//! A rollback returns `HEAD` to where it stood before the commit: the parent, the commit an amend replaced, or no `HEAD` for a root commit.
//! An unsigned merge also gets its second parent back as `MERGE_HEAD`, which Git removed before the hook ran.

/// What the hook does, decided from the cheap facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    /// Nothing to do: re-entrant, mid-operation, or already signed.
    Done,
    /// `HEAD` lacks a signature, and this hook owns the fix.
    Resign,
}

/// The hook leaves alone a commit its own amend made, one that Git makes mid-operation, and one already signed.
#[must_use]
pub const fn decide(in_hook: bool, mid_operation: bool, head_signed: bool) -> Decision {
    if in_hook || mid_operation || head_signed {
        Decision::Done
    } else {
        Decision::Resign
    }
}

/// Whether the hook refuses an ordinary commit that records no change, because its changes most likely vanished from the index.
#[must_use]
pub const fn refuses_empty(settled: bool, empty: bool, waived: bool) -> bool {
    settled && empty && !waived
}

/// How the commit came into existence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shape {
    /// The first commit of a branch, with no parent.
    Root,
    Single,
    Merge,
    /// `git commit --amend`, which replaced the commit `HEAD` named before.
    Amend,
}

/// The shape of a commit from its parent count and whether its reflog entry records an amend.
#[must_use]
pub const fn shape(parents: usize, amended: bool) -> Shape {
    if amended {
        Shape::Amend
    } else {
        match parents {
            0 => Shape::Root,
            1 => Shape::Single,
            _ => Shape::Merge,
        }
    }
}

/// What the hook's own signed amend achieved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// The amend succeeded and `HEAD` verifies.
    Signed,
    /// The amend failed and left `HEAD` untouched.
    Failed,
    /// The amend moved `HEAD`, but the new commit doesn't verify.
    Unverified,
}

/// The gate as it runs now, or as it ran when it always stepped back to the first parent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gate {
    Fixed,
    Old,
}

/// Where a rollback moves `HEAD`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// The commit carries a signature and stays.
    Keep,
    /// A root commit leaves no `HEAD`.
    Unborn,
    FirstParent,
    /// The commit that the amend replaced.
    Original,
}

/// One step of a rollback, each naming an absolute commit, so running the steps again from any point ends in the same place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    /// `git reset --soft <commit>`, which a partial amend may have moved `HEAD` away from.
    ResetToCommit,
    ResetToFirstParent,
    ResetToOriginal,
    /// `git update-ref -d HEAD`.
    DeleteRef,
    /// `git update-ref MERGE_HEAD <second parent>`, so the next commit counts as a merge again.
    RestoreMergeParent,
}

/// The most steps a rollback takes.
pub const MOST_STEPS: usize = 3;

/// The steps of one rollback in the order they run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rollback {
    pub target: Target,
    steps: [Step; MOST_STEPS],
    len: usize,
}

impl Rollback {
    /// The steps in the order they run.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        self.steps.get(..self.len).unwrap_or_default()
    }
}

/// The steps that reset one ref, and the merge rollback that also restores the second parent.
const REF_STEPS: usize = 2;
const MERGE_STEPS: usize = 3;

/// How the gate rolls back a commit of `shape` after its own amend ended in `outcome`.
#[must_use]
pub const fn rollback_target(gate: Gate, shape: Shape, outcome: Outcome) -> Rollback {
    let target = match (outcome, gate, shape) {
        (Outcome::Signed, _, _) => Target::Keep,
        (_, Gate::Fixed, Shape::Amend) => Target::Original,
        (_, _, Shape::Root) => Target::Unborn,
        (_, Gate::Old, Shape::Amend) | (_, _, Shape::Single | Shape::Merge) => Target::FirstParent,
    };
    let filler = Step::ResetToCommit;
    let (steps, len) = match target {
        Target::Keep => ([filler; MOST_STEPS], 0),
        Target::Unborn => ([Step::ResetToCommit, Step::DeleteRef, filler], REF_STEPS),
        Target::Original => (
            [Step::ResetToCommit, Step::ResetToOriginal, filler],
            REF_STEPS,
        ),
        Target::FirstParent => match (gate, shape) {
            (Gate::Fixed, Shape::Merge) => (
                [
                    Step::ResetToCommit,
                    Step::ResetToFirstParent,
                    Step::RestoreMergeParent,
                ],
                MERGE_STEPS,
            ),
            _ => (
                [Step::ResetToCommit, Step::ResetToFirstParent, filler],
                REF_STEPS,
            ),
        },
    };
    Rollback { target, steps, len }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const SHAPES: [Shape; 4] = [Shape::Root, Shape::Single, Shape::Merge, Shape::Amend];
    pub const OUTCOMES: [Outcome; 3] = [Outcome::Signed, Outcome::Failed, Outcome::Unverified];

    /// The commits of the model: the parents, the commit an amend replaced, the user's commit, and the hook's amend of it.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Commit {
        Parent,
        SecondParent,
        Original,
        Recorded,
        Amended,
    }

    /// The repository after the commit and the hook's amend.
    /// Every step takes the form of a soft reset or a ref update, which leaves the index holding the attempted tree.
    /// The model leaves the index out, and `push_and_signing.rs` checks it on a real repository.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Repository {
        pub shape: Shape,
        pub outcome: Outcome,
        pub head: Option<Commit>,
        pub merge_head: Option<Commit>,
    }

    impl Repository {
        /// Git removed `MERGE_HEAD` before the hook ran.
        pub fn after(shape: Shape, outcome: Outcome) -> Self {
            Self {
                shape,
                outcome,
                head: Some(match outcome {
                    Outcome::Failed => Commit::Recorded,
                    Outcome::Signed | Outcome::Unverified => Commit::Amended,
                }),
                merge_head: None,
            }
        }

        /// The parents of the recorded commit, which an amend copies from the commit it replaced.
        fn parents(self) -> [Option<Commit>; 2] {
            match self.shape {
                Shape::Root => [None, None],
                Shape::Single | Shape::Amend => [Some(Commit::Parent), None],
                Shape::Merge => [Some(Commit::Parent), Some(Commit::SecondParent)],
            }
        }

        /// Where `HEAD` stood before the user's commit.
        pub fn before(self) -> Option<Commit> {
            match self.shape {
                Shape::Root => None,
                Shape::Single | Shape::Merge => Some(Commit::Parent),
                Shape::Amend => Some(Commit::Original),
            }
        }

        /// Runs one step as Git does: a reset or ref update to a commit that doesn't exist fails and changes nothing, and none touches the index.
        pub fn apply(&mut self, step: Step) {
            let [first, second] = self.parents();
            match step {
                Step::ResetToCommit => self.head = Some(Commit::Recorded),
                Step::ResetToFirstParent => {
                    if first.is_some() {
                        self.head = first;
                    }
                }
                Step::ResetToOriginal => {
                    if self.shape == Shape::Amend {
                        self.head = Some(Commit::Original);
                    }
                }
                Step::DeleteRef => self.head = None,
                Step::RestoreMergeParent => {
                    if second.is_some() {
                        self.merge_head = second;
                    }
                }
            }
        }

        /// `HEAD` names the signed amend, or it stands where it stood before the commit with the merge parents kept.
        pub fn safe(self) -> bool {
            let signed = self.outcome == Outcome::Signed && self.head == Some(Commit::Amended);
            let restored = self.head == self.before()
                && (self.shape != Shape::Merge || self.merge_head == Some(Commit::SecondParent));
            signed || restored
        }
    }

    /// The repository after the gate rolls back from `crash` steps into a run and one re-entry runs every step again.
    pub fn rolled_back(gate: Gate, shape: Shape, outcome: Outcome, crash: usize) -> Repository {
        let rollback = rollback_target(gate, shape, outcome);
        let mut repository = Repository::after(shape, outcome);
        for step in rollback.steps().iter().take(crash) {
            repository.apply(*step);
        }
        for step in rollback.steps() {
            repository.apply(*step);
        }
        repository
    }

    /// The fixed gate leaves the repository safe from every crash point, and a re-entry ends where an uninterrupted rollback ends.
    pub fn spec_rollback(shape: Shape, outcome: Outcome, crash: usize) -> Repository {
        let finished = rolled_back(Gate::Fixed, shape, outcome, 0);
        let repository = rolled_back(Gate::Fixed, shape, outcome, crash);
        assert!(repository.safe());
        assert_eq!(repository, finished);
        assert_eq!(
            rollback_target(Gate::Fixed, shape, outcome).target == Target::Keep,
            outcome == Outcome::Signed
        );
        repository
    }

    /// The hook acts only on an unsigned commit outside its own amend and outside a Git operation, and refuses only a settled, empty, unwaived commit.
    pub fn spec_decide(facts: [bool; 3]) -> Decision {
        let [in_hook, mid_operation, signed] = facts;
        let decided = decide(in_hook, mid_operation, signed);
        assert_eq!(
            decided == Decision::Resign,
            !in_hook && !mid_operation && !signed
        );
        assert_eq!(
            refuses_empty(in_hook, mid_operation, signed),
            in_hook && mid_operation && !signed
        );
        decided
    }

    /// An amend decides the shape, and otherwise the parent count does.
    pub fn spec_shape(parents: usize, amended: bool) -> Shape {
        let decided = shape(parents, amended);
        let expected = if amended {
            Shape::Amend
        } else if parents == 0 {
            Shape::Root
        } else if parents == 1 {
            Shape::Single
        } else {
            Shape::Merge
        };
        assert_eq!(decided, expected);
        decided
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn every_rollback_restores_the_head_the_tree_and_the_merge_parents_from_any_crash() {
        let shape = SHAPES[usize::from(kani::any::<u8>() % 4)];
        let outcome = OUTCOMES[usize::from(kani::any::<u8>() % 3)];
        let crash = usize::from(kani::any::<u8>() % 4);
        let repository = spec_rollback(shape, outcome, crash);
        kani::cover!(repository.head == Some(Commit::Original));
        kani::cover!(repository.merge_head == Some(Commit::SecondParent));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_an_unsigned_commit_outside_the_hook_and_any_operation_is_resigned() {
        let decided = spec_decide(kani::any());
        kani::cover!(decided == Decision::Resign);
        kani::cover!(decided == Decision::Done);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn an_amend_and_then_the_parent_count_decide_the_shape() {
        let decided = spec_shape(kani::any(), kani::any());
        kani::cover!(decided == Shape::Merge);
        kani::cover!(decided == Shape::Amend);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_situation_and_crash_point_meets_the_specifications() {
        for shape in proofs::SHAPES {
            for outcome in proofs::OUTCOMES {
                for crash in 0..=MOST_STEPS {
                    proofs::spec_rollback(shape, outcome, crash);
                }
            }
        }
        for facts in crate::domain::flags() {
            proofs::spec_decide(facts);
        }
        for parents in 0..4 {
            proofs::spec_shape(parents, false);
            proofs::spec_shape(parents, true);
        }
        for (shape, outcome) in [
            (Shape::Merge, Outcome::Failed),
            (Shape::Amend, Outcome::Unverified),
        ] {
            assert!(!proofs::rolled_back(Gate::Old, shape, outcome, 0).safe());
        }
        assert_eq!(
            rollback_target(Gate::Fixed, Shape::Merge, Outcome::Failed).steps(),
            [
                Step::ResetToCommit,
                Step::ResetToFirstParent,
                Step::RestoreMergeParent
            ]
        );
    }
}
