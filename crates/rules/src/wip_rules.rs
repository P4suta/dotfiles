/// The number of open work units one person may keep in a repository.
/// The limit comes from the recorded override of the repository, else the policy default.
#[must_use]
pub const fn limit(default: u32, repository: Option<u32>) -> u32 {
    match repository {
        Some(limit) => limit,
        None => default,
    }
}

/// A new work unit fits only while the current count stays below the limit.
#[must_use]
pub const fn admits(open: u32, limit: u32) -> bool {
    open < limit
}

#[must_use]
pub const fn admits_creation(units: u32, limit: u32, joins_stack: bool) -> bool {
    joins_stack || admits(units, limit)
}

/// A parent names another member's position in the same live inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct StackMember {
    pub parent: Option<usize>,
    pub independent: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidWork {
    MissingParent,
    IndependentLink,
    MultipleStacks,
    ForkedStack,
    Cycle,
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Workload {
    units: u32,
    tip: Option<usize>,
}

impl Workload {
    #[must_use]
    pub const fn units(self) -> u32 {
        self.units
    }

    #[must_use]
    pub const fn tip(self) -> Option<usize> {
        self.tip
    }
}

/// Validate one linear stack and isolated independent PRs before counting work units.
///
/// # Errors
/// Refuses missing parents, linked independent work, branches, cycles, extra stacks, and overflowing counts.
pub fn workload(members: &[StackMember]) -> Result<Workload, InvalidWork> {
    let mut root = None;
    for (index, member) in members.iter().enumerate() {
        if let Some(parent) = member.parent {
            let Some(parent_member) = members.get(parent) else {
                return Err(InvalidWork::MissingParent);
            };
            if member.independent || parent_member.independent {
                return Err(InvalidWork::IndependentLink);
            }
            if parent == index {
                return Err(InvalidWork::Cycle);
            }
        } else if !member.independent && root.replace(index).is_some() {
            return Err(InvalidWork::MultipleStacks);
        }
        if members
            .iter()
            .filter(|child| child.parent == Some(index))
            .count()
            > 1
        {
            return Err(InvalidWork::ForkedStack);
        }
    }
    let independent = members.iter().filter(|member| member.independent).count();
    let mut tip = root;
    let mut visited = 0;
    for _ in members {
        let Some(current) = tip else { break };
        visited += 1;
        let child = members
            .iter()
            .position(|member| member.parent == Some(current));
        if child.is_none() {
            break;
        }
        tip = child;
    }
    if visited != members.len() - independent {
        return Err(InvalidWork::Cycle);
    }
    let units = u32::try_from(independent)
        .ok()
        .and_then(|count| count.checked_add(u32::from(root.is_some())))
        .ok_or(InvalidWork::Overflow)?;
    Ok(Workload { units, tip })
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_limit(default: u32, repository: u32) {
        assert_eq!(limit(default, Some(repository)), repository);
        assert_eq!(limit(default, None), default);
    }

    pub fn spec_admits(open: u32, limit: u32) {
        let admitted = admits(open, limit);
        assert_eq!(admitted, open < limit);
        assert!(!admitted || open.saturating_add(1) <= limit);
    }

    pub fn spec_creation(units: u32, limit: u32, joins_stack: bool) {
        let admitted = admits_creation(units, limit, joins_stack);
        assert_eq!(admitted, joins_stack || units < limit);
        assert!(joins_stack || !admitted || units.saturating_add(1) <= limit);
    }

    pub fn spec_workload(members: &[StackMember]) {
        let parents = members.iter().all(|member| match member.parent {
            None => true,
            Some(parent) => {
                !member.independent
                    && members
                        .get(parent)
                        .is_some_and(|parent| !parent.independent)
            }
        });
        let roots = members
            .iter()
            .filter(|member| !member.independent && member.parent.is_none())
            .count();
        let linear = (0..members.len()).all(|index| {
            members
                .iter()
                .filter(|member| member.parent == Some(index))
                .count()
                <= 1
        });
        let acyclic = (0..members.len()).all(|index| {
            let mut parent = Some(index);
            for _ in members {
                parent = parent
                    .and_then(|index| members.get(index))
                    .and_then(|member| member.parent);
            }
            parent.is_none()
        });
        let independent = members.iter().filter(|member| member.independent).count();
        let expected = u32::try_from(independent + roots).ok();
        let result = workload(members);
        assert_eq!(
            result.is_ok(),
            parents && roots <= 1 && linear && acyclic && expected.is_some()
        );
        if let Ok(work) = result {
            assert_eq!(Some(work.units()), expected);
            assert_eq!(work.tip().is_some(), roots != 0);
            if let Some(tip) = work.tip() {
                assert!(members.get(tip).is_some_and(|member| !member.independent));
                assert!(!members.iter().any(|member| member.parent == Some(tip)));
            }
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn stack_members_never_consume_an_additional_work_unit() {
        let units: u32 = kani::any();
        let limit: u32 = kani::any();
        let joins_stack: bool = kani::any();
        spec_creation(units, limit, joins_stack);
        kani::cover!(joins_stack && units >= limit && admits_creation(units, limit, joins_stack));
        kani::cover!(!joins_stack && !admits_creation(units, limit, joins_stack));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(12)]
    fn every_graph_of_up_to_three_members_matches_parent_path_validation() {
        let members: [StackMember; 3] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= members.len());
        let selected = &members[..length];
        spec_workload(selected);
        kani::cover!(
            workload(selected).is_ok_and(|work| work.units() == 2 && work.tip().is_some())
        );
        kani::cover!(workload(selected).is_err());
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_recorded_repository_limit_replaces_the_default() {
        let default: u32 = kani::any();
        let repository: u32 = kani::any();
        spec_limit(default, repository);
        kani::cover!(repository < default);
        kani::cover!(repository > default);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_new_work_unit_is_created_only_below_the_limit() {
        let open: u32 = kani::any();
        let limit: u32 = kani::any();
        spec_admits(open, limit);
        kani::cover!(admits(open, limit));
        kani::cover!(!admits(open, limit) && open == limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDARIES: [u32; 5] = [0, 1, 2, u32::MAX - 1, u32::MAX];

    #[test]
    fn stack_members_at_the_limit_keep_the_existing_unit() {
        for units in BOUNDARIES {
            for limit in BOUNDARIES {
                for joins_stack in [false, true] {
                    proofs::spec_creation(units, limit, joins_stack);
                }
            }
        }
    }

    #[test]
    fn all_small_graphs_meet_the_independent_parent_path_specification() {
        let choices: [StackMember; 10] = core::array::from_fn(|index| StackMember {
            parent: (index / 2).checked_sub(1),
            independent: index % 2 != 0,
        });
        for first in choices {
            for second in choices {
                for third in choices {
                    let members = [first, second, third];
                    for length in 0..=members.len() {
                        proofs::spec_workload(&members[..length]);
                    }
                }
            }
        }
    }

    #[test]
    fn a_long_stack_and_an_independent_pr_count_as_two_units() {
        let mut members = [StackMember {
            parent: None,
            independent: false,
        }; 33];
        for (index, member) in members.iter_mut().enumerate().skip(1) {
            member.parent = Some(index - 1);
        }
        members[32] = StackMember {
            parent: None,
            independent: true,
        };
        proofs::spec_workload(&members);
        let work = workload(&members).unwrap();
        assert_eq!(work.units(), 2);
        assert_eq!(work.tip(), Some(31));
    }

    #[test]
    fn every_boundary_meets_the_specification() {
        for first in BOUNDARIES {
            for second in BOUNDARIES {
                proofs::spec_limit(first, second);
                proofs::spec_admits(first, second);
            }
        }
    }

    #[test]
    fn a_limit_of_one_refuses_a_second_work_unit() {
        let limit = limit(1, None);
        assert!(admits(0, limit));
        assert!(!admits(1, limit));
        assert!(!admits(2, limit));
    }

    #[test]
    fn a_repository_override_decides_instead_of_the_default() {
        assert_eq!(limit(1, Some(3)), 3);
        assert!(admits(2, limit(1, Some(3))));
        assert!(!admits(3, limit(1, Some(3))));
        assert!(!admits(0, limit(1, Some(0))));
    }
}
