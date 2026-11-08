#[must_use]
pub const fn consume(remaining: &mut usize) -> bool {
    match remaining.checked_sub(1) {
        Some(next) => {
            *remaining = next;
            true
        }
        None => false,
    }
}

const ASCII_DELETE: u8 = 0x7f;

fn segment_valid(segment: &[u8]) -> bool {
    !segment.is_empty()
        && segment != b"."
        && segment != b".."
        && !matches!(segment.last(), Some(b'.' | b' '))
}

#[must_use]
pub fn normalized_relative(path: &[u8]) -> bool {
    let mut start = 0;
    for (index, byte) in path.iter().copied().enumerate() {
        if byte < b' ' || matches!(byte, b'\\' | b':' | ASCII_DELETE) {
            return false;
        }
        if byte == b'/' {
            if !path.get(start..index).is_some_and(segment_valid) {
                return false;
            }
            start = index + 1;
        }
    }
    path.get(start..).is_some_and(segment_valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_publisher_occurrence_cannot_authorize_a_second_occurrence() {
        let mut remaining = 1;
        assert!(consume(&mut remaining));
        assert!(!consume(&mut remaining));
        assert_eq!(remaining, 0);
    }
    #[test]
    fn publisher_paths_have_one_platform_independent_spelling() {
        for path in [
            b"src/lib.rs".as_slice(),
            b".cargo_vcs_info.json",
            b"a/file with spaces.md",
        ] {
            assert!(normalized_relative(path));
        }
        for path in [
            b"".as_slice(),
            b"/absolute",
            b"../escape",
            b"a/../b",
            b"a//b",
            b"a/./b",
            b"a/b/",
            b"c:relative",
            b"a\\b",
            b"a/ ",
            b"a/b.",
            b"a/\0b",
        ] {
            assert!(!normalized_relative(path));
        }
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;
    #[kani::proof]
    fn consuming_an_occurrence_never_underflows_or_authorizes_an_extra_occurrence() {
        let before: usize = kani::any();
        let mut remaining = before;
        let inherited = consume(&mut remaining);
        assert_eq!(inherited, before != 0);
        assert_eq!(remaining, before.saturating_sub(1));
        kani::cover!(inherited);
        kani::cover!(!inherited);
    }
    #[kani::proof]
    #[kani::unwind(20)]
    fn relative_paths_match_the_portable_component_grammar() {
        let bytes: [u8; 16] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        let path = &bytes[..length];
        let reference = !path.is_empty()
            && path.first() != Some(&b'/')
            && !matches!(path.last(), Some(b'/' | b'.' | b' '))
            && path
                .iter()
                .all(|byte| *byte >= b' ' && ![b'\\', b':', 0x7f].contains(byte))
            && path
                .windows(2)
                .all(|pair| !matches!(pair, [b'/' | b'.' | b' ', b'/']));
        assert_eq!(normalized_relative(path), reference);
        kani::cover!(reference && length > 2);
        kani::cover!(!reference && length > 2);
    }
}
