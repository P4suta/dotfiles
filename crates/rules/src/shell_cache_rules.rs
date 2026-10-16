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

const fn byte_at(input: &[u8], index: usize) -> Option<u8> {
    let Some((_, tail)) = input.split_at_checked(index) else {
        return None;
    };
    tail.first().copied()
}

const fn line(input: &[u8], start: usize) -> (usize, usize) {
    let mut end = start;
    while end < input.len() {
        if matches!(byte_at(input, end), Some(b'\n')) {
            return (end, end + 1);
        }
        end += 1;
    }
    (end, end)
}

const fn span_is(input: &[u8], start: usize, end: usize, expected: &[u8]) -> bool {
    if !matches!(end.checked_sub(start), Some(length) if length == expected.len()) {
        return false;
    }
    let mut index = 0;
    while index < expected.len() {
        if !matches!(
            (byte_at(input, start + index), byte_at(expected, index)),
            (Some(actual), Some(expected)) if actual == expected
        ) {
            return false;
        }
        index += 1;
    }
    true
}

const fn program_range(
    input: &[u8],
    start: usize,
    end: usize,
    prefix: &[u8],
    suffix: &[u8],
) -> Option<Range<usize>> {
    let Some(program_start) = start.checked_add(prefix.len()) else {
        return None;
    };
    let Some(program_end) = end.checked_sub(suffix.len()) else {
        return None;
    };
    if !span_is(input, start, program_start, prefix) || !span_is(input, program_end, end, suffix) {
        return None;
    }
    Some(program_start..program_end)
}

const fn command(shell: Shell, input: &[u8], start: usize, end: usize) -> bool {
    match shell {
        Shell::Nu => nu_command(input, start, end),
        Shell::Zsh => {
            let Some(program) =
                program_range(input, start, end, ZSH_COMMAND_START, ZSH_COMMAND_END)
            else {
                return false;
            };
            let mut index = program.start;
            let mut nonempty = false;
            while index < program.end {
                if matches!(
                    byte_at(input, index),
                    Some(0 | b'\r' | b'\n' | b'\"' | b'$' | b'`' | b'(' | b')')
                ) {
                    return false;
                }
                nonempty = true;
                index += 1;
            }
            nonempty
        }
    }
}

const fn nu_command(input: &[u8], start: usize, end: usize) -> bool {
    let (program, quoted) =
        if let Some(program) = program_range(input, start, end, NU_COMMAND_START, NU_COMMAND_END) {
            (program, true)
        } else if let Some(program) = program_range(
            input,
            start,
            end,
            NU_BARE_COMMAND_START,
            NU_BARE_COMMAND_END,
        ) {
            (program, false)
        } else {
            return false;
        };
    let mut index = program.start;
    let mut nonempty = false;
    while index < program.end {
        let byte = byte_at(input, index);
        if if quoted {
            matches!(byte, Some(0 | b'\r' | b'\n' | b'\''))
        } else {
            !matches!(byte, Some(byte) if nu_bare_path_byte(byte))
        } {
            return false;
        }
        nonempty = true;
        index += 1;
    }
    nonempty
}

const fn nu_bare_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-')
}

