//! The run order of the setup steps: every step after each step it relies on, with ties in declaration order.
//! Each step goes by its index in the declaration, and `after[step]` holds one bit for each step that `step` relies on.

/// The most steps a declaration holds, one bit each in a `u32` set.
const MAX_STEPS: usize = 32;

/// Writes the run order of the steps `0..after.len()` into `order` and returns `true`.
/// Among the steps whose dependencies already ran, the one declared first runs next.
/// Returns `false` when the dependencies form a cycle, name a step outside the list, or the list holds more than 32 steps.
#[must_use]
pub fn order(after: &[u32], order: &mut [usize]) -> bool {
    let count = after.len();
    if count > MAX_STEPS || order.len() != count {
        return false;
    }
    let all = if count == MAX_STEPS {
        u32::MAX
    } else {
        (1u32 << count) - 1
    };
    let mut placed: u32 = 0;
    let mut slots = order;
    while let Some((slot, later)) = core::mem::take(&mut slots).split_first_mut() {
        let mut chosen = count;
        let mut candidates = after;
        let mut step = 0;
        while let [needs, rest @ ..] = candidates {
            let bit = 1u32 << step;
            if placed & bit == 0 && needs & !all == 0 && needs & !placed == 0 {
                chosen = step;
                break;
            }
            candidates = rest;
            step += 1;
        }
        if chosen == count {
            return false;
        }
        *slot = chosen;
        placed |= 1u32 << chosen;
        slots = later;
    }
    true
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
pub(crate) mod proofs {
    use super::*;

    /// Whether `sequence` runs each of the steps `0..after.len()` exactly once and every step after each step it relies on.
    #[must_use]
    pub fn respects(after: &[u32], sequence: &[usize]) -> bool {
        if sequence.len() != after.len() || after.len() > MAX_STEPS {
            return false;
        }
        let mut ran: u32 = 0;
        let mut index = 0;
        while let Some(&step) = sequence.get(index) {
            let Some(&needs) = after.get(step) else {
                return false;
            };
            let bit = 1u32 << step;
            if ran & bit != 0 || needs & !ran != 0 {
                return false;
            }
            ran |= bit;
            index += 1;
        }
        true
    }

    /// An accepted order runs every step once after what it relies on, and each position takes the first declared step that may run.
    pub fn spec_accepted(after: &[u32], sequence: &[usize]) {
        assert!(respects(after, sequence));
        let mut ran: u32 = 0;
        let mut position = 0;
        while let Some(&chosen) = sequence.get(position) {
            let mut step = 0;
            while step < chosen {
                let ready = ran & (1 << step) == 0
                    && after.get(step).is_some_and(|&needs| needs & !ran == 0);
                assert!(!ready);
                step += 1;
            }
            ran |= 1 << chosen;
            position += 1;
        }
    }

    #[cfg(kani)]
    fn any_dependencies<const N: usize>() -> [u32; N] {
        let after: [u32; N] = kani::any();
        let mut index = 0;
        while index < N {
            kani::assume(after[index] < (1u32 << N));
            index += 1;
        }
        after
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn an_order_runs_every_step_once_after_what_it_relies_on_and_ties_keep_the_declaration() {
        let after = any_dependencies::<4>();
        let mut sequence = [0usize; 4];
        let ordered = order(&after, &mut sequence);
        if ordered {
            spec_accepted(&after, &sequence);
        }
        kani::cover!(ordered && after[0] != 0);
        kani::cover!(!ordered);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn a_refused_order_leaves_no_sequence_that_respects_the_dependencies() {
        const PERMUTATIONS: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let after = any_dependencies::<3>();
        let mut sequence = [0usize; 3];
        let ordered = order(&after, &mut sequence);
        if !ordered {
            let mut index = 0;
            while index < PERMUTATIONS.len() {
                assert!(!respects(&after, &PERMUTATIONS[index]));
                index += 1;
            }
        }
        kani::cover!(ordered);
        kani::cover!(!ordered);
    }
}

#[cfg(test)]
mod tests {
    use super::proofs::{respects, spec_accepted};
    use super::*;
    use alloc::vec::Vec;

    /// Every ordering of `0..count`.
    fn permutations(count: usize) -> Vec<Vec<usize>> {
        if count == 0 {
            return alloc::vec![Vec::new()];
        }
        let mut found = Vec::new();
        for shorter in permutations(count - 1) {
            for position in 0..=shorter.len() {
                let mut longer = shorter.clone();
                longer.insert(position, count - 1);
                found.push(longer);
            }
        }
        found
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every dependency set")]
    fn every_dependency_set_of_up_to_four_steps_is_ordered_exactly_when_some_order_respects_it() {
        for count in 0..=4 {
            let orders = permutations(count);
            let sets = 1u32 << count;
            for code in 0..sets.pow(u32::try_from(count).unwrap_or_default()) {
                let after: Vec<u32> = (0..count)
                    .map(|step| code / sets.pow(u32::try_from(step).unwrap_or_default()) % sets)
                    .collect();
                let mut sequence = alloc::vec![0; count];
                let ordered = order(&after, &mut sequence);
                if ordered {
                    spec_accepted(&after, &sequence);
                }
                assert_eq!(
                    ordered,
                    orders.iter().any(|candidate| respects(&after, candidate)),
                    "{after:?}"
                );
            }
        }
    }

    #[test]
    fn a_step_runs_after_what_it_relies_on_and_ties_keep_the_declaration() {
        let after = [0b100, 0, 0];
        let mut sequence = [0; 3];
        assert!(order(&after, &mut sequence));
        assert_eq!(sequence, [1, 2, 0]);
        assert!(respects(&after, &sequence));
        assert!(!respects(&after, &[0, 1, 2]));
    }

    #[test]
    fn a_cycle_a_self_dependency_an_unknown_step_or_too_many_steps_is_refused() {
        let mut sequence = [0; 2];
        assert!(!order(&[0b10, 0b01], &mut sequence));
        assert!(!order(&[0b01, 0], &mut sequence));
        assert!(!order(&[0b100, 0], &mut sequence));
        assert!(!order(&[0, 0], &mut [0; 1]));
        let mut longest = [0; MAX_STEPS];
        assert!(order(&[0; MAX_STEPS], &mut longest));
        assert_eq!(longest.get(MAX_STEPS - 1), Some(&(MAX_STEPS - 1)));
        let mut last_first = [0u32; MAX_STEPS];
        if let Some(first) = last_first.first_mut() {
            *first = 1 << (MAX_STEPS - 1);
        }
        assert!(order(&last_first, &mut longest));
        assert_eq!(longest.last(), Some(&0));
        assert!(!order(&[0; MAX_STEPS + 1], &mut [0; MAX_STEPS + 1]));
        assert!(!respects(&[0; MAX_STEPS + 1], &[0; MAX_STEPS + 1]));
    }

    #[test]
    fn a_sequence_runs_each_step_once() {
        assert!(!respects(&[0, 0], &[0, 0]));
        assert!(!respects(&[0, 0], &[0]));
        assert!(!respects(&[0, 0], &[0, 2]));
    }
}
