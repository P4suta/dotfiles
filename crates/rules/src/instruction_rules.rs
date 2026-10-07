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
#[must_use]
pub fn admitted(line: Line, class: Class, follow_up_defined: bool) -> bool {
    match (line, class) {
        (Line::Text, Class::FollowUp) => follow_up_defined,
        (Line::Text, Class::Judgment)
        | (Line::Heading, Class::Heading)
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
#[must_use]
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
#[must_use]
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

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const LINES: [Line; 5] = [
        Line::Text,
        Line::Heading,
        Line::Directive,
        Line::Output,
        Line::Setting,
    ];
    pub const CLASSES: [Class; 6] = [
        Class::Missing,
        Class::Judgment,
        Class::FollowUp,
        Class::Heading,
        Class::Directive,
        Class::Setting,
    ];
    pub const HOLDERS: [Option<Holder>; 6] = [
        Some(Holder::Gate),
        Some(Holder::Skill),
        Some(Holder::GateAndSkill),
        Some(Holder::Merged),
        Some(Holder::Contradicted),
        None,
    ];

    /// A text line takes judgment or a defined follow-up, every other shape its own class, and output none.
    pub fn spec_admitted(line: Line, class: Class, defined: bool) {
        let expected = match line {
            Line::Text => class == Class::Judgment || (class == Class::FollowUp && defined),
            Line::Heading => class == Class::Heading,
            Line::Directive => class == Class::Directive,
            Line::Setting => class == Class::Setting,
            Line::Output => false,
        };
        assert_eq!(admitted(line, class, defined), expected);
    }

    /// A follow-up stays current only with one definition and a reference, and each other state names its cause.
    pub fn spec_follow_up(definitions: usize, references: usize) {
        let state = follow_up(definitions, references);
        assert_eq!(
            state == Definition::Current,
            definitions == 1 && references > 0
        );
        assert_eq!(state == Definition::Duplicate, definitions > 1);
        assert_eq!(state == Definition::Undefined, definitions == 0);
        assert_eq!(
            state == Definition::Unreferenced,
            definitions == 1 && references == 0
        );
    }

    /// A removed line holds exactly when the row names what its holder needs.
    pub fn spec_removed_row_held(
        holder: Option<Holder>,
        source: bool,
        skill: bool,
        retained: bool,
    ) {
        let expected = match holder {
            Some(Holder::Gate) => source,
            Some(Holder::Skill) => skill,
            Some(Holder::GateAndSkill) => source && skill,
            Some(Holder::Merged) => retained,
            Some(Holder::Contradicted) => retained || skill,
            None => false,
        };
        assert_eq!(removed_row_held(holder, source, skill, retained), expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn every_line_needs_the_class_of_its_shape() {
        let line = LINES[usize::from(kani::any::<u8>() % 5)];
        let class = CLASSES[usize::from(kani::any::<u8>() % 6)];
        let defined: bool = kani::any();
        spec_admitted(line, class, defined);
        kani::cover!(admitted(line, class, defined));
        kani::cover!(!admitted(line, class, defined) && class == Class::FollowUp);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn follow_ups_are_defined_once_and_referenced() {
        let definitions: usize = kani::any();
        let references: usize = kani::any();
        spec_follow_up(definitions, references);
        let state = follow_up(definitions, references);
        kani::cover!(state == Definition::Current);
        kani::cover!(state == Definition::Duplicate);
        kani::cover!(state == Definition::Unreferenced);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn removed_lines_name_what_holds_them() {
        let holder = HOLDERS[usize::from(kani::any::<u8>() % 6)];
        let source: bool = kani::any();
        let skill: bool = kani::any();
        let retained: bool = kani::any();
        spec_removed_row_held(holder, source, skill, retained);
        kani::cover!(removed_row_held(holder, source, skill, retained));
        kani::cover!(!removed_row_held(holder, source, skill, retained) && holder.is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::proofs;
    use crate::domain::flags;

    #[test]
    fn every_input_meets_the_specifications() {
        for line in proofs::LINES {
            for class in proofs::CLASSES {
                for defined in [false, true] {
                    proofs::spec_admitted(line, class, defined);
                }
            }
        }
        for definitions in [0, 1, 2, usize::MAX] {
            for references in [0, 1, 2, usize::MAX] {
                proofs::spec_follow_up(definitions, references);
            }
        }
        for holder in proofs::HOLDERS {
            for [source, skill, retained] in flags() {
                proofs::spec_removed_row_held(holder, source, skill, retained);
            }
        }
    }
}
