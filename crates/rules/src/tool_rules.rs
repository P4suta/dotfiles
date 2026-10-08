/// How the skill catalog covers one tool that an install source places on a profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Coverage {
    /// A skill declares the tool at the version the inventory installs, or one decision says the tool needs no skill.
    Covered,
    /// No skill declares the tool and no decision covers it.
    Missing,
    /// A skill declares the tool, and a decision also says the tool needs no skill.
    Contradicted,
    /// A skill declares the tool at a version the inventory no longer installs, so its decision needs a review.
    Stale,
    /// A skill or a decision names a tool that no install source places on any profile.
    Orphaned,
}

/// `installed` holds whether an install source places the tool, and `skills` how many skills declare it.
/// `current` holds whether every declaring skill names a version the inventory installs, and `decisions` how many no-skill decisions name the tool.
#[must_use]
pub fn coverage(installed: bool, skills: usize, current: bool, decisions: usize) -> Coverage {
    if !installed {
        if skills == 0 && decisions == 0 {
            Coverage::Covered
        } else {
            Coverage::Orphaned
        }
    } else if (skills > 0 && decisions > 0) || decisions > 1 {
        Coverage::Contradicted
    } else if skills > 0 {
        if current {
            Coverage::Covered
        } else {
            Coverage::Stale
        }
    } else if decisions == 1 {
        Coverage::Covered
    } else {
        Coverage::Missing
    }
}

/// Where a skill bound to tools installs: a skill for a tool that exists on only some profiles installs on exactly those, and any other skill installs everywhere.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Placement {
    Correct,
    /// The skill installs on a profile that lacks its platform-specific tool, or misses one that has it.
    Misplaced,
}

/// `platform_specific` holds whether the skill's tool exists on only some platforms, and `restricted_to_its_profiles` whether the skill installs on exactly the profiles that install the tool.
/// `restricted` holds whether the skill installs on fewer than every profile.
#[must_use]
pub fn placement(
    platform_specific: bool,
    restricted: bool,
    restricted_to_its_profiles: bool,
) -> Placement {
    let correct = if platform_specific {
        restricted_to_its_profiles
    } else {
        !restricted
    };
    if correct {
        Placement::Correct
    } else {
        Placement::Misplaced
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// An installed tool passes with current skills or one decision, a tool nothing installs passes when nothing names it, and every other state names its fault.
    pub fn spec_coverage(installed: bool, skills: usize, current: bool, decisions: usize) {
        let result = coverage(installed, skills, current, decisions);
        let named = skills > 0 || decisions > 0;
        assert_eq!(result == Coverage::Orphaned, !installed && named);
        assert_eq!(result == Coverage::Missing, installed && !named);
        assert_eq!(
            result == Coverage::Contradicted,
            installed && ((skills > 0 && decisions > 0) || decisions > 1)
        );
        assert_eq!(
            result == Coverage::Stale,
            installed && skills > 0 && decisions == 0 && !current
        );
        assert_eq!(
            result == Coverage::Covered,
            if installed {
                (skills > 0 && decisions == 0 && current) || (skills == 0 && decisions == 1)
            } else {
                !named
            }
        );
    }

    /// A platform-specific skill installs exactly where its tool does, and any other skill installs everywhere.
    pub fn spec_placement(specific: bool, restricted: bool, exact: bool) {
        let result = placement(specific, restricted, exact);
        assert_eq!(
            result == Placement::Correct,
            (specific && exact) || (!specific && !restricted)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_tool_passes_only_with_one_current_skill_or_one_decision() {
        let installed: bool = kani::any();
        let current: bool = kani::any();
        let skills: usize = kani::any();
        let decisions: usize = kani::any();
        kani::assume(skills <= 3 && decisions <= 3);
        spec_coverage(installed, skills, current, decisions);
        let result = coverage(installed, skills, current, decisions);
        kani::cover!(result == Coverage::Covered && installed);
        kani::cover!(result == Coverage::Stale);
        kani::cover!(result == Coverage::Missing);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_platform_specific_skill_installs_only_where_its_tool_does() {
        let (specific, restricted, exact): (bool, bool, bool) =
            (kani::any(), kani::any(), kani::any());
        spec_placement(specific, restricted, exact);
        let result = placement(specific, restricted, exact);
        kani::cover!(result == Placement::Correct && specific);
        kani::cover!(result == Placement::Misplaced && !specific);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specifications() {
        for [installed, current] in crate::domain::flags() {
            for skills in 0..3 {
                for decisions in 0..3 {
                    proofs::spec_coverage(installed, skills, current, decisions);
                }
            }
        }
        for [specific, restricted, exact] in crate::domain::flags() {
            proofs::spec_placement(specific, restricted, exact);
        }
        assert_eq!(coverage(true, 0, true, 0), Coverage::Missing);
        assert_eq!(coverage(true, 0, true, 1), Coverage::Covered);
        assert_eq!(coverage(true, 1, false, 0), Coverage::Stale);
        assert_eq!(placement(true, true, true), Placement::Correct);
    }
}
