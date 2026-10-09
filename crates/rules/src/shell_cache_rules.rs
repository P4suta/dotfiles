//! Initialization that belongs at the first prompt rather than while a shell starts.

use core::ops::Range;

/// Atuin 18.23.0 starts this index job before Nushell's first prompt.
const ATUIN_INDEX: &[u8] = b"if (version).minor >= 104 or (version).major > 0 {\n    with-env { ATUIN_SHELL: nu } {\n        job spawn {\n            atuin __internal prepare-search-index | complete\n        } | ignore\n    }\n}\n";
const ATUIN_ZSH_INDEX: &[u8] =
    b"(ATUIN_SHELL=zsh atuin __internal prepare-search-index >/dev/null 2>&1 &)";

/// Whether Atuin's zsh initialization line leaves index construction to the first search.
#[must_use]
pub fn keep_atuin_line(line: &[u8]) -> bool {
    line != ATUIN_ZSH_INDEX
}

/// The exact top-level Atuin index job, when present, without any surrounding initialization.
#[must_use]
pub fn startup_index(input: &[u8]) -> Option<Range<usize>> {
    input
        .windows(ATUIN_INDEX.len())
        .enumerate()
        .find(|(start, block)| {
            (*start == 0 || input.get(start - 1) == Some(&b'\n')) && *block == ATUIN_INDEX
        })
        .map(|(start, _)| start..start + ATUIN_INDEX.len())
}

