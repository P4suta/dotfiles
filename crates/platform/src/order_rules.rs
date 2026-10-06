//! The run order of the setup steps: every step after each step it relies on, with ties in declaration order.
//! Each step goes by its index in the declaration, and `after[step]` holds one bit for each step that `step` relies on.

/// Writes the run order of the steps `0..after.len()` into `order` and returns `true`.
/// Among the steps whose dependencies already ran, the one declared first runs next.
/// Returns `false` when the dependencies form a cycle, name a step outside the list, or the list holds more than 32 steps.
#[must_use]
pub fn order(after: &[u32], order: &mut [usize]) -> bool {
    let count = after.len();
    if count > 32 || order.len() != count {
        return false;
    }
    let all = if count == 32 {
        u32::MAX
    } else {
        (1u32 << count) - 1
    };
    let mut placed: u32 = 0;
    let mut position = 0;
    while position < count {
        let mut chosen = count;
        let mut step = 0;
        while step < count {
            let bit = 1u32 << step;
            if placed & bit == 0 && after[step] & !all == 0 && after[step] & !placed == 0 {
                chosen = step;
                break;
            }
            step += 1;
        }
        if chosen == count {
            return false;
        }
        order[position] = chosen;
        placed |= 1u32 << chosen;
        position += 1;
    }
    true
}

/// Whether `sequence` runs each of the steps `0..after.len()` exactly once and every step after each step it relies on.
#[must_use]
pub fn respects(after: &[u32], sequence: &[usize]) -> bool {
    if sequence.len() != after.len() || after.len() > 32 {
        return false;
    }
    let mut ran: u32 = 0;
    let mut index = 0;
    while index < sequence.len() {
        let step = sequence[index];
        if step >= after.len() {
            return false;
        }
        let bit = 1u32 << step;
        if ran & bit != 0 || after[step] & !ran != 0 {
            return false;
        }
        ran |= bit;
        index += 1;
    }
    true
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
fn an_order_runs_every_step_once_after_what_it_relies_on() {
    let after = any_dependencies::<4>();
    let mut sequence = [0usize; 4];
    let ordered = order(&after, &mut sequence);
    if ordered {
        assert!(respects(&after, &sequence));
    }
    kani::cover!(ordered && after[0] != 0);
    kani::cover!(!ordered);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(6)]
fn a_declaration_whose_steps_rely_only_on_earlier_ones_keeps_its_order() {
    let after = any_dependencies::<4>();
    let mut index = 0;
    while index < 4 {
        kani::assume(after[index] >> index == 0);
        index += 1;
    }
    let mut sequence = [0usize; 4];
    assert!(order(&after, &mut sequence));
    let mut position = 0;
    while position < 4 {
        assert!(sequence[position] == position);
        position += 1;
    }
    kani::cover!(after[3] == 0b111);
    kani::cover!(after[0] | after[1] | after[2] | after[3] == 0);
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_cycle_a_self_dependency_or_an_unknown_step_is_refused() {
        let mut sequence = [0; 2];
        assert!(!order(&[0b10, 0b01], &mut sequence));
        assert!(!order(&[0b01, 0], &mut sequence));
        assert!(!order(&[0b100, 0], &mut sequence));
        assert!(!order(&[0, 0], &mut [0; 1]));
    }

    #[test]
    fn a_sequence_runs_each_step_once() {
        assert!(!respects(&[0, 0], &[0, 0]));
        assert!(!respects(&[0, 0], &[0]));
        assert!(!respects(&[0, 0], &[0, 2]));
    }
}
