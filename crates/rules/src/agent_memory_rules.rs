//! Whether an edit lands in a client's persistent agent memory, which every managed profile turns off.

/// One class of home-relative path component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Part {
    /// `.claude`
    Claude,
    /// `projects`
    Projects,
    /// `memory`
    Memory,
    /// `.codex`
    Codex,
    /// A name that starts with `memories`.
    Memories,
    Other,
    /// The path has no component at this depth.
    End,
}

/// Claude Code keeps `.claude/projects/<project>/memory/`, and Codex keeps `.codex/memories*`.
#[must_use]
pub const fn memory(parts: [Part; 4]) -> bool {
    match parts {
        [Part::Claude, Part::Projects, project, Part::Memory] => !matches!(project, Part::End),
        [Part::Codex, Part::Memories, _, _] => true,
        _ => false,
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const PARTS: [Part; 7] = [
        Part::Claude,
        Part::Projects,
        Part::Memory,
        Part::Codex,
        Part::Memories,
        Part::Other,
        Part::End,
    ];

    pub fn spec_memory(parts: [Part; 4]) {
        let [first, second, third, fourth] = parts;
        let claude = first == Part::Claude
            && second == Part::Projects
            && third != Part::End
            && fourth == Part::Memory;
        let codex = first == Part::Codex && second == Part::Memories;
        assert_eq!(memory(parts), claude || codex);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_client_memory_location_is_memory() {
        let parts: [Part; 4] = kani::any();
        spec_memory(parts);
        kani::cover!(memory(parts) && parts[0] == Part::Claude);
        kani::cover!(memory(parts) && parts[0] == Part::Codex);
        kani::cover!(!memory(parts) && parts[3] == Part::Memory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification() {
        for first in proofs::PARTS {
            for second in proofs::PARTS {
                for third in proofs::PARTS {
                    for fourth in proofs::PARTS {
                        proofs::spec_memory([first, second, third, fourth]);
                    }
                }
            }
        }
    }

    #[test]
    fn each_client_store_is_memory_and_its_neighbors_are_not() {
        assert!(memory([
            Part::Claude,
            Part::Projects,
            Part::Other,
            Part::Memory
        ]));
        assert!(memory([Part::Codex, Part::Memories, Part::End, Part::End]));
        assert!(!memory([
            Part::Claude,
            Part::Projects,
            Part::End,
            Part::End
        ]));
        assert!(!memory([
            Part::Claude,
            Part::Projects,
            Part::Other,
            Part::Other
        ]));
        assert!(!memory([Part::Codex, Part::Other, Part::End, Part::End]));
        assert!(!memory([
            Part::Other,
            Part::Projects,
            Part::Other,
            Part::Memory
        ]));
    }
}
