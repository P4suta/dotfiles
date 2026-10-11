//! The skill load plan: the requested skills and every skill they rely on, each after what it relies on.
//! `specs/lean/Dotfiles/SkillPlan.lean` defines the same algorithm step for step and proves `plan_correct` for every catalog.
//! Each skill goes by its index in the catalog, and `deps[skill]` lists the skills it relies on.

extern crate alloc;
use self::alloc::vec::Vec;

/// Why a plan fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Failure {
    /// A requested skill or a dependency names an index outside the catalog.
    Unknown,
    /// The skills rely on one another in a cycle.
    Cycle,
}

/// The skills `skill` relies on, none for an index outside the catalog.
fn needs<'a>(deps: &'a [&'a [usize]], skill: usize) -> &'a [usize] {
    deps.get(skill).copied().unwrap_or_default()
}

fn contains(skills: &[usize], skill: usize) -> bool {
    for index in 0..skills.len() {
        if skills.get(index).copied() == Some(skill) {
            return true;
        }
    }
    false
}

/// Whether every skill `skill` relies on already lies in `placed`.
fn ready(deps: &[&[usize]], placed: &[usize], skill: usize) -> bool {
    let dependencies = needs(deps, skill);
    for index in 0..dependencies.len() {
        if dependencies
            .get(index)
            .is_some_and(|dependency| !contains(placed, *dependency))
        {
            return false;
        }
    }
    true
}

/// Places the first pending skill whose dependencies all lie in `placed`, until none remains or none can go next.
fn order(deps: &[&[usize]], pending: &mut [usize]) -> bool {
    // Each round places one skill, so as many rounds as pending skills always end the loop.
    for placed in 0..pending.len() {
        let before = pending.get(..placed).unwrap_or_default();
        let mut selected = None;
        for position in placed..pending.len() {
            if pending
                .get(position)
                .is_some_and(|skill| ready(deps, before, *skill))
            {
                selected = Some(position);
                break;
            }
        }
        let Some(position) = selected else {
            return false;
        };
        // Moving the ready skill to the front keeps every remaining skill in its declaration order.
        for index in (placed + 1..=position).rev() {
            pending.swap(index - 1, index);
        }
    }
    true
}

/// The skills of the ordered list `full` that a requested skill needs, found from the last skill back.
fn needed(deps: &[&[usize]], requested: &[usize], full: &mut [usize]) -> usize {
    let mut start = full.len();
    for index in (0..full.len()).rev() {
        let skill = full.get(index).copied().unwrap_or_default();
        let mut keep = contains(requested, skill);
        for later in start..full.len() {
            if full
                .get(later)
                .is_some_and(|needer| contains(needs(deps, *needer), skill))
            {
                keep = true;
                break;
            }
        }
        if keep {
            start -= 1;
            if let Some(slot) = full.get_mut(start) {
                *slot = skill;
            }
        }
    }
    let length = full.len() - start;
    for index in 0..length {
        let skill = full.get(start + index).copied().unwrap_or_default();
        if let Some(slot) = full.get_mut(index) {
            *slot = skill;
        }
    }
    length
}

fn unknown(deps: &[&[usize]], requested: &[usize]) -> bool {
    for index in 0..requested.len() {
        if requested
            .get(index)
            .is_some_and(|skill| *skill >= deps.len())
        {
            return true;
        }
    }
    for skill in 0..deps.len() {
        let dependencies = needs(deps, skill);
        for index in 0..dependencies.len() {
            if dependencies
                .get(index)
                .is_some_and(|dependency| *dependency >= deps.len())
            {
                return true;
            }
        }
    }
    false
}

const WORD_SKILLS: usize = u8::BITS as usize;

#[derive(Clone, Copy, Debug)]
struct WordGraph {
    count: usize,
    edges: [u8; WORD_SKILLS],
}

#[derive(Clone, Copy, Debug)]
struct WordPlan {
    skills: [usize; WORD_SKILLS],
    length: usize,
}

/// A catalog and its requested skills, validated together before planning.
#[derive(Clone, Copy, Debug)]
pub struct Input<'a>(InputKind<'a>);

#[derive(Clone, Copy, Debug)]
enum InputKind<'a> {
    Word(WordGraph, u8),
    Slices(&'a [&'a [usize]], &'a [usize]),
}

/// An ordered set of requested skills and their dependencies.
#[derive(Debug)]
pub struct Plan(PlanKind);

#[derive(Debug)]
enum PlanKind {
    Word(WordPlan),
    Slices(Vec<usize>),
}

