//! Initialization that belongs at the first prompt rather than while a shell starts.

use core::ops::Range;

/// Atuin 18.23.0 starts this index job before Nushell's first prompt.
const ATUIN_INDEX: &[u8] = b"if (version).minor >= 104 or (version).major > 0 {\n    with-env { ATUIN_SHELL: nu } {\n        job spawn {\n            atuin __internal prepare-search-index | complete\n        } | ignore\n    }\n}\n";
const ATUIN_ZSH_INDEX: &[u8] =
    b"(ATUIN_SHELL=zsh atuin __internal prepare-search-index >/dev/null 2>&1 &)";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shell {
    Nu,
    Zsh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Escape {
    Verbatim,
    Quote,
    Slash,
    Control,
}

const NU_CONTINUATION_START: &[u8] = b"    PROMPT_MULTILINE_INDICATOR: (";
const NU_CONTINUATION_END: &[u8] = b"    )";
const NU_COMMAND_START: &[u8] = b"        ^'";
const NU_COMMAND_END: &[u8] = b"' prompt --continuation";
const NU_BARE_COMMAND_START: &[u8] = b"        ^";
const NU_BARE_COMMAND_END: &[u8] = b" prompt --continuation";
const ZSH_COMMAND_START: &[u8] = b"PROMPT2=\"$(";
const ZSH_COMMAND_END: &[u8] = b" prompt --continuation)\"";

fn line(input: &[u8], start: usize) -> Option<(&[u8], usize)> {
    let remaining = input.get(start..)?;
    let length = remaining.iter().position(|byte| *byte == b'\n');
    match length {
        Some(length) => Some((remaining.get(..length)?, start + length + 1)),
        None => Some((remaining, input.len())),
    }
}

fn command(shell: Shell, line: &[u8]) -> bool {
    match shell {
        Shell::Nu => nu_command(line),
        Shell::Zsh => {
            let Some(program) = line
                .strip_prefix(ZSH_COMMAND_START)
                .and_then(|line| line.strip_suffix(ZSH_COMMAND_END))
            else {
                return false;
            };
            !program.is_empty()
                && program.iter().all(|byte| {
                    !matches!(byte, 0 | b'\r' | b'\n' | b'\"' | b'$' | b'`' | b'(' | b')')
                })
        }
    }
}

fn nu_command(line: &[u8]) -> bool {
    if let Some(program) = line
        .strip_prefix(NU_COMMAND_START)
        .and_then(|line| line.strip_suffix(NU_COMMAND_END))
    {
        return !program.is_empty()
            && program
                .iter()
                .all(|byte| !matches!(byte, 0 | b'\r' | b'\n' | b'\''));
    }
    let Some(program) = line
        .strip_prefix(NU_BARE_COMMAND_START)
        .and_then(|line| line.strip_suffix(NU_BARE_COMMAND_END))
    else {
        return false;
    };
    !program.is_empty() && program.iter().copied().all(nu_bare_path_byte)
}

const fn nu_bare_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-')
}

/// The unique eager continuation assignment in the pinned Starship generator.
#[must_use]
pub fn continuation(shell: Shell, input: &[u8]) -> Option<Range<usize>> {
    let mut start = 0;
    let mut selected = None;
    while start < input.len() {
        let (first, next) = line(input, start)?;
        let end = match shell {
            Shell::Nu if first == NU_CONTINUATION_START => {
                let (expression, following) = line(input, next)?;
                let (last, end) = line(input, following)?;
                (command(shell, expression) && last == NU_CONTINUATION_END).then_some(end)
            }
            Shell::Zsh if command(shell, first) => Some(next),
            _ => None,
        };
        if let Some(end) = end {
            if selected.is_some() {
                return None;
            }
            selected = Some(start..end);
        }
        start = next;
    }
    selected
}

/// Escaping for Nushell strings and zsh `$'...'` strings.
#[must_use]
pub fn literal_escape(shell: Shell, character: char) -> Escape {
    match (shell, character) {
        (Shell::Nu, '"') | (Shell::Zsh, '\'') => Escape::Quote,
        (_, '\\') => Escape::Slash,
        (_, character) if character.is_control() => Escape::Control,
        _ => Escape::Verbatim,
    }
}

