/// The shape of a partial line, which decides the classes that may describe it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Line {
    Text,
    Heading,
    /// A sole template action that renders nothing: an assignment, a condition, or a comment.
    Directive,
    /// A line whose template actions render output the audit can't read.
    Output,
    /// A line of a prompt file's front matter.
    Setting,
}

/// The classification that follows a quoted partial line in the audit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Class {
    Missing,
    Judgment,
    FollowUp,
    Heading,
    Directive,
    Setting,
}

/// A text line needs judgment or a defined follow-up, and every other shape needs its own class, so none can carry an unaudited instruction.
/// No class admits a line that renders template output.
pub fn admitted(line: Line, class: Class, follow_up_defined: bool) -> bool {
    match (line, class) {
        (Line::Text, Class::Judgment) => true,
        (Line::Text, Class::FollowUp) => follow_up_defined,
        (Line::Heading, Class::Heading)
        | (Line::Directive, Class::Directive)
        | (Line::Setting, Class::Setting) => true,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Definition {
    Current,
    Undefined,
    Duplicate,
    Unreferenced,
}

/// A follow-up stays current when one definition exists and a retained line refers to it.
pub fn follow_up(definitions: usize, references: usize) -> Definition {
    match (definitions, references) {
        (0, _) => Definition::Undefined,
        (1, 0) => Definition::Unreferenced,
        (1, _) => Definition::Current,
        _ => Definition::Duplicate,
    }
}

/// What holds a removed line now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Holder {
    Gate,
    Skill,
    GateAndSkill,
    Merged,
    /// The line contradicted a retained line or a skill, which holds the rule that applies instead.
    Contradicted,
}

/// A removed-line row holds when its holder exists.
/// A gate row names the gate's source file, a skill row names an existing skill, and a merge row names exactly one retained line.
/// A contradicted line names the retained line or the skill it contradicted.
pub fn removed_row_held(
    holder: Option<Holder>,
    names_source: bool,
    names_skill: bool,
    names_retained: bool,
) -> bool {
    match holder {
        Some(Holder::Gate) => names_source,
        Some(Holder::Skill) => names_skill,
        Some(Holder::GateAndSkill) => names_source && names_skill,
        Some(Holder::Merged) => names_retained,
        Some(Holder::Contradicted) => names_retained || names_skill,
        None => false,
    }
}

#[cfg(kani)]
fn any_line() -> Line {
    let choice: u8 = kani::any();
    kani::assume(choice < 5);
    match choice {
        0 => Line::Text,
        1 => Line::Heading,
        2 => Line::Directive,
        3 => Line::Output,
        _ => Line::Setting,
    }
}

#[cfg(kani)]
fn any_class() -> Class {
    let choice: u8 = kani::any();
    kani::assume(choice < 6);
    match choice {
        0 => Class::Missing,
        1 => Class::Judgment,
        2 => Class::FollowUp,
        3 => Class::Heading,
        4 => Class::Directive,
        _ => Class::Setting,
    }
}

#[cfg(kani)]
fn any_holder() -> Option<Holder> {
    let choice: u8 = kani::any();
    kani::assume(choice < 6);
    match choice {
        0 => Some(Holder::Gate),
        1 => Some(Holder::Skill),
        2 => Some(Holder::GateAndSkill),
        3 => Some(Holder::Merged),
        4 => Some(Holder::Contradicted),
        _ => None,
    }
}

#[cfg(kani)]
#[kani::proof]
fn every_line_needs_the_class_of_its_shape() {
    let line = any_line();
    let class = any_class();
    let defined: bool = kani::any();
    let result = admitted(line, class, defined);
    assert!(!result || class != Class::Missing);
    assert!(!result || (line == Line::Heading) == (class == Class::Heading));
    assert!(!result || (line == Line::Directive) == (class == Class::Directive));
    assert!(!result || (line == Line::Setting) == (class == Class::Setting));
    assert!(!result || line != Line::Output);
    assert!(!result || class != Class::FollowUp || defined);
    assert!(line != Line::Text || class != Class::Judgment || result);
    kani::cover!(result);
    kani::cover!(!result);
}

#[cfg(kani)]
#[kani::proof]
fn follow_ups_are_defined_once_and_referenced() {
    let definitions: usize = kani::any();
    let references: usize = kani::any();
    let state = follow_up(definitions, references);
    assert_eq!(
        state == Definition::Current,
        definitions == 1 && references > 0
    );
    assert!(state != Definition::Duplicate || definitions > 1);
    assert!(state != Definition::Undefined || definitions == 0);
    assert!(state != Definition::Unreferenced || (definitions == 1 && references == 0));
    kani::cover!(state == Definition::Current);
    kani::cover!(state == Definition::Undefined);
    kani::cover!(state == Definition::Duplicate);
    kani::cover!(state == Definition::Unreferenced);
}

#[cfg(kani)]
#[kani::proof]
fn removed_lines_name_what_holds_them() {
    let holder = any_holder();
    let source: bool = kani::any();
    let skill: bool = kani::any();
    let retained: bool = kani::any();
    let result = removed_row_held(holder, source, skill, retained);
    assert!(!result || holder.is_some());
    assert!(!result || !matches!(holder, Some(Holder::Gate | Holder::GateAndSkill)) || source);
    assert!(!result || !matches!(holder, Some(Holder::Skill | Holder::GateAndSkill)) || skill);
    assert!(!result || holder != Some(Holder::Merged) || retained);
    assert!(!result || holder != Some(Holder::Contradicted) || retained || skill);
    kani::cover!(result);
    kani::cover!(!result);
}
