//! How an installed skill differs from its entry in the approved managed catalog.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Difference {
    /// The installed tree holds a skill that the approved catalog doesn't name.
    Added,
    /// The approved catalog names a skill that the installed tree lacks.
    Removed,
    /// Both hold the skill, but its installed content differs from the approved entry.
    Changed,
}

/// `approved` and `installed` say whether each side holds the skill, and `same` whether the installed content matches the approved entry.
#[must_use]
pub const fn difference(approved: bool, installed: bool, same: bool) -> Option<Difference> {
    match (approved, installed, same) {
        (true, true, true) | (false, false, _) => None,
        (true, true, false) => Some(Difference::Changed),
        (true, false, _) => Some(Difference::Removed),
        (false, true, _) => Some(Difference::Added),
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_difference(approved: bool, installed: bool, same: bool) {
        let found = difference(approved, installed, same);
        assert_eq!(found == Some(Difference::Added), !approved && installed);
        assert_eq!(found == Some(Difference::Removed), approved && !installed);
        assert_eq!(
            found == Some(Difference::Changed),
            approved && installed && !same
        );
        assert_eq!(
            found.is_none(),
            approved == installed && (same || !approved)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn each_skill_reports_exactly_how_it_left_the_approved_catalog() {
        let approved: bool = kani::any();
        let installed: bool = kani::any();
        let same: bool = kani::any();
        spec_difference(approved, installed, same);
        let found = difference(approved, installed, same);
        kani::cover!(found.is_none());
        kani::cover!(found == Some(Difference::Added));
        kani::cover!(found == Some(Difference::Removed));
        kani::cover!(found == Some(Difference::Changed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_presence_and_content_fact_meets_the_specification() {
        for [approved, installed, same] in crate::domain::flags() {
            proofs::spec_difference(approved, installed, same);
        }
    }

    #[test]
    fn an_unchanged_skill_reports_nothing_and_a_changed_one_reports_its_change() {
        assert_eq!(difference(true, true, true), None);
        assert_eq!(difference(true, true, false), Some(Difference::Changed));
    }
}