/// Whether a mise activation line preserves the shell's own environment and waits for its first prompt.
#[must_use]
pub fn keep_mise_line(line: &[u8]) -> bool {
    line != b"_mise_hook --force"
        && line != b"_mise_hook"
        && !line.starts_with(b"${Env:PATH}=")
        && !line.starts_with(b"export PATH='")
        && !line.starts_with(b"export __MISE_ORIG_PATH='")
        && !line.starts_with(b"export __MISE_ZSH_ACTIVATE_PATH=")
        && !line.starts_with(b"export __MISE_ZSH_ACTIVATE_ENV=")
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(75)]
    fn only_the_exact_zsh_index_job_is_deferred() {
        let mut line = [0u8; ATUIN_ZSH_INDEX.len()];
        line.copy_from_slice(ATUIN_ZSH_INDEX);
        let offset = usize::from(kani::any::<u8>());
        kani::assume(offset < line.len());
        let byte: u8 = kani::any();
        line[offset] = byte;
        assert_eq!(
            keep_atuin_line(&line),
            ATUIN_ZSH_INDEX.get(offset) != Some(&byte)
        );
        kani::cover!(keep_atuin_line(&line));
        kani::cover!(!keep_atuin_line(&line));
    }

    pub fn spec_index(input: &[u8]) {
        if let Some(span) = startup_index(input) {
            assert_eq!(input.get(span.clone()), Some(ATUIN_INDEX));
            assert!(span.start == 0 || input.get(span.start - 1) == Some(&b'\n'));
            assert!(span.end <= input.len());
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(196)]
    fn only_the_complete_top_level_index_job_is_selected() {
        let mut input = [0u8; ATUIN_INDEX.len() + 1];
        input[0] = kani::any();
        input[1..].copy_from_slice(ATUIN_INDEX);
        spec_index(&input);
        assert_eq!(startup_index(&input).is_some(), input[0] == b'\n');
        kani::cover!(startup_index(&input).is_some());
        kani::cover!(startup_index(&input).is_none());
    }

    #[cfg(kani)]
    fn changed_index_job(range: Range<u8>) {
        let mut input = [0u8; ATUIN_INDEX.len()];
        input.copy_from_slice(ATUIN_INDEX);
        let offset: u8 = kani::any();
        kani::assume(range.contains(&offset));
        let offset = usize::from(offset);
        let byte: u8 = kani::any();
        input[offset] = byte;
        spec_index(&input);
        assert_eq!(
            startup_index(&input).is_some(),
            ATUIN_INDEX.get(offset) == Some(&byte)
        );
        kani::cover!(startup_index(&input).is_some());
        kani::cover!(startup_index(&input).is_none());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(196)]
    fn changed_index_bytes_zero_through_sixty_three_are_preserved() {
        changed_index_job(0..64);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(196)]
    fn changed_index_bytes_sixty_four_through_one_twenty_seven_are_preserved() {
        changed_index_job(64..128);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(196)]
    fn changed_index_bytes_one_twenty_eight_through_one_ninety_one_are_preserved() {
        changed_index_job(128..192);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(196)]
    fn changed_index_bytes_one_ninety_two_through_the_end_are_preserved() {
        changed_index_job(192..194);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(40)]
    fn mise_keeps_live_path_assignments_and_defers_the_forced_hook() {
        let suffix: [u8; 2] = kani::any();
        let mut baked = [0u8; 15];
        baked[..13].copy_from_slice(b"export PATH='");
        baked[13..].copy_from_slice(&suffix);
        assert!(!keep_mise_line(&baked));
        assert!(!keep_mise_line(b"_mise_hook --force"));
        assert!(!keep_mise_line(b"_mise_hook"));
        assert!(keep_mise_line(b"export __MISE_ORIG_PATH=\"$PATH\""));
        assert!(keep_mise_line(b"add-zsh-hook precmd _mise_hook_precmd"));
        kani::cover!(suffix[0] == b'a');
        kani::cover!(suffix[0] != b'a');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn atuin_keeps_search_and_history_functions_but_starts_no_index_job() {
        assert!(!keep_atuin_line(ATUIN_ZSH_INDEX));
        for line in [
            &b""[..],
            b"atuin search",
            b"atuin history start",
            b"# prepare-search-index",
            b"(ATUIN_SHELL=zsh atuin other >/dev/null 2>&1 &)",
        ] {
            assert!(keep_atuin_line(line));
        }
        for offset in 0..ATUIN_ZSH_INDEX.len() {
            let mut changed = ATUIN_ZSH_INDEX.to_vec();
            if let Some(byte) = changed.get_mut(offset) {
                *byte ^= 1;
            }
            assert!(keep_atuin_line(&changed));
        }
    }

    #[test]
    fn index_selection_keeps_unknown_blocks_and_surrounding_initialization() {
        let proved: Vec<_> = (0..64)
            .chain(64..128)
            .chain(128..192)
            .chain(192..194)
            .collect();
        assert_eq!(proved, (0..ATUIN_INDEX.len()).collect::<Vec<_>>());
        assert_eq!(startup_index(ATUIN_INDEX), Some(0..ATUIN_INDEX.len()));
        for prefix in [&b"\n"[..], b"let x = 1\n", b"# prepare-search-index\n"] {
            let mut input = prefix.to_vec();
            input.extend_from_slice(ATUIN_INDEX);
            input.extend_from_slice(b"$env.config = {}\n");
            assert_eq!(
                startup_index(&input),
                Some(prefix.len()..prefix.len() + ATUIN_INDEX.len())
            );
            proofs::spec_index(&input);
        }
        for prefix in [&b" "[..], b"#", b"if other { "] {
            let mut input = prefix.to_vec();
            input.extend_from_slice(ATUIN_INDEX);
            assert_eq!(startup_index(&input), None);
        }
        for offset in 0..ATUIN_INDEX.len() {
            let mut changed = ATUIN_INDEX.to_vec();
            if let Some(byte) = changed.get_mut(offset) {
                *byte ^= 1;
            }
            assert_eq!(startup_index(&changed), None, "offset {offset}");
        }
        for input in [&b""[..], b"if other {\n    print 'prepare-search-index'\n}\n", b"if (version).minor >= 104 {\n    job spawn { atuin __internal prepare-search-index }\n}\n"] {
            assert_eq!(startup_index(input), None);
        }
    }

    #[test]
    fn only_baked_paths_and_startup_hook_state_are_deferred() {
        for prefix in [
            &b"${Env:PATH}="[..],
            &b"export PATH='"[..],
            b"export __MISE_ORIG_PATH='",
            b"export __MISE_ZSH_ACTIVATE_PATH=",
            b"export __MISE_ZSH_ACTIVATE_ENV=",
        ] {
            for suffix in [
                &b""[..],
                b"/baked'",
                b"\"$PATH\"",
                b"\"$(_mise_hook_env_state)\"",
            ] {
                let line: Vec<u8> = prefix.iter().chain(suffix).copied().collect();
                assert!(!keep_mise_line(&line));
            }
            let mut changed = prefix.to_vec();
            if let Some(byte) = changed.last_mut() {
                *byte ^= 1;
            }
            assert!(keep_mise_line(&changed));
        }
        assert!(!keep_mise_line(b"_mise_hook --force"));
        for line in [
            &b""[..],
            b" _mise_hook",
            b"_mise_hook --force-extra",
            b"export PATH=\"$PATH\"",
            b"export __MISE_ORIG_PATH=\"$PATH\"",
            b"add-zsh-hook precmd _mise_hook_precmd",
            b"  export PATH='/nested'",
        ] {
            assert!(keep_mise_line(line));
        }
    }
}
