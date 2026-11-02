//! Whether a leased restack carries a branch's issue context to its rewritten head.

/// How the restack moved a branch relative to the start commit that its issue context records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Movement {
    Unmoved,
    /// The branch moved, but its recorded start commit is still an ancestor of the new head.
    StillCurrent,
    /// The branch moved to a head that no longer contains its recorded start commit.
    Rewritten,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Carry {
    /// The context still holds, so the restack leaves it unchanged.
    Unchanged,
    /// The context moves to the rewritten head.
    Carried,
    /// The context stays at its recorded start commit, and the branch needs a new start.
    Left,
}

/// A rewritten branch carries its context only when that context held before the restack and the restack kept the branch's own change.
#[must_use]
pub const fn carry(movement: Movement, was_current: bool, same_change: bool) -> Carry {
    match (movement, was_current, same_change) {
        (Movement::Unmoved | Movement::StillCurrent, _, _) => Carry::Unchanged,
        (Movement::Rewritten, true, true) => Carry::Carried,
        (Movement::Rewritten, false, _) | (Movement::Rewritten, true, false) => Carry::Left,
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_carry(movement: Movement, was_current: bool, same_change: bool) {
        let decided = carry(movement, was_current, same_change);
        assert_eq!(
            decided == Carry::Carried,
            movement == Movement::Rewritten && was_current && same_change
        );
        assert_eq!(decided == Carry::Unchanged, movement != Movement::Rewritten);
        assert_eq!(
            decided == Carry::Left,
            movement == Movement::Rewritten && !(was_current && same_change)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_an_unchanged_rewrite_of_a_current_context_carries_it() {
        let movement: Movement = kani::any();
        let was_current: bool = kani::any();
        let same_change: bool = kani::any();
        spec_carry(movement, was_current, same_change);
        let decided = carry(movement, was_current, same_change);
        kani::cover!(decided == Carry::Carried);
        kani::cover!(decided == Carry::Left);
        kani::cover!(decided == Carry::Unchanged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_movement_and_fact_meets_the_specification() {
        for movement in [
            Movement::Unmoved,
            Movement::StillCurrent,
            Movement::Rewritten,
        ] {
            for [was_current, same_change] in crate::domain::flags() {
                proofs::spec_carry(movement, was_current, same_change);
            }
        }
    }

    #[test]
    fn a_changed_rewrite_leaves_the_context_behind() {
        assert_eq!(carry(Movement::Rewritten, true, false), Carry::Left);
        assert_eq!(carry(Movement::Rewritten, true, true), Carry::Carried);
    }
}
