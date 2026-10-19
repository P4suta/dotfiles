//! Decisions for one captured `BuildKit` server and its bounded preparation.

pub const WALL_SECONDS: u64 = 1_800;
pub const RELEASE_SECONDS: u64 = 30;
pub const WORK_SECONDS: u64 = WALL_SECONDS - RELEASE_SECONDS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Ownership {
    Uncaptured,
    Changed,
    Captured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Capacity {
    Unmeasured,
    Exhausted,
    Admitted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Work {
    Continue,
    Release,
    Refuse,
}

/// New work requires current ownership, capacity, and reserved release time.
#[must_use]
pub fn work(elapsed: u64, ownership: Ownership, capacity: Capacity) -> Work {
    match (ownership, capacity) {
        (Ownership::Uncaptured | Ownership::Changed, _) => Work::Refuse,
        (Ownership::Captured, Capacity::Admitted) if elapsed < WORK_SECONDS => Work::Continue,
        (Ownership::Captured, Capacity::Unmeasured | Capacity::Exhausted | Capacity::Admitted) => {
            Work::Release
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Running {
    Unknown,
    Yes,
    No,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Release {
    Stop,
    Acknowledged,
    Unsettled,
}

/// No name lookup or client exit substitutes for same-ID server readback.
#[must_use]
pub fn release(elapsed: u64, ownership: Ownership, running: Running) -> Release {
    match (ownership, running) {
        (Ownership::Uncaptured | Ownership::Changed, _) | (_, Running::Unknown) => {
            Release::Unsettled
        }
        (Ownership::Captured, Running::No) => Release::Acknowledged,
        (Ownership::Captured, Running::Yes) if elapsed < WALL_SECONDS => Release::Stop,
        (Ownership::Captured, Running::Yes) => Release::Unsettled,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservedStorage {
    Unknown,
    BelowFloor,
    DeclaredLoss,
    Admitted,
}

/// Observations beyond the initial reading never earn imaginary growth credit.
#[must_use]
pub fn observed_storage(
    initial: Option<u64>,
    current: Option<u64>,
    floor: u64,
    maximum_loss: u64,
) -> ObservedStorage {
    let (Some(initial), Some(current)) = (initial, current) else {
        return ObservedStorage::Unknown;
    };
    if current < floor {
        return ObservedStorage::BelowFloor;
    }
    if initial.saturating_sub(current) > maximum_loss {
        return ObservedStorage::DeclaredLoss;
    }
    ObservedStorage::Admitted
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn every_continuing_preparation_has_current_ownership_capacity_and_release_time() {
        let elapsed: u64 = kani::any();
        let ownership: Ownership = kani::any();
        let capacity: Capacity = kani::any();
        let result = work(elapsed, ownership, capacity);
        assert_eq!(
            result == Work::Continue,
            elapsed < WORK_SECONDS
                && ownership == Ownership::Captured
                && capacity == Capacity::Admitted
        );
        kani::cover!(result == Work::Continue);
        kani::cover!(result == Work::Release);
    }

    #[kani::proof]
    fn every_release_acknowledgement_names_the_same_captured_stopped_server() {
        let elapsed: u64 = kani::any();
        let ownership: Ownership = kani::any();
        let running: Running = kani::any();
        let result = release(elapsed, ownership, running);
        assert_eq!(
            result == Release::Acknowledged,
            ownership == Ownership::Captured && running == Running::No
        );
        assert_eq!(
            result == Release::Stop,
            ownership == Ownership::Captured && running == Running::Yes && elapsed < WALL_SECONDS
        );
        kani::cover!(result == Release::Acknowledged);
        kani::cover!(result == Release::Unsettled);
    }

    #[kani::proof]
    fn admitted_observed_storage_meets_the_floor_and_declared_loss_without_overflow() {
        let initial: Option<u64> = kani::any();
        let current: Option<u64> = kani::any();
        let floor: u64 = kani::any();
        let loss: u64 = kani::any();
        let result = observed_storage(initial, current, floor, loss);
        let expected = match (initial, current) {
            (Some(initial), Some(current)) => {
                current >= floor && u128::from(initial) <= u128::from(current) + u128::from(loss)
            }
            _ => false,
        };
        assert_eq!(result == ObservedStorage::Admitted, expected);
        kani::cover!(result == ObservedStorage::Admitted);
        kani::cover!(result != ObservedStorage::Admitted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_reserves_release_time_and_missing_facts_never_continue() {
        assert_eq!(WORK_SECONDS + RELEASE_SECONDS, WALL_SECONDS);
        for ownership in [
            Ownership::Uncaptured,
            Ownership::Changed,
            Ownership::Captured,
        ] {
            for capacity in [
                Capacity::Unmeasured,
                Capacity::Exhausted,
                Capacity::Admitted,
            ] {
                for elapsed in [0, WORK_SECONDS - 1, WORK_SECONDS, WALL_SECONDS, u64::MAX] {
                    assert_eq!(
                        work(elapsed, ownership, capacity) == Work::Continue,
                        ownership == Ownership::Captured
                            && capacity == Capacity::Admitted
                            && elapsed < WORK_SECONDS
                    );
                }
            }
        }
    }

    #[test]
    fn unknown_foreign_and_unacknowledged_servers_remain_unsettled() {
        for (ownership, expectations) in [
            (Ownership::Uncaptured, [[Release::Unsettled; 4]; 3]),
            (Ownership::Changed, [[Release::Unsettled; 4]; 3]),
            (
                Ownership::Captured,
                [
                    [Release::Unsettled; 4],
                    [
                        Release::Stop,
                        Release::Stop,
                        Release::Unsettled,
                        Release::Unsettled,
                    ],
                    [Release::Acknowledged; 4],
                ],
            ),
        ] {
            for (running, expectations) in [Running::Unknown, Running::Yes, Running::No]
                .into_iter()
                .zip(expectations)
            {
                for (elapsed, expected) in [0, WALL_SECONDS - 1, WALL_SECONDS, u64::MAX]
                    .into_iter()
                    .zip(expectations)
                {
                    assert_eq!(release(elapsed, ownership, running), expected);
                }
            }
        }
    }

    #[test]
    fn observed_losses_refuse_missing_floor_and_overflow_without_extra_credit() {
        assert_eq!(
            observed_storage(Some(10), Some(10), 10, 0),
            ObservedStorage::Admitted
        );
        assert_eq!(
            observed_storage(None, Some(10), 1, 2),
            ObservedStorage::Unknown
        );
        assert_eq!(
            observed_storage(Some(10), None, 1, 2),
            ObservedStorage::Unknown
        );
        assert_eq!(
            observed_storage(Some(10), Some(0), 1, 20),
            ObservedStorage::BelowFloor
        );
        assert_eq!(
            observed_storage(Some(10), Some(7), 1, 2),
            ObservedStorage::DeclaredLoss
        );
        assert_eq!(
            observed_storage(Some(u64::MAX), Some(u64::MAX - 1), 0, 1),
            ObservedStorage::Admitted
        );
        assert_eq!(
            observed_storage(Some(1), Some(u64::MAX), 0, 0),
            ObservedStorage::Admitted
        );
    }
}
