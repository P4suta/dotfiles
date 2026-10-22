#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssessmentChange {
    Keep,
    Resign,
    Reassess,
}

#[must_use]
pub const fn assessment_change(
    same_binding: bool,
    same_diff: bool,
    same_identity: bool,
) -> AssessmentChange {
    if !same_diff || !same_identity {
        AssessmentChange::Reassess
    } else if same_binding {
        AssessmentChange::Keep
    } else {
        AssessmentChange::Resign
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaseUpdate {
    Current,
    Merge,
    Restack,
}

#[must_use]
pub const fn base_update(behind: bool, stacked: bool, queued: bool) -> BaseUpdate {
    if !behind {
        BaseUpdate::Current
    } else if stacked {
        BaseUpdate::Restack
    } else if queued {
        BaseUpdate::Current
    } else {
        BaseUpdate::Merge
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Settlement {
    Waiting,
    Merged,
    Ejected,
    TimedOut,
    HeadChanged,
    Closed,
}

impl Settlement {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Merged => "merged",
            Self::Ejected => "ejected",
            Self::TimedOut => "timed_out",
            Self::HeadChanged => "head_changed",
            Self::Closed => "closed",
        }
    }
}

#[must_use]
pub const fn settlement(
    same_head: bool,
    lifecycle: Lifecycle,
    queue: Queue,
    expired: bool,
) -> Settlement {
    if !same_head {
        Settlement::HeadChanged
    } else if matches!(lifecycle, Lifecycle::Merged) {
        Settlement::Merged
    } else if matches!(lifecycle, Lifecycle::Closed) {
        Settlement::Closed
    } else if matches!(queue, Queue::Removed) {
        Settlement::Ejected
    } else if expired {
        Settlement::TimedOut
    } else {
        Settlement::Waiting
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lifecycle {
    Open,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Queue {
    NotRequested,
    Present,
    Removed,
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    fn spec_assessment(binding: bool, diff: bool, identity: bool) {
        let decision = assessment_change(binding, diff, identity);
        assert_eq!(decision == AssessmentChange::Reassess, !(diff && identity));
        assert_eq!(
            decision == AssessmentChange::Keep,
            binding && diff && identity
        );
        assert_eq!(
            decision == AssessmentChange::Resign,
            !binding && diff && identity
        );
    }

    fn spec_base(behind: bool, stacked: bool, queue: bool) {
        let update = base_update(behind, stacked, queue);
        assert_eq!(update == BaseUpdate::Restack, behind && stacked);
        assert_eq!(update == BaseUpdate::Merge, behind && !stacked && !queue);
        assert_eq!(update == BaseUpdate::Current, !behind || !stacked && queue);
    }

    fn spec_settlement(head: bool, lifecycle: Lifecycle, queue: Queue, expired: bool) {
        let result = settlement(head, lifecycle, queue, expired);
        let expected = match (
            head,
            lifecycle == Lifecycle::Merged,
            lifecycle == Lifecycle::Closed,
            queue == Queue::Removed,
            expired,
        ) {
            (false, _, _, _, _) => Settlement::HeadChanged,
            (true, true, _, _, _) => Settlement::Merged,
            (true, false, true, _, _) => Settlement::Closed,
            (true, false, false, true, _) => Settlement::Ejected,
            (true, false, false, false, true) => Settlement::TimedOut,
            (true, false, false, false, false) => Settlement::Waiting,
        };
        assert_eq!(result, expected);
        let key = match expected {
            Settlement::Waiting => "waiting",
            Settlement::Merged => "merged",
            Settlement::Ejected => "ejected",
            Settlement::TimedOut => "timed_out",
            Settlement::HeadChanged => "head_changed",
            Settlement::Closed => "closed",
        };
        assert_eq!(result.key(), key);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn assessment_carry_requires_the_same_diff_source_and_trusted_signer() {
        let [binding, diff, identity] = kani::any();
        spec_assessment(binding, diff, identity);
        let decision = assessment_change(binding, diff, identity);
        kani::cover!(decision == AssessmentChange::Resign);
        kani::cover!(decision == AssessmentChange::Reassess);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_queue_owns_base_updates_but_stacks_still_restack() {
        let [behind, stacked, queued] = kani::any();
        spec_base(behind, stacked, queued);
        let update = base_update(behind, stacked, queued);
        kani::cover!(update == BaseUpdate::Merge);
        kani::cover!(update == BaseUpdate::Restack);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn merge_settlement_never_credits_a_different_head_or_a_queue_acknowledgement() {
        let [head, expired] = kani::any();
        let lifecycle = [Lifecycle::Open, Lifecycle::Merged, Lifecycle::Closed]
            [usize::from(kani::any::<u8>() % 3)];
        let queue = [Queue::NotRequested, Queue::Present, Queue::Removed]
            [usize::from(kani::any::<u8>() % 3)];
        spec_settlement(head, lifecycle, queue, expired);
        let result = settlement(head, lifecycle, queue, expired);
        kani::cover!(result == Settlement::Merged);
        kani::cover!(result == Settlement::Ejected);
    }

    #[test]
    fn every_assessment_and_base_transition() {
        for head in [false, true] {
            for diff in [false, true] {
                for source in [false, true] {
                    spec_base(head, diff, source);
                    spec_assessment(head, diff, source);
                }
            }
        }
    }

    #[test]
    fn every_merge_settlement() {
        for head in [false, true] {
            for lifecycle in [Lifecycle::Open, Lifecycle::Merged, Lifecycle::Closed] {
                for queue in [Queue::NotRequested, Queue::Present, Queue::Removed] {
                    for expired in [false, true] {
                        spec_settlement(head, lifecycle, queue, expired);
                    }
                }
            }
        }
    }
}
