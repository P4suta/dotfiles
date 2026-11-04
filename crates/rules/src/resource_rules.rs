//! Admission for a declared peak allocation and its bounded competing consumers.

/// Workspace, build growth, proof growth, temporary growth, safety, and competing growth.
pub const COMPONENTS: usize = 6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Allocation {
    pub bytes: u64,
    pub inodes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Inodes {
    Available(u64),
    NotApplicable,
    Unmeasured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    Overflow,
    EmptyPlan,
    UnmeasuredBytes,
    UnmeasuredInodes,
    Bytes,
    Inodes,
}

/// The inode capacity of one `df -i` row with its `total` and `free` counts.
/// A filesystem without a fixed inode table, such as Btrfs or an overlay on it, reports a total of zero.
/// A row with more free inodes than its total is inconsistent, so it leaves the capacity unmeasured.
#[must_use]
pub const fn reported_inodes(total: u64, free: u64) -> Inodes {
    if free > total {
        Inodes::Unmeasured
    } else if total == 0 {
        Inodes::NotApplicable
    } else {
        Inodes::Available(free)
    }
}

/// Sum every declared allocation without admitting arithmetic overflow.
#[must_use]
pub fn required(plan: [Allocation; COMPONENTS]) -> Option<Allocation> {
    let mut total = Allocation::default();
    for allocation in plan {
        total.bytes = total.bytes.checked_add(allocation.bytes)?;
        total.inodes = total.inodes.checked_add(allocation.inodes)?;
    }
    Some(total)
}

/// Missing measurements and insufficient headroom refuse the complete declared plan.
///
/// # Errors
/// Returns the first invalid allocation, missing measurement, or insufficient resource.
pub fn admission(
    plan: [Allocation; COMPONENTS],
    bytes: Option<u64>,
    inodes: Inodes,
) -> Result<Allocation, Refusal> {
    let total = required(plan).ok_or(Refusal::Overflow)?;
    if total.bytes == 0 || total.inodes == 0 {
        return Err(Refusal::EmptyPlan);
    }
    let bytes = bytes.ok_or(Refusal::UnmeasuredBytes)?;
    if bytes < total.bytes {
        return Err(Refusal::Bytes);
    }
    match inodes {
        Inodes::Unmeasured => Err(Refusal::UnmeasuredInodes),
        Inodes::Available(available) if available < total.inodes => Err(Refusal::Inodes),
        Inodes::Available(_) | Inodes::NotApplicable => Ok(total),
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    #[kani::unwind(7)]
    fn every_peak_sum_preserves_all_components_or_refuses_overflow() {
        let plan: [Allocation; COMPONENTS] = kani::any();
        let mut bytes = 0u128;
        let mut inodes = 0u128;
        for allocation in plan {
            bytes += u128::from(allocation.bytes);
            inodes += u128::from(allocation.inodes);
        }
        let result = required(plan);
        let fits = bytes <= u128::from(u64::MAX) && inodes <= u128::from(u64::MAX);
        assert_eq!(result.is_some(), fits);
        if let Some(total) = result {
            assert_eq!(u128::from(total.bytes), bytes);
            assert_eq!(u128::from(total.inodes), inodes);
        }
        kani::cover!(result.is_some());
        kani::cover!(result.is_none());
    }

    #[kani::proof]
    #[kani::unwind(7)]
    fn admitted_resources_cover_the_complete_measured_nonempty_plan() {
        let plan: [Allocation; COMPONENTS] = kani::any();
        let bytes: Option<u64> = kani::any();
        let inodes: Inodes = kani::any();
        let result = admission(plan, bytes, inodes);
        let expected = required(plan).is_some_and(|total| {
            total.bytes > 0
                && total.inodes > 0
                && bytes.is_some_and(|available| available >= total.bytes)
                && match inodes {
                    Inodes::Available(available) => available >= total.inodes,
                    Inodes::NotApplicable => true,
                    Inodes::Unmeasured => false,
                }
        });
        assert_eq!(result.is_ok(), expected);
        if let Ok(total) = result {
            assert_eq!(Some(total), required(plan));
        }
        kani::cover!(result.is_ok());
        kani::cover!(result.is_err());
    }

    #[kani::proof]
    fn only_a_consistent_zero_total_reports_no_inode_table() {
        let [total, free]: [u64; 2] = kani::any();
        let reported = reported_inodes(total, free);
        assert_eq!(reported == Inodes::NotApplicable, total == 0 && free == 0);
        assert_eq!(reported == Inodes::Unmeasured, free > total);
        assert_eq!(
            reported == Inodes::Available(free),
            total > 0 && free <= total
        );
        kani::cover!(reported == Inodes::NotApplicable);
        kani::cover!(reported == Inodes::Unmeasured);
        kani::cover!(matches!(reported, Inodes::Available(_)));
    }

    #[kani::proof]
    #[kani::unwind(7)]
    fn a_fixed_inode_table_still_refuses_a_real_shortfall() {
        let plan: [Allocation; COMPONENTS] = kani::any();
        let bytes: Option<u64> = kani::any();
        let [total, free]: [u64; 2] = kani::any();
        let result = admission(plan, bytes, reported_inodes(total, free));
        let shortfall = total > 0 && required(plan).is_some_and(|need| free < need.inodes);
        if shortfall {
            assert!(result.is_err());
            let otherwise_admitted = required(plan).is_some_and(|need| {
                need.bytes > 0
                    && need.inodes > 0
                    && bytes.is_some_and(|available| available >= need.bytes)
            });
            if otherwise_admitted {
                assert_eq!(
                    result,
                    Err(if free > total {
                        Refusal::UnmeasuredInodes
                    } else {
                        Refusal::Inodes
                    })
                );
            }
        }
        kani::cover!(shortfall && result == Err(Refusal::Inodes));
        kani::cover!(total == 0 && free == 0 && result.is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> [Allocation; COMPONENTS] {
        [Allocation {
            bytes: 10,
            inodes: 2,
        }; COMPONENTS]
    }

    #[test]
    fn a_competing_allocation_can_refuse_an_otherwise_fitting_campaign() {
        let mut peak = plan();
        assert!(admission(peak, Some(60), Inodes::Available(12)).is_ok());
        peak.last_mut().unwrap().bytes += 1;
        assert_eq!(
            admission(peak, Some(60), Inodes::Available(12)),
            Err(Refusal::Bytes)
        );
    }

    #[test]
    fn unavailable_and_exhausted_inodes_do_not_supply_admission() {
        assert_eq!(
            admission(plan(), Some(60), Inodes::Unmeasured),
            Err(Refusal::UnmeasuredInodes)
        );
        assert_eq!(
            admission(plan(), Some(60), Inodes::Available(11)),
            Err(Refusal::Inodes)
        );
        assert!(admission(plan(), Some(60), Inodes::NotApplicable).is_ok());
        assert_eq!(
            admission(plan(), None, Inodes::Available(12)),
            Err(Refusal::UnmeasuredBytes)
        );
    }

    #[test]
    fn a_reported_row_maps_tableless_fixed_and_inconsistent_counts() {
        assert_eq!(reported_inodes(0, 0), Inodes::NotApplicable);
        assert_eq!(reported_inodes(0, 1), Inodes::Unmeasured);
        assert_eq!(reported_inodes(12, 13), Inodes::Unmeasured);
        assert_eq!(reported_inodes(12, 0), Inodes::Available(0));
        assert_eq!(reported_inodes(12, 12), Inodes::Available(12));
        assert!(admission(plan(), Some(60), reported_inodes(0, 0)).is_ok());
        assert_eq!(
            admission(plan(), Some(60), reported_inodes(100, 11)),
            Err(Refusal::Inodes)
        );
        assert_eq!(
            admission(plan(), Some(60), reported_inodes(0, 12)),
            Err(Refusal::UnmeasuredInodes)
        );
        for total in 0..=13 {
            for free in 0..=13 {
                let reported = reported_inodes(total, free);
                assert_eq!(reported == Inodes::NotApplicable, total == 0 && free == 0);
                assert_eq!(reported == Inodes::Unmeasured, free > total);
                if total > 0 && free < 12 {
                    assert!(admission(plan(), Some(60), reported).is_err());
                }
            }
        }
    }

    #[test]
    fn overflowing_and_empty_plans_are_refused_before_measurement_credit() {
        let mut peak = plan();
        peak.first_mut().unwrap().bytes = u64::MAX;
        assert_eq!(required(peak), None);
        assert_eq!(
            admission(peak, Some(u64::MAX), Inodes::NotApplicable),
            Err(Refusal::Overflow)
        );
        assert_eq!(
            admission(
                [Allocation::default(); COMPONENTS],
                Some(u64::MAX),
                Inodes::NotApplicable
            ),
            Err(Refusal::EmptyPlan)
        );
        assert_eq!(
            admission(
                [Allocation {
                    bytes: 0,
                    inodes: 2
                }; COMPONENTS],
                Some(u64::MAX),
                Inodes::Available(u64::MAX)
            ),
            Err(Refusal::EmptyPlan)
        );
        assert_eq!(
            admission(
                [Allocation {
                    bytes: 10,
                    inodes: 0
                }; COMPONENTS],
                Some(u64::MAX),
                Inodes::NotApplicable
            ),
            Err(Refusal::EmptyPlan)
        );
    }
}
