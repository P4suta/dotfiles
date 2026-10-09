//! The steps of a dependency cycle, so a refused run order names the steps that keep it from existing.
//! Each step goes by its index in the declaration, and `after[step]` holds one bit for each step that `step` relies on.

/// The most steps a declaration holds, one bit each in a `u32` set.
const MAX_STEPS: usize = 32;

/// The steps that rely on something that never runs: what remains after the scan sets aside every step whose dependencies can all run.
fn stuck(after: &[u32]) -> u32 {
    let count = after.len().min(MAX_STEPS);
    let all = if count == MAX_STEPS {
        u32::MAX
    } else {
        (1u32 << count) - 1
    };
    let mut remaining = all;
    // Each round sets aside at least one step or ends, so `count` rounds always reach the end.
    for _ in 0..count {
        let mut freed = 0;
        for (index, needs) in after.iter().take(count).enumerate() {
            let bit = 1u32 << index;
            if needs & remaining & all == 0 {
                freed |= bit;
            }
        }
        remaining &= !freed;
    }
    remaining
}

/// The steps of one dependency cycle among `after`, as a set of bits, or 0 when the steps can all run in some order.
/// Walking from the first stuck step along its first stuck dependency must return to a step it passed, and the steps from there on form the cycle.
#[must_use]
pub fn cycle(after: &[u32]) -> u32 {
    let remaining = stuck(after);
    // The first stuck dependency of a stuck step, which a stuck step always has.
    let next = |step: u32| {
        (after
            .get(usize::try_from(step).unwrap_or(MAX_STEPS))
            .copied()
            .unwrap_or(0)
            & remaining)
            .trailing_zeros()
    };
    // After one move per step, the walk stands inside the cycle it ends in.
    let mut step = remaining.trailing_zeros();
    for _ in 0..after.len().min(MAX_STEPS) {
        step = next(step);
    }
    if step >= u32::BITS {
        return 0;
    }
    let mut members = 0u32;
    for _ in 0..after.len().min(MAX_STEPS) {
        members |= 1 << step;
        step = next(step);
    }
    members
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// A reported cycle holds at least one step exactly when some step can never run, and each of its steps relies on another of them.
    pub fn spec_cycle(after: &[u32]) -> u32 {
        let members = cycle(after);
        assert_eq!(members == 0, stuck(after) == 0);
        let mut index = 0;
        while let Some(&needs) = after.get(index) {
            if members & (1 << index) != 0 {
                assert_ne!(needs & members, 0);
            }
            index += 1;
        }
        members
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_reported_cycle_holds_only_steps_that_rely_on_one_another() {
        let after: [u32; 4] = kani::any();
        let mut index = 0;
        while index < 4 {
            kani::assume(after[index] < 16);
            index += 1;
        }
        let members = spec_cycle(&after);
        kani::cover!(members == 0);
        kani::cover!(members.count_ones() == 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every dependency set")]
    fn every_dependency_set_of_up_to_four_steps_reports_a_cycle_exactly_when_it_has_one() {
        for count in 0..=4u32 {
            let sets = 1u32 << count;
            for code in 0..sets.pow(count) {
                let after: alloc::vec::Vec<u32> = (0..count)
                    .map(|step| code / sets.pow(step) % sets)
                    .collect();
                proofs::spec_cycle(&after);
            }
        }
        assert_eq!(cycle(&[0b10, 0b01]), 0b11);
        assert_eq!(cycle(&[0b001, 0]), 0b001);
        assert_eq!(cycle(&[0, 0b100, 0b010]), 0b110);
        assert_eq!(cycle(&[0b10, 0]), 0);
        assert_eq!(cycle(&[0b1000, 0b100, 0b10, 0b100]), 0b110);
    }
}