impl Plan {
    /// The skill indices in their dependency order.
    #[must_use]
    pub fn skills(&self) -> &[usize] {
        match &self.0 {
            PlanKind::Word(plan) => plan.skills.get(..plan.length).unwrap_or_default(),
            PlanKind::Slices(plan) => plan,
        }
    }
}

fn word_members(count: usize) -> u8 {
    if count >= WORD_SKILLS {
        u8::MAX
    } else {
        (1u8 << count) - 1
    }
}

fn word_unknown(graph: &WordGraph, requested: u8) -> bool {
    let members = word_members(graph.count);
    if requested & !members != 0 {
        return true;
    }
    for skill in 0..graph.count {
        if graph
            .edges
            .get(skill)
            .is_some_and(|edges| *edges & !members != 0)
        {
            return true;
        }
    }
    false
}

#[cfg(any(test, kani))]
fn word_valid(graph: &WordGraph, requested: u8, plan: &WordPlan) -> bool {
    if plan.length > graph.count {
        return false;
    }
    let mut seen = 0u8;
    for index in 0..plan.length {
        let skill = plan.skills.get(index).copied().unwrap_or(WORD_SKILLS);
        if skill >= graph.count {
            return false;
        }
        let bit = 1u8 << skill;
        if seen & bit != 0
            || graph
                .edges
                .get(skill)
                .is_some_and(|edges| *edges & !seen != 0)
        {
            return false;
        }
        seen |= bit;
    }
    requested & !seen == 0
}

#[cfg(any(test, kani))]
fn word_cyclic(graph: &WordGraph) -> bool {
    let mut reach = graph.edges;
    for middle in 0..graph.count {
        let via = reach.get(middle).copied().unwrap_or_default();
        for skill in 0..graph.count {
            if let Some(row) = reach.get_mut(skill)
                && *row & (1u8 << middle) != 0
            {
                *row |= via;
            }
        }
    }
    for skill in 0..graph.count {
        if reach
            .get(skill)
            .is_some_and(|row| *row & (1u8 << skill) != 0)
        {
            return true;
        }
    }
    false
}

fn word_plan(graph: WordGraph, mut requested: u8) -> Result<WordPlan, Failure> {
    if graph.count > WORD_SKILLS || word_unknown(&graph, requested) {
        return Err(Failure::Unknown);
    }
    let mut plan = WordPlan {
        skills: [0; WORD_SKILLS],
        length: 0,
    };
    let mut placed = 0u8;
    for position in 0..graph.count {
        let mut selected = None;
        for skill in 0..graph.count {
            let edges = graph.edges.get(skill).copied().unwrap_or_default();
            if placed & (1u8 << skill) == 0 && edges & !placed == 0 {
                selected = Some(skill);
                break;
            }
        }
        let Some(skill) = selected else {
            return Err(Failure::Cycle);
        };
        if let Some(slot) = plan.skills.get_mut(position) {
            *slot = skill;
        }
        placed |= 1u8 << skill;
    }
    for index in (0..graph.count).rev() {
        let skill = plan.skills.get(index).copied().unwrap_or_default();
        if requested & (1u8 << skill) != 0 {
            requested |= graph.edges.get(skill).copied().unwrap_or_default();
        }
    }
    for index in 0..graph.count {
        let skill = plan.skills.get(index).copied().unwrap_or_default();
        if requested & (1u8 << skill) != 0 {
            if let Some(slot) = plan.skills.get_mut(plan.length) {
                *slot = skill;
            }
            plan.length += 1;
        }
    }
    Ok(plan)
}

fn plan_into(input: Input<'_>, out: &mut [usize]) -> Result<usize, Failure> {
    match input.0 {
        InputKind::Word(graph, requested) => {
            if out.len() != graph.count {
                return Err(Failure::Unknown);
            }
            let plan = word_plan(graph, requested)?;
            for index in 0..plan.length {
                if let Some(slot) = out.get_mut(index) {
                    *slot = plan.skills.get(index).copied().unwrap_or_default();
                }
            }
            Ok(plan.length)
        }
        InputKind::Slices(deps, requested) => slice_plan(deps, requested, out),
    }
}

fn slice_plan(deps: &[&[usize]], requested: &[usize], out: &mut [usize]) -> Result<usize, Failure> {
    if out.len() != deps.len() || unknown(deps, requested) {
        return Err(Failure::Unknown);
    }
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = index;
    }
    if !order(deps, out) {
        return Err(Failure::Cycle);
    }
    Ok(needed(deps, requested, out))
}

