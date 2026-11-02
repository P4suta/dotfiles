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

/// The command that admits work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Operation {
    Start,
    Create,
    StackCreate,
}

/// How the declaration `start` stored for a branch relates to the work an operation admits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum StoredStart {
    /// No declaration is pending, because `start` stored none or it already names its PR.
    Absent,
    /// It names another login, destination, source, branch, or issue.
    OtherWork,
    /// It names this work, but the head doesn't descend from its starting commit.
    LeftHistory,
    /// It names this work, and the head descends from its starting commit.
    Descendant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Carry {
    Declared,
    Undeclared,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaleStart {
    OtherWork,
    LeftHistory,
}

/// Whether a pending `start` declaration admits this work as independent.
/// A later `start` or `create` carries it for the same work on a descendant commit, and `stack create` joins its named stack instead.
/// A later `start` treats any other declaration as absent, so only work that the stack would refuse needs a new one.
///
/// # Errors
/// `create` refuses a declaration for other work or another history, because it would otherwise publish the work without it.
pub const fn carried_start(operation: Operation, stored: StoredStart) -> Result<Carry, StaleStart> {
    match (operation, stored) {
        (Operation::Start | Operation::Create, StoredStart::Descendant) => Ok(Carry::Declared),
        (Operation::Create, StoredStart::OtherWork) => Err(StaleStart::OtherWork),
        (Operation::Create, StoredStart::LeftHistory) => Err(StaleStart::LeftHistory),
        (Operation::Start, StoredStart::OtherWork | StoredStart::LeftHistory)
        | (Operation::Start | Operation::Create, StoredStart::Absent)
        | (
            Operation::StackCreate,
            StoredStart::Absent
            | StoredStart::OtherWork
            | StoredStart::LeftHistory
            | StoredStart::Descendant,
        ) => Ok(Carry::Undeclared),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkFacts {
    pub members: usize,
    pub distinct: bool,
    pub owned_open: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidLink {
    TooShort,
    Duplicate,
    NotOwnedOpen,
}

/// Check the selected membership before changing any remote parent or local declaration.
///
/// # Errors
/// Refuses fewer than two members, duplicate members, or members outside the owned open inventory.
pub const fn link_admission(facts: LinkFacts) -> Result<(), InvalidLink> {
    const MIN_LINK_MEMBERS: usize = 2;
    if facts.members < MIN_LINK_MEMBERS {
        Err(InvalidLink::TooShort)
    } else if !facts.distinct {
        Err(InvalidLink::Duplicate)
    } else if !facts.owned_open {
        Err(InvalidLink::NotOwnedOpen)
    } else {
        Ok(())
    }
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

    pub fn spec_carried_start(operation: Operation, stored: StoredStart) {
        let result = carried_start(operation, stored);
        assert_eq!(
            result == Ok(Carry::Declared),
            operation != Operation::StackCreate && stored == StoredStart::Descendant
        );
        match result {
            Ok(_) => assert!(operation != Operation::Create || !stale(stored)),
            Err(StaleStart::OtherWork) => {
                assert!(operation == Operation::Create && stored == StoredStart::OtherWork);
            }
            Err(StaleStart::LeftHistory) => {
                assert!(operation == Operation::Create && stored == StoredStart::LeftHistory);
            }
        }
    }

    fn stale(stored: StoredStart) -> bool {
        matches!(stored, StoredStart::OtherWork | StoredStart::LeftHistory)
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_the_same_work_on_a_descendant_commit_carries_a_start_declaration() {
        let operation: Operation = kani::any();
        let stored: StoredStart = kani::any();
        spec_carried_start(operation, stored);
        let result = carried_start(operation, stored);
        kani::cover!(operation == Operation::Start && result == Ok(Carry::Declared));
        kani::cover!(operation == Operation::Start && stale(stored) && result.is_ok());
        kani::cover!(result == Err(StaleStart::OtherWork));
        kani::cover!(result == Err(StaleStart::LeftHistory));
        kani::cover!(
            operation == Operation::StackCreate
                && stored == StoredStart::Descendant
                && result == Ok(Carry::Undeclared)
        );
    }

    pub fn spec_link_admission(facts: LinkFacts) {
        let result = link_admission(facts);
        assert_eq!(
            result.is_ok(),
            facts.members >= 2 && facts.distinct && facts.owned_open
        );
        match result {
            Ok(()) => {}
            Err(InvalidLink::TooShort) => assert!(facts.members < 2),
            Err(InvalidLink::Duplicate) => assert!(facts.members >= 2 && !facts.distinct),
            Err(InvalidLink::NotOwnedOpen) => {
                assert!(facts.members >= 2 && facts.distinct && !facts.owned_open);
            }
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(2)]
    fn stack_link_admission_requires_every_membership_fact() {
        let facts = LinkFacts {
            members: kani::any(),
            distinct: kani::any(),
            owned_open: kani::any(),
        };
        spec_link_admission(facts);
        kani::cover!(link_admission(facts).is_ok() && facts.members == 2);
        kani::cover!(link_admission(facts).is_ok() && facts.members == usize::MAX);
        kani::cover!(link_admission(facts) == Err(InvalidLink::Duplicate));
        kani::cover!(link_admission(facts) == Err(InvalidLink::NotOwnedOpen));
    }

    pub fn spec_workload(members: &[StackMember]) -> Result<Workload, InvalidWork> {
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
        result
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
    fn every_graph_of_at_most_one_member_matches_parent_path_validation() {
        let empty = spec_workload(&[]);
        assert!(empty.is_ok_and(|work| work.units() == 0 && work.tip().is_none()));
        let members: [StackMember; 1] = kani::any();
        let result = spec_workload(&members);
        kani::cover!(result.is_ok());
        kani::cover!(result.is_err());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(12)]
    fn every_graph_of_two_members_matches_parent_path_validation() {
        let members: [StackMember; 2] = kani::any();
        let result = spec_workload(&members);
        kani::cover!(result.is_ok_and(|work| work.units() == 2 && work.tip().is_some()));
        kani::cover!(result.is_err());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(12)]
    fn every_graph_of_three_members_matches_parent_path_validation() {
        let members: [StackMember; 3] = kani::any();
        let result = spec_workload(&members);
        kani::cover!(result.is_ok_and(|work| work.units() == 2 && work.tip().is_some()));
        kani::cover!(result.is_err());
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
    fn stack_linking_requires_a_distinct_owned_open_selection() {
        for members in [0, 1, 2, 3, usize::MAX] {
            for distinct in [true, false] {
                for owned_open in [true, false] {
                    let facts = LinkFacts {
                        members,
                        distinct,
                        owned_open,
                    };
                    proofs::spec_link_admission(facts);
                }
            }
        }
    }

    #[test]
    fn every_operation_and_stored_start_meets_the_specification() {
        for operation in [Operation::Start, Operation::Create, Operation::StackCreate] {
            for stored in [
                StoredStart::Absent,
                StoredStart::OtherWork,
                StoredStart::LeftHistory,
                StoredStart::Descendant,
            ] {
                proofs::spec_carried_start(operation, stored);
            }
        }
    }

    #[test]
    fn a_start_refresh_carries_its_own_declaration_and_ignores_any_other() {
        assert_eq!(
            carried_start(Operation::Start, StoredStart::Descendant),
            Ok(Carry::Declared)
        );
        for stored in [
            StoredStart::Absent,
            StoredStart::OtherWork,
            StoredStart::LeftHistory,
        ] {
            assert_eq!(
                carried_start(Operation::Start, stored),
                Ok(Carry::Undeclared)
            );
        }
        assert_eq!(
            carried_start(Operation::Create, StoredStart::OtherWork),
            Err(StaleStart::OtherWork)
        );
    }

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
                        let _ = proofs::spec_workload(&members[..length]);
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
        let _ = proofs::spec_workload(&members);
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