/// The captured prompt prefix after the shell's command-output newline handling.
#[must_use]
pub fn prompt_end(shell: Shell, input: &[u8]) -> usize {
    match shell {
        Shell::Nu => input
            .strip_suffix(b"\r\n")
            .or_else(|| input.strip_suffix(b"\n"))
            .map_or(input.len(), <[u8]>::len),
        Shell::Zsh => input
            .iter()
            .rposition(|byte| *byte != b'\n')
            .map_or(0, |last| last + 1),
    }
}

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
    const NU_QUOTED: &[u8; 76] =
        b"    PROMPT_MULTILINE_INDICATOR: (\n        ^'/p' prompt --continuation\n    )\n";
    #[cfg(kani)]
    const NU_BARE: &[u8; 74] =
        b"    PROMPT_MULTILINE_INDICATOR: (\n        ^/p prompt --continuation\n    )\n";
    #[cfg(kani)]
    const ZSH: &[u8; 38] = b"PROMPT2=\"$(/p prompt --continuation)\"\n";
    #[cfg(kani)]
    fn changed_continuation<const N: usize>(
        shell: Shell,
        range: Range<u8>,
        bare_nu: bool,
        original: &[u8; N],
    ) {
        let path = match shell {
            Shell::Nu if bare_nu => NU_CONTINUATION_START.len() + 1 + NU_BARE_COMMAND_START.len(),
            Shell::Nu => NU_CONTINUATION_START.len() + 1 + NU_COMMAND_START.len(),
            Shell::Zsh => ZSH_COMMAND_START.len(),
        };
        let mut input = *original;
        let offset: u8 = kani::any();
        kani::assume(range.contains(&offset));
        let offset = usize::from(offset);
        let byte: u8 = kani::any();
        input[offset] = byte;
        let retained_path = matches!(offset.checked_sub(path), Some(0 | 1));
        let path_byte = !matches!(byte, 0 | b'\r' | b'\n')
            && match shell {
                Shell::Nu if bare_nu => nu_bare_path_byte(byte),
                Shell::Nu => !matches!(byte, 0 | b'\r' | b'\n' | b'\''),
                Shell::Zsh => !matches!(byte, b'"' | b'$' | b'`' | b'(' | b')'),
            };
        let expected = if retained_path {
            path_byte
        } else {
            original.get(offset) == Some(&byte)
        };
        assert_eq!(continuation(shell, &input).is_some(), expected);
        kani::cover!(expected);
        kani::cover!(!expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn nu_continuation_shape_bytes_zero_through_thirty_one() {
        let bare_nu: bool = kani::any();
        if bare_nu {
            changed_continuation(Shell::Nu, 0..32, true, NU_BARE);
        } else {
            changed_continuation(Shell::Nu, 0..32, false, NU_QUOTED);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn nu_continuation_shape_bytes_thirty_two_through_sixty_three() {
        let bare_nu: bool = kani::any();
        if bare_nu {
            changed_continuation(Shell::Nu, 32..64, true, NU_BARE);
        } else {
            changed_continuation(Shell::Nu, 32..64, false, NU_QUOTED);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn nu_continuation_shape_bytes_sixty_four_through_the_end() {
        let bare_nu: bool = kani::any();
        if bare_nu {
            changed_continuation(
                Shell::Nu,
                64..u8::try_from(NU_BARE.len()).unwrap(),
                true,
                NU_BARE,
            );
        } else {
            changed_continuation(
                Shell::Nu,
                64..u8::try_from(NU_QUOTED.len()).unwrap(),
                false,
                NU_QUOTED,
            );
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn zsh_continuation_shape_bytes_zero_through_thirty_one() {
        changed_continuation(Shell::Zsh, 0..32, false, ZSH);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn zsh_continuation_shape_bytes_thirty_two_through_the_end() {
        changed_continuation(Shell::Zsh, 32..u8::try_from(ZSH.len()).unwrap(), false, ZSH);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(201)]
    fn continuation_requires_a_line_boundary_and_a_unique_assignment() {
        let mut input = [0u8; NU_BARE.len() + 1];
        input[0] = kani::any();
        input[1..].copy_from_slice(NU_BARE);
        assert!(continuation(Shell::Nu, &input[1..]).is_some());
        assert_eq!(continuation(Shell::Nu, &input).is_some(), input[0] == b'\n');
        let mut quoted_input = [0u8; NU_QUOTED.len() + 1];
        quoted_input[0] = kani::any();
        quoted_input[1..].copy_from_slice(NU_QUOTED);
        assert!(continuation(Shell::Nu, &quoted_input[1..]).is_some());
        assert_eq!(
            continuation(Shell::Nu, &quoted_input).is_some(),
            quoted_input[0] == b'\n'
        );
        let mut duplicate = [0u8; NU_BARE.len() * 2];
        duplicate[..NU_BARE.len()].copy_from_slice(NU_BARE);
        duplicate[NU_BARE.len()..].copy_from_slice(NU_BARE);
        assert!(continuation(Shell::Nu, &duplicate).is_none());
        let mut duplicate_quoted = [0u8; NU_QUOTED.len() * 2];
        duplicate_quoted[..NU_QUOTED.len()].copy_from_slice(NU_QUOTED);
        duplicate_quoted[NU_QUOTED.len()..].copy_from_slice(NU_QUOTED);
        assert!(continuation(Shell::Nu, &duplicate_quoted).is_none());
        kani::cover!(input[0] == b'\n');
        kani::cover!(input[0] != b'\n');
    }

    #[cfg(kani)]
    #[kani::proof]
    fn literal_escaping_covers_every_unicode_scalar_and_shell_quote() {
        let character: char = kani::any();
        let nu: bool = kani::any();
        let shell = if nu { Shell::Nu } else { Shell::Zsh };
        let expected = if (nu && character == '"') || (!nu && character == '\'') {
            Escape::Quote
        } else if character == '\\' {
            Escape::Slash
        } else if matches!(character, '\0'..='\u{1f}' | '\u{7f}'..='\u{9f}') {
            Escape::Control
        } else {
            Escape::Verbatim
        };
        assert_eq!(literal_escape(shell, character), expected);
        kani::cover!(expected == Escape::Quote);
        kani::cover!(expected == Escape::Slash);
        kani::cover!(expected == Escape::Control);
        kani::cover!(expected == Escape::Verbatim);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn captured_prompt_newlines_match_each_shell() {
        let input: [u8; 8] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= input.len());
        let input = &input[..length];
        let nu = prompt_end(Shell::Nu, input);
        let zsh = prompt_end(Shell::Zsh, input);
        assert!(nu <= length && zsh <= length);
        let ending = if input.last() == Some(&b'\n') {
            if length >= 2 && input.get(length - 2) == Some(&b'\r') {
                2
            } else {
                1
            }
        } else {
            0
        };
        assert_eq!(nu, length - ending);
        assert!(input[zsh..].iter().all(|byte| *byte == b'\n'));
        assert!(zsh == 0 || input.get(zsh - 1) != Some(&b'\n'));
        kani::cover!(nu < length);
        kani::cover!(nu == length);
        kani::cover!(zsh < length);
        kani::cover!(zsh == length);
    }

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
    fn continuation_selects_one_known_eager_assignment_and_preserves_unknown_shapes() {
        let nu = b"    PROMPT_MULTILINE_INDICATOR: (\n        ^/native/mise/installs/starship/1.26.0/starship prompt --continuation\n    )\n";
        let nu_quoted = b"    PROMPT_MULTILINE_INDICATOR: (\n        ^'/path with spaces/starship' prompt --continuation\n    )\n";
        let zsh = b"PROMPT2=\"$(/native/starship prompt --continuation)\"\n";
        for (shell, script) in [
            (Shell::Nu, &nu[..]),
            (Shell::Nu, &nu_quoted[..]),
            (Shell::Zsh, &zsh[..]),
        ] {
            assert_eq!(continuation(shell, script), Some(0..script.len()));
            let duplicated: Vec<_> = script.iter().chain(script).copied().collect();
            assert_eq!(continuation(shell, &duplicated), None);
            let nested: Vec<_> = b" ".iter().chain(script).copied().collect();
            assert_eq!(continuation(shell, &nested), None);
            for prefix in [&b"\n"[..], b"# known generator\n"] {
                let surrounded: Vec<_> = prefix
                    .iter()
                    .chain(script)
                    .chain(b"# deferred\n")
                    .copied()
                    .collect();
                assert_eq!(
                    continuation(shell, &surrounded),
                    Some(prefix.len()..prefix.len() + script.len())
                );
            }
        }
        for invalid in [
            &b""[..],
            b"    PROMPT_MULTILINE_INDICATOR: (\n        ^/path/starship;touch prompt --continuation\n    )\n",
            b"    PROMPT_MULTILINE_INDICATOR: (\n        ^/path/$(touch) prompt --continuation\n    )\n",
            b"PROMPT2='deferred prompt'\n",
            b"PROMPT2=\"$(starship prompt --unknown)\"\n",
        ] {
            assert_eq!(continuation(Shell::Zsh, invalid), None);
        }
        for character in ['\0', '\n', '\u{1b}', '\u{7f}', '\u{85}', '\u{9f}'] {
            assert_eq!(literal_escape(Shell::Nu, character), Escape::Control);
            assert_eq!(literal_escape(Shell::Zsh, character), Escape::Control);
        }
        assert_eq!(literal_escape(Shell::Nu, '"'), Escape::Quote);
        assert_eq!(literal_escape(Shell::Zsh, '\''), Escape::Quote);
        assert_eq!(literal_escape(Shell::Nu, '\\'), Escape::Slash);
        assert_eq!(literal_escape(Shell::Zsh, '\\'), Escape::Slash);
        for character in ['$', '`', '#', '日', '😀'] {
            assert_eq!(literal_escape(Shell::Nu, character), Escape::Verbatim);
            assert_eq!(literal_escape(Shell::Zsh, character), Escape::Verbatim);
        }
    }

    #[test]
    fn prompt_capture_preserves_the_original_shell_newline_semantics() {
        for (input, nu, zsh) in [
            (&b""[..], &b""[..], &b""[..]),
            (b"line", b"line", b"line"),
            (b"line\r", b"line\r", b"line\r"),
            (b"line\n", b"line", b"line"),
            (b"line\n\n", b"line\n", b"line"),
            (b"line\r\n\r\n", b"line\r\n", b"line\r\n\r"),
        ] {
            assert_eq!(&input[..prompt_end(Shell::Nu, input)], nu);
            assert_eq!(&input[..prompt_end(Shell::Zsh, input)], zsh);
        }
    }

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