/// The unique eager continuation assignment in the pinned Starship generator.
#[must_use]
pub const fn continuation(shell: Shell, input: &[u8]) -> Option<Range<usize>> {
    let mut start = 0;
    let mut selected = None;
    let mut previous_header: Option<usize> = None;
    let mut command_header: Option<usize> = None;
    while start < input.len() {
        let (end, next) = line(input, start);
        let candidate = match shell {
            Shell::Nu => {
                let candidate = if span_is(input, start, end, NU_CONTINUATION_END) {
                    match command_header {
                        Some(header) => Some(header..next),
                        None => None,
                    }
                } else {
                    None
                };
                command_header = if command(shell, input, start, end) {
                    previous_header
                } else {
                    None
                };
                previous_header = if span_is(input, start, end, NU_CONTINUATION_START) {
                    Some(start)
                } else {
                    None
                };
                candidate
            }
            Shell::Zsh => {
                if command(shell, input, start, end) {
                    Some(start..next)
                } else {
                    None
                }
            }
        };
        if let Some(candidate) = candidate {
            if selected.is_some() {
                return None;
            }
            selected = Some(candidate);
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
    const CELL_BITS: usize = 2;
    #[cfg(kani)]
    const CELLS_PER_BYTE: usize = 4;
    #[cfg(kani)]
    const BYTE_VALUES: usize = 256;
    #[cfg(kani)]
    const PACKED_BYTES: usize = 64;
    #[cfg(kani)]
    const CELL_UNEVALUATED: u8 = 0;
    #[cfg(kani)]
    const CELL_REJECT: u8 = 1;
    #[cfg(kani)]
    const CELL_ACCEPT: u8 = 2;
    #[cfg(kani)]
    const CELL_INVALID: u8 = 3;
    #[cfg(kani)]
    const CELL_MASK: u8 = 3;

    #[cfg(kani)]
    const fn raw_cell(cells: &[u8; PACKED_BYTES], key: u8) -> u8 {
        let slot = key as usize / CELLS_PER_BYTE;
        let shift = (key as usize % CELLS_PER_BYTE) * CELL_BITS;
        (cells[slot] >> shift) & CELL_MASK
    }

    #[cfg(kani)]
    const fn insert_cell(cells: &mut [u8; PACKED_BYTES], key: u8, actual: bool) {
        let slot = key as usize / CELLS_PER_BYTE;
        let shift = (key as usize % CELLS_PER_BYTE) * CELL_BITS;
        let encoded = if actual { CELL_ACCEPT } else { CELL_REJECT };
        let mask = CELL_MASK << shift;
        cells[slot] = (cells[slot] & !mask) | (encoded << shift);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn continuation_cell_storage_preserves_key_identity_and_other_cells() {
        let original: [u8; PACKED_BYTES] = kani::any();
        let key: u8 = kani::any();
        let other: u8 = kani::any();
        let actual: bool = kani::any();
        let mut updated = original;
        insert_cell(&mut updated, key, actual);
        assert_eq!(
            raw_cell(&updated, key),
            if actual { CELL_ACCEPT } else { CELL_REJECT }
        );
        if other != key {
            assert_eq!(raw_cell(&updated, other), raw_cell(&original, other));
        }
        kani::cover!(other == key);
        kani::cover!(other != key);
    }

    #[cfg(kani)]
    struct ContinuationRow {
        offset: usize,
        cells: [u8; PACKED_BYTES],
    }

    #[cfg(kani)]
    const fn continuation_row<const N: usize>(
        shell: Shell,
        original: &[u8; N],
        offset: usize,
    ) -> ContinuationRow {
        assert!(offset < N);
        let mut cells = [CELL_UNEVALUATED; PACKED_BYTES];
        let mut byte = 0;
        while byte < BYTE_VALUES {
            let evaluated_byte = byte as u8;
            let mut input = *original;
            input[offset] = evaluated_byte;
            insert_cell(
                &mut cells,
                evaluated_byte,
                continuation(shell, &input).is_some(),
            );
            byte += 1;
        }
        ContinuationRow { offset, cells }
    }

    #[cfg(kani)]
    macro_rules! continuation_rows {
        ($shell:expr, $original:ident; $($offset:literal),+ $(,)?) => {
            [$(const { continuation_row($shell, $original, $offset) }),+]
        };
    }

    #[cfg(kani)]
    const NU_BARE_ROWS: [ContinuationRow; NU_BARE.len()] = continuation_rows!(Shell::Nu, NU_BARE; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73);
    #[cfg(kani)]
    const NU_QUOTED_ROWS: [ContinuationRow; NU_QUOTED.len()] = continuation_rows!(Shell::Nu, NU_QUOTED; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75);
    #[cfg(kani)]
    const ZSH_ROWS: [ContinuationRow; ZSH.len()] = continuation_rows!(Shell::Zsh, ZSH; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37);

    #[cfg(kani)]
    fn evaluated_continuation(row: &ContinuationRow, offset: usize, byte: u8) -> bool {
        assert_eq!(row.offset, offset);
        match raw_cell(&row.cells, byte) {
            CELL_REJECT => false,
            CELL_ACCEPT => true,
            CELL_UNEVALUATED => panic!("CTFE row must evaluate every admitted input"),
            CELL_INVALID => panic!("CTFE row must contain a valid result code"),
            _ => unreachable!(),
        }
    }

    #[cfg(kani)]
    fn changed_continuation<const N: usize>(
        shell: Shell,
        range: Range<u8>,
        bare_nu: bool,
        original: &[u8; N],
        rows: &[ContinuationRow; N],
    ) {
        let path = match shell {
            Shell::Nu if bare_nu => NU_CONTINUATION_START.len() + 1 + NU_BARE_COMMAND_START.len(),
            Shell::Nu => NU_CONTINUATION_START.len() + 1 + NU_COMMAND_START.len(),
            Shell::Zsh => ZSH_COMMAND_START.len(),
        };
        let offset: u8 = kani::any();
        kani::assume(range.contains(&offset));
        let offset = usize::from(offset);
        let byte: u8 = kani::any();
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
        assert_eq!(
            evaluated_continuation(&rows[offset], offset, byte),
            expected
        );
        kani::cover!(expected);
        kani::cover!(!expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn nu_continuation_shape_bytes_zero_through_thirty_one() {
        let bare_nu: bool = kani::any();
        if bare_nu {
            changed_continuation(Shell::Nu, 0..32, true, NU_BARE, &NU_BARE_ROWS);
        } else {
            changed_continuation(Shell::Nu, 0..32, false, NU_QUOTED, &NU_QUOTED_ROWS);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn nu_continuation_shape_bytes_thirty_two_through_sixty_three() {
        let bare_nu: bool = kani::any();
        if bare_nu {
            changed_continuation(Shell::Nu, 32..64, true, NU_BARE, &NU_BARE_ROWS);
        } else {
            changed_continuation(Shell::Nu, 32..64, false, NU_QUOTED, &NU_QUOTED_ROWS);
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
                &NU_BARE_ROWS,
            );
        } else {
            changed_continuation(
                Shell::Nu,
                64..u8::try_from(NU_QUOTED.len()).unwrap(),
                false,
                NU_QUOTED,
                &NU_QUOTED_ROWS,
            );
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn zsh_continuation_shape_bytes_zero_through_thirty_one() {
        changed_continuation(Shell::Zsh, 0..32, false, ZSH, &ZSH_ROWS);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(101)]
    fn zsh_continuation_shape_bytes_thirty_two_through_the_end() {
        changed_continuation(
            Shell::Zsh,
            32..u8::try_from(ZSH.len()).unwrap(),
            false,
            ZSH,
            &ZSH_ROWS,
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(201)]
    fn continuation_requires_a_line_boundary_and_a_unique_assignment() {
        const BARE_BOUNDARY: ContinuationRow = const {
            let mut input = [0u8; NU_BARE.len() + 1];
            let mut index = 0;
            while index < NU_BARE.len() {
                input[index + 1] = NU_BARE[index];
                index += 1;
            }
            continuation_row(Shell::Nu, &input, 0)
        };
        const QUOTED_BOUNDARY: ContinuationRow = const {
            let mut input = [0u8; NU_QUOTED.len() + 1];
            let mut index = 0;
            while index < NU_QUOTED.len() {
                input[index + 1] = NU_QUOTED[index];
                index += 1;
            }
            continuation_row(Shell::Nu, &input, 0)
        };
        let byte: u8 = kani::any();
        assert!(const { continuation(Shell::Nu, NU_BARE).is_some() });
        assert_eq!(
            evaluated_continuation(&BARE_BOUNDARY, 0, byte),
            byte == b'\n'
        );
        let quoted_byte: u8 = kani::any();
        assert!(const { continuation(Shell::Nu, NU_QUOTED).is_some() });
        assert_eq!(
            evaluated_continuation(&QUOTED_BOUNDARY, 0, quoted_byte),
            quoted_byte == b'\n'
        );
        assert!(
            const {
                let mut duplicate = [0u8; NU_BARE.len() * 2];
                let mut index = 0;
                while index < NU_BARE.len() {
                    duplicate[index] = NU_BARE[index];
                    duplicate[index + NU_BARE.len()] = NU_BARE[index];
                    index += 1;
                }
                continuation(Shell::Nu, &duplicate).is_none()
            }
        );
        assert!(
            const {
                let mut duplicate = [0u8; NU_QUOTED.len() * 2];
                let mut index = 0;
                while index < NU_QUOTED.len() {
                    duplicate[index] = NU_QUOTED[index];
                    duplicate[index + NU_QUOTED.len()] = NU_QUOTED[index];
                    index += 1;
                }
                continuation(Shell::Nu, &duplicate).is_none()
            }
        );
        kani::cover!(byte == b'\n');
        kani::cover!(byte != b'\n');
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
        for (shell, invalid) in [
            (Shell::Nu, &b""[..]),
            (Shell::Zsh, &b""[..]),
            (Shell::Nu, &b"    PROMPT_MULTILINE_INDICATOR: (\n        ^/path/starship;touch prompt --continuation\n    )\n"[..]),
            (Shell::Nu, &b"    PROMPT_MULTILINE_INDICATOR: (\n        ^/path/$(touch) prompt --continuation\n    )\n"[..]),
            (Shell::Zsh, &b"PROMPT2='deferred prompt'\n"[..]),
            (Shell::Zsh, &b"PROMPT3=\"$(/native/starship prompt --continuation)\"\n"[..]),
            (Shell::Zsh, &b"PROMPT2=\"$(starship prompt --unknown)\"\n"[..]),
        ] {
            assert_eq!(continuation(shell, invalid), None);
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
    fn continuation_refuses_empty_or_controlled_programs_and_an_unclosed_block() {
        for (shell, input) in [
            (Shell::Nu, &b"    PROMPT_MULTILINE_INDICATOR: (\n        ^'' prompt --continuation\n    )\n"[..]),
            (Shell::Nu, &b"    PROMPT_MULTILINE_INDICATOR: (\n        ^ prompt --continuation\n    )\n"[..]),
            (Shell::Nu, &b"    PROMPT_MULTILINE_INDICATOR: (\n        ^'/p\0' prompt --continuation\n    )\n"[..]),
            (Shell::Nu, &b"    PROMPT_MULTILINE_INDICATOR: (\n        ^'/p' prompt --continuation\n    ) extra\n"[..]),
            (Shell::Zsh, &b"PROMPT2=\"$( prompt --continuation)\"\n"[..]),
            (Shell::Zsh, &b"PROMPT2=\"$(/p\0 prompt --continuation)\"\n"[..]),
            (Shell::Zsh, &b"PROMPT2=\"$(/p$ prompt --continuation)\"\n"[..]),
        ] {
            assert_eq!(continuation(shell, input), None, "{shell:?}: {input:?}");
        }
    }

    #[test]
    fn continuation_preserves_eof_and_the_complete_program_suffix() {
        for (shell, script) in [
            (Shell::Nu, &b"    PROMPT_MULTILINE_INDICATOR: (\n        ^/p prompt --continuation\n    )"[..]),
            (Shell::Nu, &b"    PROMPT_MULTILINE_INDICATOR: (\n        ^'/p $x (q) \"r\"' prompt --continuation\n    )"[..]),
            (Shell::Zsh, &b"PROMPT2=\"$(/path with spaces prompt --continuation)\""[..]),
            (Shell::Zsh, &b"PROMPT2=\"$(/p prompt --continuation q prompt --continuation)\""[..]),
        ] {
            assert_eq!(continuation(shell, script), Some(0..script.len()));
            let mut duplicate = script.to_vec();
            duplicate.push(b'\n');
            duplicate.extend_from_slice(script);
            assert_eq!(continuation(shell, &duplicate), None);
            for suffix in *b"x\r" {
                let mut unknown = script.to_vec();
                unknown.push(suffix);
                assert_eq!(continuation(shell, &unknown), None);
            }
        }
    }

    #[test]
    fn continuation_restarts_at_an_adjacent_header_without_a_final_newline() {
        let header = b"    PROMPT_MULTILINE_INDICATOR: (\n";
        let script =
            b"    PROMPT_MULTILINE_INDICATOR: (\n        ^'/p' prompt --continuation\n    )";
        let mut input = header.to_vec();
        input.extend_from_slice(script);
        assert_eq!(
            continuation(Shell::Nu, &input),
            Some(header.len()..input.len())
        );
        assert_eq!(continuation(Shell::Nu, header), None);
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