/// Validates a catalog and its requests, packing small catalogs into dependency sets.
///
/// # Errors
///
/// Returns `Unknown` when a requested skill or dependency lies outside the catalog.
pub fn input<'a>(deps: &'a [&'a [usize]], requested: &'a [usize]) -> Result<Input<'a>, Failure> {
    if unknown(deps, requested) {
        return Err(Failure::Unknown);
    }
    if deps.len() <= WORD_SKILLS {
        let mut graph = WordGraph {
            count: deps.len(),
            edges: [0; WORD_SKILLS],
        };
        for skill in 0..deps.len() {
            let dependencies = needs(deps, skill);
            let mut edges = 0;
            for index in 0..dependencies.len() {
                if let Some(dependency) = dependencies.get(index) {
                    edges |= 1u8 << dependency;
                }
            }
            if let Some(row) = graph.edges.get_mut(skill) {
                *row = edges;
            }
        }
        let mut wanted = 0;
        for index in 0..requested.len() {
            if let Some(skill) = requested.get(index) {
                wanted |= 1u8 << skill;
            }
        }
        return Ok(Input(InputKind::Word(graph, wanted)));
    }
    Ok(Input(InputKind::Slices(deps, requested)))
}

/// Orders every catalog skill, then retains the skills the validated requests need.
///
/// # Errors
///
/// Returns `Cycle` when the skills rely on one another in a cycle.
pub fn plan(input: Input<'_>) -> Result<Plan, Failure> {
    let (deps, requested) = match input.0 {
        InputKind::Word(graph, requested) => {
            let plan = word_plan(graph, requested)?;
            return Ok(Plan(PlanKind::Word(plan)));
        }
        InputKind::Slices(deps, requested) => (deps, requested),
    };
    let mut out = alloc::vec![0; deps.len()];
    let length = plan_into(Input(InputKind::Slices(deps, requested)), &mut out)?;
    out.truncate(length);
    Ok(Plan(PlanKind::Slices(out)))
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// The most skills the bounded checks hold.
    pub const SKILLS: usize = 5;

    fn lean_plan<const CAPACITY: usize>(
        graph: WordGraph,
        requested: u8,
    ) -> Result<WordPlan, Failure> {
        assert!(CAPACITY <= WORD_SKILLS);
        assert!(graph.count <= CAPACITY);
        if word_unknown(&graph, requested) {
            return Err(Failure::Unknown);
        }
        let mut pending = [0u8; CAPACITY];
        for index in 0..graph.count {
            if let Some(slot) = pending.get_mut(index) {
                *slot = u8::try_from(index).unwrap();
            }
        }
        let mut remaining = graph.count;
        let mut full = [0u8; CAPACITY];
        let mut placed = 0u8;
        for round in 0..graph.count {
            let mut first = None;
            for position in 0..remaining {
                let skill = pending.get(position).copied().unwrap_or_default();
                if graph
                    .edges
                    .get(usize::from(skill))
                    .is_some_and(|edges| *edges & !placed == 0)
                {
                    first = Some(position);
                    break;
                }
            }
            let Some(position) = first else {
                return Err(Failure::Cycle);
            };
            let skill = pending.get(position).copied().unwrap_or_default();
            if let Some(slot) = full.get_mut(round) {
                *slot = skill;
            }
            placed |= 1u8 << skill;
            for index in position + 1..remaining {
                let skill = pending.get(index).copied().unwrap_or_default();
                if let Some(slot) = pending.get_mut(index - 1) {
                    *slot = skill;
                }
            }
            remaining -= 1;
        }
        let mut needed = [0u8; CAPACITY];
        let mut start = graph.count;
        for position in (0..graph.count).rev() {
            let skill = full.get(position).copied().unwrap_or_default();
            let mut keep = requested & (1u8 << skill) != 0;
            for later in start..graph.count {
                let dependent = needed.get(later).copied().unwrap_or_default();
                if graph
                    .edges
                    .get(usize::from(dependent))
                    .is_some_and(|edges| *edges & (1u8 << skill) != 0)
                {
                    keep = true;
                }
            }
            if keep {
                start -= 1;
                if let Some(slot) = needed.get_mut(start) {
                    *slot = skill;
                }
            }
        }
        let length = graph.count - start;
        for index in 0..length {
            let skill = needed.get(start + index).copied().unwrap_or_default();
            if let Some(slot) = needed.get_mut(index) {
                *slot = skill;
            }
        }
        let mut out = WordPlan {
            skills: [0; WORD_SKILLS],
            length,
        };
        for index in 0..CAPACITY {
            if let Some(slot) = out.skills.get_mut(index) {
                *slot = usize::from(needed.get(index).copied().unwrap_or_default());
            }
        }
        Ok(out)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_five_skill_plan_equals_the_lean_algorithm() {
        let actual = bounded_lean_equality::<0>();
        kani::cover!(actual.as_ref().is_ok_and(|out| out.skills().len() == 3));
        kani::cover!(matches!(actual, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_five_skill_plan_first_skill_equals_the_lean_algorithm() {
        let actual = bounded_lean_equality::<1>();
        kani::cover!(actual.as_ref().is_ok_and(|out| out.skills().len() == 3));
        kani::cover!(matches!(actual, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_five_skill_plan_second_skill_equals_the_lean_algorithm() {
        let actual = bounded_lean_equality::<2>();
        kani::cover!(actual.as_ref().is_ok_and(|out| out.skills().len() == 3));
        kani::cover!(matches!(actual, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_five_skill_plan_third_skill_equals_the_lean_algorithm() {
        let actual = bounded_lean_equality::<3>();
        kani::cover!(actual.as_ref().is_ok_and(|out| out.skills().len() == 3));
        kani::cover!(matches!(actual, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_five_skill_plan_fourth_skill_equals_the_lean_algorithm() {
        let actual = bounded_lean_equality::<4>();
        kani::cover!(actual.as_ref().is_ok_and(|out| out.skills().len() == 3));
        kani::cover!(matches!(actual, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_five_skill_plan_fifth_skill_equals_the_lean_algorithm() {
        let actual = bounded_lean_equality::<5>();
        kani::cover!(matches!(actual, Err(Failure::Unknown)));
        kani::cover!(matches!(actual, Err(Failure::Cycle)));
        kani::cover!(actual.as_ref().is_ok_and(|out| out.skills().len() == 5));
    }

    #[cfg(kani)]
    fn bounded_lean_equality<const PROPERTY: usize>() -> Result<Plan, Failure> {
        let mut graph = WordGraph {
            count: 5,
            edges: [0; WORD_SKILLS],
        };
        for skill in 0..SKILLS {
            if let Some(row) = graph.edges.get_mut(skill) {
                *row = kani::any::<u8>() % 32;
            }
        }
        let requested = kani::any::<u8>() % 64;
        let actual = plan(Input(InputKind::Word(graph, requested)));
        let reference = lean_plan::<SKILLS>(graph, requested);
        match (&actual, &reference) {
            (Ok(actual), Ok(reference)) => {
                let actual = actual.skills();
                if PROPERTY == 0 {
                    assert_eq!(actual.len(), reference.length);
                } else {
                    let index = PROPERTY - 1;
                    if actual.len() > index {
                        assert!(reference.length > index);
                        assert_eq!(actual.get(index), reference.skills.get(index));
                    }
                }
            }
            (Err(actual), Err(reference)) => assert_eq!(actual, reference),
            _ => assert_eq!(actual.is_ok(), reference.is_ok()),
        }
        actual
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn the_word_plan_of_five_skills_meets_plan_correct() {
        let mut graph = WordGraph {
            count: 5,
            edges: [0; WORD_SKILLS],
        };
        for skill in 0..SKILLS {
            if let Some(row) = graph.edges.get_mut(skill) {
                *row = kani::any::<u8>() % 32;
            }
        }
        let requested = kani::any::<u8>() % 64;
        let result = word_plan(graph, requested);
        match &result {
            Ok(plan) => assert!(word_valid(&graph, requested, plan)),
            Err(Failure::Unknown) => assert!(word_unknown(&graph, requested)),
            Err(Failure::Cycle) => assert!(word_cyclic(&graph)),
        }
        kani::cover!(result.as_ref().is_ok_and(|plan| plan.length == 3));
        kani::cover!(matches!(result, Err(Failure::Cycle)));
    }

    /// The catalog that `masks` describes, one bit for each skill a skill relies on.
    pub fn catalog(masks: &[u8]) -> Vec<Vec<usize>> {
        masks
            .iter()
            .map(|mask| {
                (0..SKILLS)
                    .filter(|skill| mask & (1 << skill) != 0)
                    .collect()
            })
            .collect()
    }

    /// Whether some nonempty set of skills holds, for each member, another member it relies on.
    pub(super) fn cyclic(deps: &[&[usize]]) -> bool {
        assert!(deps.len() <= SKILLS);
        let mut reach = [0u8; SKILLS];
        for skill in 0..deps.len() {
            let dependencies = needs(deps, skill);
            for index in 0..dependencies.len() {
                if let Some(dependency) = dependencies.get(index)
                    && *dependency < deps.len()
                    && let Some(row) = reach.get_mut(skill)
                {
                    *row |= 1u8 << dependency;
                }
            }
        }
        for middle in 0..deps.len() {
            let via = reach.get(middle).copied().unwrap_or_default();
            for skill in 0..deps.len() {
                if let Some(row) = reach.get_mut(skill)
                    && *row & (1u8 << middle) != 0
                {
                    *row |= via;
                }
            }
        }
        for skill in 0..deps.len() {
            if reach
                .get(skill)
                .is_some_and(|row| *row & (1u8 << skill) != 0)
            {
                return true;
            }
        }
        false
    }

    /// A plan holds each requested skill once, after every skill it relies on, and a refusal names an unknown skill or a cycle, as `plan_correct` states.
    pub fn spec_plan(deps: &[&[usize]], requested: &[usize]) -> Result<Vec<usize>, Failure> {
        assert!(deps.len() <= SKILLS);
        let planned = input(deps, requested)
            .and_then(plan)
            .map(|plan| plan.skills().to_vec());
        let cycle = matches!(planned, Err(Failure::Cycle)) && cyclic(deps);
        match &planned {
            Ok(out) => {
                let mut seen = 0u8;
                for index in 0..out.len() {
                    let skill = out.get(index).copied().unwrap_or_default();
                    assert!(skill < deps.len());
                    assert_eq!(seen & (1u8 << skill), 0);
                    let dependencies = needs(deps, skill);
                    for dependency in 0..dependencies.len() {
                        let dependency = dependencies.get(dependency).copied().unwrap_or_default();
                        assert_ne!(seen & (1u8 << dependency), 0);
                    }
                    seen |= 1u8 << skill;
                }
                for request in 0..requested.len() {
                    let skill = requested.get(request).copied().unwrap_or_default();
                    assert_ne!(seen & (1u8 << skill), 0);
                }
                #[cfg(test)]
                if deps.len() <= WORD_SKILLS {
                    let mut graph = WordGraph {
                        count: deps.len(),
                        edges: [0; WORD_SKILLS],
                    };
                    for (skill, dependencies) in deps.iter().enumerate() {
                        for dependency in *dependencies {
                            if let Some(row) = graph.edges.get_mut(skill) {
                                *row |= 1u8 << dependency;
                            }
                        }
                    }
                    let wanted = requested
                        .iter()
                        .fold(0u8, |bits, skill| bits | (1u8 << skill));
                    let word = word_plan(graph, wanted).unwrap_or(WordPlan {
                        skills: [0; WORD_SKILLS],
                        length: WORD_SKILLS,
                    });
                    assert!(word_valid(&graph, wanted, &word));
                    assert!(!word_cyclic(&graph));
                    assert_eq!(
                        word.skills.get(..word.length).unwrap_or_default(),
                        out.as_slice()
                    );
                    let reference = lean_plan::<WORD_SKILLS>(graph, wanted)
                        .map(|plan| plan.skills.get(..plan.length).unwrap_or_default().to_vec());
                    assert_eq!(reference, planned);
                }
            }
            Err(Failure::Unknown) => {
                assert!(unknown(deps, requested));
            }
            Err(Failure::Cycle) => {
                assert!(cycle);
            }
        }
        if planned.is_err() && requested.iter().all(|skill| *skill < deps.len()) {
            assert!(matches!(planned, Err(Failure::Unknown)) || cycle);
        }
        planned
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_plan_of_five_skills_meets_plan_correct() {
        let planned = bounded_plan::<5>();
        kani::cover!(planned.as_ref().is_ok_and(|out| out.skills().len() == 3));
        kani::cover!(matches!(planned, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_plan_of_four_skills_meets_plan_correct() {
        let planned = bounded_plan::<4>();
        kani::cover!(planned.as_ref().is_ok_and(|out| out.skills().len() == 3));
        kani::cover!(matches!(planned, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_plan_of_three_skills_meets_plan_correct() {
        let planned = bounded_plan::<3>();
        kani::cover!(planned.as_ref().is_ok_and(|out| out.skills().len() == 3));
        kani::cover!(matches!(planned, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_plan_of_two_skills_meets_plan_correct() {
        let planned = bounded_plan::<2>();
        kani::cover!(planned.as_ref().is_ok_and(|out| out.skills().len() == 2));
        kani::cover!(matches!(planned, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_plan_of_one_skill_meets_plan_correct() {
        let planned = bounded_plan::<1>();
        kani::cover!(planned.as_ref().is_ok_and(|out| out.skills().len() == 1));
        kani::cover!(matches!(planned, Err(Failure::Cycle)));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_plan_of_an_empty_catalog_meets_plan_correct() {
        let planned = bounded_plan::<0>();
        kani::cover!(planned.as_ref().is_ok_and(|out| out.skills().is_empty()));
        kani::cover!(matches!(planned, Err(Failure::Unknown)));
    }

    #[cfg(kani)]
    fn bounded_plan<const N: usize>() -> Result<Plan, Failure> {
        let mut graph = WordGraph {
            count: N,
            edges: [0; WORD_SKILLS],
        };
        for skill in 0..N {
            if let Some(row) = graph.edges.get_mut(skill) {
                *row = kani::any::<u8>() % 32;
            }
        }
        let requested = kani::any::<u8>() % 64;
        let result = plan(Input(InputKind::Word(graph, requested)));
        match &result {
            Ok(out) => {
                let out = out.skills();
                let mut seen = 0u8;
                for index in 0..out.len() {
                    let skill = out.get(index).copied().unwrap_or_default();
                    assert!(skill < N);
                    let bit = 1u8 << skill;
                    assert_eq!(seen & bit, 0);
                    assert!(
                        graph
                            .edges
                            .get(skill)
                            .is_some_and(|edges| *edges & !seen == 0)
                    );
                    seen |= bit;
                }
                assert_eq!(requested & !seen, 0);
                assert!(!word_cyclic(&graph));
            }
            Err(Failure::Unknown) => assert!(word_unknown(&graph, requested)),
            Err(Failure::Cycle) => assert!(word_cyclic(&graph)),
        }
        result
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_five_skill_catalog_and_request_sequence_has_its_exact_input_facts() {
        let valid = bounded_facts::<5>();
        kani::cover!(valid);
        kani::cover!(!valid);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_four_skill_catalog_and_request_sequence_has_its_exact_input_facts() {
        let valid = bounded_facts::<4>();
        kani::cover!(valid);
        kani::cover!(!valid);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_three_skill_catalog_and_request_sequence_has_its_exact_input_facts() {
        let valid = bounded_facts::<3>();
        kani::cover!(valid);
        kani::cover!(!valid);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_two_skill_catalog_and_request_sequence_has_its_exact_input_facts() {
        let valid = bounded_facts::<2>();
        kani::cover!(valid);
        kani::cover!(!valid);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_one_skill_catalog_and_request_sequence_has_its_exact_input_facts() {
        let valid = bounded_facts::<1>();
        kani::cover!(valid);
        kani::cover!(!valid);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn every_empty_catalog_and_request_sequence_has_its_exact_input_facts() {
        let valid = bounded_facts::<0>();
        kani::cover!(valid);
        kani::cover!(!valid);
    }

    #[cfg(kani)]
    fn bounded_facts<const N: usize>() -> bool {
        raw_input::<N, _>(|deps, requested, masks| {
            let members = (1u8 << N) - 1;
            let mut invalid = false;
            for skill in 0..N {
                invalid |= masks
                    .get(skill)
                    .is_some_and(|mask| *mask & 31 & !members != 0);
            }
            let mut wanted = 0u8;
            for index in 0..requested.len() {
                let skill = requested.get(index).copied().unwrap_or_default();
                wanted |= 1u8 << skill;
                invalid |= skill >= N;
            }
            match input(deps, requested) {
                Ok(Input(InputKind::Word(graph, requests))) => {
                    assert!(!invalid);
                    assert_eq!(graph.count, N);
                    assert_eq!(requests, wanted);
                    for skill in 0..N {
                        assert_eq!(
                            graph.edges.get(skill).copied(),
                            masks.get(skill).map(|mask| *mask & 31)
                        );
                    }
                    true
                }
                Ok(Input(InputKind::Slices(_, _))) => {
                    assert!(N > WORD_SKILLS);
                    true
                }
                Err(failure) => {
                    assert_eq!(failure, Failure::Unknown);
                    assert!(invalid);
                    false
                }
            }
        })
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn the_in_place_plan_of_five_skills_meets_the_same_contract() {
        let result = bounded_in_place::<0>();
        kani::cover!(result == Ok(0));
        kani::cover!(result == Err(Failure::Cycle));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn the_in_place_plan_for_one_request_meets_the_same_contract() {
        let result = bounded_in_place::<1>();
        kani::cover!(result == Ok(3));
        kani::cover!(result == Err(Failure::Cycle));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn the_in_place_plan_for_two_requests_meets_the_same_contract() {
        let result = bounded_in_place::<2>();
        kani::cover!(result == Ok(3));
        kani::cover!(result == Err(Failure::Cycle));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn the_in_place_plan_for_three_requests_meets_the_same_contract() {
        let result = bounded_in_place::<3>();
        kani::cover!(result == Ok(3));
        kani::cover!(result == Err(Failure::Cycle));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn the_in_place_plan_for_four_requests_meets_the_same_contract() {
        let result = bounded_in_place::<4>();
        kani::cover!(result == Ok(3));
        kani::cover!(result == Err(Failure::Cycle));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn the_in_place_plan_for_five_requests_meets_the_same_contract() {
        let result = bounded_in_place::<5>();
        kani::cover!(result == Ok(3));
        kani::cover!(result == Err(Failure::Cycle));
    }

    #[cfg(kani)]
    fn bounded_in_place<const REQUESTS: usize>() -> Result<usize, Failure> {
        let mut graph = WordGraph {
            count: 5,
            edges: [0; WORD_SKILLS],
        };
        for skill in 0..SKILLS {
            if let Some(row) = graph.edges.get_mut(skill) {
                *row = kani::any::<u8>() % 32;
            }
        }
        let requests = kani::any::<[u8; SKILLS]>().map(|skill| usize::from(skill % 6));
        let requested = requests.get(..REQUESTS).unwrap_or_default();
        let mut wanted = 0u8;
        for index in 0..requested.len() {
            if let Some(skill) = requested.get(index) {
                wanted |= 1u8 << skill;
            }
        }
        let mut out = [0; 5];
        let result = plan_into(Input(InputKind::Word(graph, wanted)), &mut out);
        match result {
            Ok(length) => {
                assert!(length <= out.len());
                let out = out.get(..length).unwrap_or_default();
                let mut before = 0u8;
                for index in 0..out.len() {
                    let skill = out.get(index).copied().unwrap_or_default();
                    assert!(skill < SKILLS);
                    assert_eq!(before & (1u8 << skill), 0);
                    assert!(
                        graph
                            .edges
                            .get(skill)
                            .is_some_and(|dependencies| *dependencies & !before == 0)
                    );
                    before |= 1u8 << skill;
                }
                assert_eq!(wanted & !before, 0);
            }
            Err(Failure::Unknown) => assert!(word_unknown(&graph, wanted)),
            Err(Failure::Cycle) => assert!(word_cyclic(&graph)),
        }
        result
    }

    #[cfg(kani)]
    fn raw_input<const N: usize, R>(
        run: impl FnOnce(&[&[usize]], &[usize], &[u8; SKILLS]) -> R,
    ) -> R {
        raw_input_length::<N, R>(usize::from(kani::any::<u8>() % 6), run)
    }

    #[cfg(kani)]
    fn raw_input_length<const N: usize, R>(
        length: usize,
        run: impl FnOnce(&[&[usize]], &[usize], &[u8; SKILLS]) -> R,
    ) -> R {
        let masks: [u8; SKILLS] = kani::any();
        let mut storage = [[0; SKILLS]; N];
        let mut lengths = [0; N];
        for (skill, mask) in masks.iter().take(N).enumerate() {
            for dependency in 0..SKILLS {
                if mask & (1 << dependency) != 0 {
                    if let (Some(row), Some(length)) =
                        (storage.get_mut(skill), lengths.get_mut(skill))
                    {
                        if let Some(slot) = row.get_mut(*length) {
                            *slot = dependency;
                            *length += 1;
                        }
                    }
                }
            }
        }
        let deps: [&[usize]; N] = core::array::from_fn(|skill| {
            let length = lengths.get(skill).copied().unwrap_or_default();
            storage
                .get(skill)
                .and_then(|row| row.get(..length))
                .unwrap_or_default()
        });
        let requests: [u8; SKILLS] = kani::any();
        let requested = requests.map(|skill| usize::from(skill % 6));
        run(&deps, &requested[..length], &masks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(deps: &[&[usize]], requested: &[usize]) -> Result<Vec<usize>, Failure> {
        super::input(deps, requested)
            .and_then(super::plan)
            .map(|plan| plan.skills().to_vec())
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every catalog of four skills")]
    fn every_catalog_of_up_to_four_skills_meets_plan_correct() {
        for count in 0..=4u32 {
            let sets = 1u32 << count;
            for code in 0..sets.pow(count) {
                let masks: Vec<u8> = (0..count)
                    .map(|skill| u8::try_from(code / sets.pow(skill) % sets).unwrap_or(0))
                    .collect();
                let deps = proofs::catalog(&masks);
                let deps: Vec<&[usize]> = deps.iter().map(Vec::as_slice).collect();
                let mut edges = [0; WORD_SKILLS];
                edges[..masks.len()].copy_from_slice(&masks);
                let graph = WordGraph {
                    count: deps.len(),
                    edges,
                };
                assert_eq!(word_cyclic(&graph), plan(&deps, &[]) == Err(Failure::Cycle));
                for requested in 0..=count {
                    let requested = usize::try_from(requested).unwrap_or(0);
                    let planned = proofs::spec_plan(&deps, &[requested]);
                    let requested_bits = 1u8 << requested;
                    assert_eq!(
                        word_unknown(&graph, requested_bits),
                        requested == deps.len()
                    );
                    let word = word_plan(graph, requested_bits).map(|word| {
                        assert!(word_valid(&graph, requested_bits, &word));
                        word.skills[..word.length].to_vec()
                    });
                    assert_eq!(planned, word);
                    let requests = [requested];
                    let in_place = super::input(&deps, &requests).and_then(|input| {
                        let mut output = alloc::vec![0; deps.len()];
                        plan_into(input, &mut output).map(|length| {
                            output.truncate(length);
                            output
                        })
                    });
                    assert_eq!(planned, in_place);
                    let mut reference = alloc::vec![0; deps.len()];
                    let reference = slice_plan(&deps, &[requested], &mut reference).map(|length| {
                        reference.truncate(length);
                        reference
                    });
                    assert_eq!(planned, reference);
                }
            }
        }
        let deps: [&[usize]; 3] = [&[], &[2, 0], &[0]];
        let graph = WordGraph {
            count: 2,
            edges: [0; WORD_SKILLS],
        };
        let mut repeated = WordPlan {
            skills: [0; WORD_SKILLS],
            length: 2,
        };
        assert!(!word_valid(&graph, 1, &repeated));
        repeated.skills[1] = 1;
        assert!(word_valid(&graph, 3, &repeated));
        let mut dependent = graph;
        dependent.edges[0] = 2;
        assert!(!word_valid(&dependent, 3, &repeated));
        let unknown = WordGraph {
            count: 1,
            edges: [2; WORD_SKILLS],
        };
        assert!(word_unknown(&unknown, 1));
        assert!(matches!(word_plan(unknown, 1), Err(Failure::Unknown)));
        let too_large = WordGraph {
            count: WORD_SKILLS + 1,
            edges: [0; WORD_SKILLS],
        };
        assert!(matches!(word_plan(too_large, 0), Err(Failure::Unknown)));
        assert_eq!(
            plan_into(super::input(&[&[]], &[0]).unwrap(), &mut []),
            Err(Failure::Unknown)
        );
        assert_eq!(plan(&deps, &[1]), Ok(Vec::from([0, 2, 1])));
        assert_eq!(plan(&deps, &[2]), Ok(Vec::from([0, 2])));
        assert_eq!(plan(&deps, &[]), Ok(Vec::new()));
        assert_eq!(plan(&deps, &[1, 2]), Ok(Vec::from([0, 2, 1])));
        assert_eq!(plan(&deps, &[1, 1]), Ok(Vec::from([0, 2, 1])));
        assert_eq!(plan(&[&[1], &[0]], &[0]), Err(Failure::Cycle));
        assert_eq!(plan(&[&[2]], &[0]), Err(Failure::Unknown));
        assert_eq!(plan(&[&[]], &[3]), Err(Failure::Unknown));
    }

    #[test]
    fn both_word_and_slice_catalogs_keep_the_same_declaration_order() {
        for count in [WORD_SKILLS, WORD_SKILLS + 1] {
            let storage: Vec<Vec<usize>> = (0..count)
                .map(|skill| {
                    if skill == 0 {
                        Vec::new()
                    } else {
                        Vec::from([skill - 1])
                    }
                })
                .collect();
            let deps: Vec<&[usize]> = storage.iter().map(Vec::as_slice).collect();
            let requested = [count - 1];
            let planned = plan(&deps, &requested);
            assert_eq!(planned, Ok((0..count).collect()));
            let mut reference = alloc::vec![0; count];
            let reference = slice_plan(&deps, &requested, &mut reference).map(|length| {
                reference.truncate(length);
                reference
            });
            assert_eq!(planned, reference);
        }
    }
}
