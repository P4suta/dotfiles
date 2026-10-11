/// Whether `pattern[p_start..p_end]` matches `text[t_start..t_end]`, where `*` matches any run of bytes and `?` one byte.
/// Offsets instead of subslices keep the proof's formula small enough for the proof budget.
fn segment_matches(
    pattern: &[u8],
    (p_start, p_end): (usize, usize),
    text: &[u8],
    (t_start, t_end): (usize, usize),
) -> bool {
    let (mut p, mut t) = (p_start, t_start);
    let mut star: Option<usize> = None;
    let mut mark = t_start;
    while t < t_end {
        // The cursor never passes the end, since it moves only past a byte it read.
        let current = if p == p_end {
            None
        } else {
            pattern.get(p).copied()
        };
        if current.is_some() && (current == Some(b'?') || current == text.get(t).copied()) {
            p += 1;
            t += 1;
        } else if current == Some(b'*') {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(position) = star {
            p = position + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p != p_end && pattern.get(p) == Some(&b'*') {
        p += 1;
    }
    p == p_end
}

/// Whether a `.chezmoiignore` or `.chezmoiremove` pattern matches a target path.
/// A `**` segment matches any number of whole segments, `*` and `?` stay inside one segment, and every other byte matches itself.
#[must_use]
pub fn matches(pattern: &[u8], path: &[u8]) -> bool {
    // Each cursor holds the offset where a segment starts, and one past the length marks a fully read text.
    let (pattern_done, path_done) = (pattern.len() + 1, path.len() + 1);
    let (mut p, mut t) = (0, 0);
    let mut star: Option<usize> = None;
    let mut mark = 0;
    while t < path_done {
        let t_end = segment_end(path, t);
        if p < pattern_done {
            let p_end = segment_end(pattern, p);
            if globstar(pattern, p, p_end) {
                star = Some(p_end + 1);
                mark = t;
                p = p_end + 1;
                continue;
            }
            if segment_matches(pattern, (p, p_end), path, (t, t_end)) {
                p = p_end + 1;
                t = t_end + 1;
                continue;
            }
        }
        match star {
            Some(resume) => {
                p = resume;
                mark = segment_end(path, mark) + 1;
                t = mark;
            }
            None => return false,
        }
    }
    while p < pattern_done {
        let p_end = segment_end(pattern, p);
        if !globstar(pattern, p, p_end) {
            return false;
        }
        p = p_end + 1;
    }
    true
}

/// The offset of the `/` that ends the segment starting at `start`, or the length.
fn segment_end(text: &[u8], start: usize) -> usize {
    let mut index = start;
    while let Some(&byte) = text.get(index) {
        if byte == b'/' {
            break;
        }
        index += 1;
    }
    index
}

/// The bytes of `**`.
const GLOBSTAR_LENGTH: usize = 2;

fn globstar(pattern: &[u8], start: usize, end: usize) -> bool {
    end == start + GLOBSTAR_LENGTH
        && pattern.get(start) == Some(&b'*')
        && pattern.get(start + 1) == Some(&b'*')
}

/// A pattern stays only when its listing's justification holds and no earlier line already lists it.
/// An ignore pattern needs a target that the source declares, and a remove pattern needs to miss every managed target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Kept,
    Unjustified,
    Repeated,
}

#[must_use]
pub const fn verdict(matched: bool, earlier_occurrences: usize) -> Verdict {
    if earlier_occurrences > 0 {
        Verdict::Repeated
    } else if matched {
        Verdict::Kept
    } else {
        Verdict::Unjustified
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// A pattern without wildcards matches only itself.
    pub fn spec_literal(pattern: &[u8], path: &[u8]) -> bool {
        let result = matches(pattern, path);
        if count(pattern, b"*?") == 0 {
            assert_eq!(result, same(pattern, path));
        }
        result
    }

    /// How many bytes of `text` appear in `wanted`, counted without iterators so the proof stays small.
    #[expect(
        clippy::indexing_slicing,
        reason = "the loop bounds keep every index in range, and indexing keeps the proof inside its budget"
    )]
    fn count(text: &[u8], wanted: &[u8]) -> usize {
        let mut found = 0;
        let mut index = 0;
        while index < text.len() {
            let mut choice = 0;
            while choice < wanted.len() {
                if text[index] == wanted[choice] {
                    found += 1;
                }
                choice += 1;
            }
            index += 1;
        }
        found
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "the length check and the loop bound keep every index in range, and indexing keeps the proof inside its budget"
    )]
    fn same(left: &[u8], right: &[u8]) -> bool {
        if left.len() != right.len() {
            return false;
        }
        let mut index = 0;
        while index < left.len() {
            if left[index] != right[index] {
                return false;
            }
            index += 1;
        }
        true
    }

    /// A star stays inside one segment, a mark matches one byte, and a globstar crosses segments.
    pub fn spec_wildcards(path: &[u8]) -> bool {
        let crosses = count(path, b"/") > 0;
        assert!(matches(b"**", path));
        let star = matches(b"*", path);
        assert_eq!(star, !crosses);
        assert_eq!(matches(b"?", path), path.len() == 1 && !crosses);
        assert!(!matches(b"a/**", path) || path.first() == Some(&b'a'));
        star
    }

    pub fn spec_verdict(found: bool, earlier: usize) {
        let kept = verdict(found, earlier);
        assert_eq!(kept == Verdict::Kept, found && earlier == 0);
        assert_eq!(kept == Verdict::Repeated, earlier > 0);
        assert_eq!(kept == Verdict::Unjustified, !found && earlier == 0);
    }

    #[cfg(kani)]
    fn any_text<const N: usize>(wildcards: bool) -> ([u8; N], usize) {
        let bytes: [u8; N] = kani::any();
        let mut index = 0;
        while index < N {
            let byte = bytes[index];
            kani::assume(
                byte == b'a' || byte == b'/' || (wildcards && (byte == b'*' || byte == b'?')),
            );
            index += 1;
        }
        let length: usize = kani::any();
        kani::assume(length <= N);
        (bytes, length)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(7)]
    fn a_star_stays_inside_one_segment_and_a_globstar_crosses_them() {
        let (path, path_length) = any_text::<2>(false);
        let path = &path[..path_length];
        let star = spec_wildcards(path);
        kani::cover!(star);
        kani::cover!(!star);
    }

    /// `verdict` reads only whether a pattern matched, so every match result stands for every pattern and path.
    #[cfg(kani)]
    #[kani::proof]
    fn a_pattern_is_kept_only_when_it_matches_and_appears_first() {
        let found: bool = kani::any();
        let earlier: usize = kani::any();
        spec_verdict(found, earlier);
        let result = verdict(found, earlier);
        kani::cover!(result == Verdict::Kept);
        kani::cover!((result == Verdict::Repeated) & found);
    }
}

#[cfg(kani)]
fn any_text<const N: usize>(wildcards: bool) -> ([u8; N], usize) {
    let bytes: [u8; N] = kani::any();
    let mut index = 0;
    while index < N {
        let byte = bytes[index];
        kani::assume(byte == b'a' || byte == b'/' || (wildcards && (byte == b'*' || byte == b'?')));
        index += 1;
    }
    let length: usize = kani::any();
    kani::assume(length <= N);
    (bytes, length)
}

#[cfg(kani)]
fn count(text: &[u8], wanted: &[u8]) -> usize {
    let mut found = 0;
    let mut index = 0;
    while index < text.len() {
        let mut choice = 0;
        while choice < wanted.len() {
            if text[index] == wanted[choice] {
                found += 1;
            }
            choice += 1;
        }
        index += 1;
    }
    found
}

#[cfg(kani)]
fn same(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// The length partitions keep every pattern of up to three bytes and every path of up to two bytes.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(7)]
fn a_literal_pattern_matches_only_itself() {
    let (pattern, pattern_length) = any_text::<1>(true);
    let (path, path_length) = any_text::<2>(false);
    let pattern = &pattern[..pattern_length];
    let path = &path[..path_length];
    let wild = count(pattern, b"*?") > 0;
    let result = matches(pattern, path);
    if !wild {
        assert_eq!(result, same(pattern, path));
    }
    kani::cover!(result & wild);
    kani::cover!(!result & wild);
}

#[cfg(kani)]
mod literal_proofs {
    use super::*;

    #[kani::proof]
    #[kani::unwind(7)]
    fn a_two_byte_literal_pattern_matches_only_itself() {
        let (pattern, pattern_length) = any_text::<3>(true);
        kani::assume(pattern_length == 2);
        let (path, path_length) = any_text::<2>(false);
        let pattern = &pattern[..pattern_length];
        let path = &path[..path_length];
        let wild = count(pattern, b"*?") > 0;
        let result = matches(pattern, path);
        if !wild {
            assert_eq!(result, same(pattern, path));
        }
        kani::cover!(result & wild);
        kani::cover!(!result & wild);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(7)]
    fn a_three_byte_literal_pattern_matches_only_itself() {
        let (pattern, pattern_length) = any_text::<3>(true);
        kani::assume(pattern_length == 3);
        let (path, path_length) = any_text::<2>(false);
        let pattern = &pattern[..pattern_length];
        let path = &path[..path_length];
        let wild = count(pattern, b"*?") > 0;
        let result = matches(pattern, path);
        if !wild {
            assert_eq!(result, same(pattern, path));
        }
        kani::cover!(result & wild);
        kani::cover!(!result & wild);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::words;
    use alloc::vec::Vec;

    /// A direct recursive reading of the pattern language, which the tests hold the matcher to.
    fn segment(pattern: &[u8], text: &[u8]) -> bool {
        match pattern.split_first() {
            None => text.is_empty(),
            Some((b'*', rest)) => {
                segment(rest, text)
                    || text
                        .split_first()
                        .is_some_and(|(_, tail)| segment(pattern, tail))
            }
            Some((b'?', rest)) => text
                .split_first()
                .is_some_and(|(_, tail)| segment(rest, tail)),
            Some((byte, rest)) => text
                .split_first()
                .is_some_and(|(first, tail)| first == byte && segment(rest, tail)),
        }
    }

    fn segments(pattern: &[&[u8]], path: &[&[u8]]) -> bool {
        match pattern.split_first() {
            None => path.is_empty(),
            Some((&b"**", rest)) => {
                segments(rest, path)
                    || path
                        .split_first()
                        .is_some_and(|(_, tail)| segments(pattern, tail))
            }
            Some((first, rest)) => path
                .split_first()
                .is_some_and(|(head, tail)| segment(first, head) && segments(rest, tail)),
        }
    }

    fn oracle(pattern: &[u8], path: &[u8]) -> bool {
        let pattern: Vec<&[u8]> = pattern.split(|&byte| byte == b'/').collect();
        let path: Vec<&[u8]> = path.split(|&byte| byte == b'/').collect();
        segments(&pattern, &path)
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every pattern and path")]
    fn every_short_pattern_and_path_matches_as_the_pattern_language_reads() {
        let paths = words(b"ab/", 4);
        for pattern in words(b"a/*?", 4) {
            for path in &paths {
                assert_eq!(
                    matches(&pattern, path),
                    oracle(&pattern, path),
                    "{pattern:?} {path:?}"
                );
                proofs::spec_literal(&pattern, path);
            }
        }
        for path in &paths {
            proofs::spec_wildcards(path);
        }
        for found in [false, true] {
            for earlier in [0, 1, 2, usize::MAX] {
                proofs::spec_verdict(found, earlier);
            }
        }
    }

    #[test]
    fn globstars_cover_zero_or_more_segments() {
        assert!(matches(b"guard/**", b"guard"));
        assert!(matches(b"guard/**", b"guard/src/main.rs"));
        assert!(matches(b"**/.DS_Store", b".DS_Store"));
        assert!(matches(b"**/.DS_Store", b"a/b/.DS_Store"));
        assert!(!matches(b"guard/**", b"guards"));
    }

    #[test]
    fn stars_and_marks_stay_inside_a_segment() {
        assert!(matches(b".zcompdump*", b".zcompdump-host-5.9"));
        assert!(matches(b"*.md", b"README.md"));
        assert!(!matches(b"*.md", b"docs/README.md"));
        assert!(matches(
            b"Library/LaunchAgents/dev.?otfiles.*",
            b"Library/LaunchAgents/dev.dotfiles.x.plist"
        ));
        assert!(!matches(b".config/bat", b".config/bat/config"));
        assert!(matches(b"a*b*c", b"aXbYc"));
        assert!(!matches(b"a*b", b"ab/b"));
    }

    #[test]
    fn only_a_first_matching_occurrence_is_kept() {
        assert_eq!(verdict(true, 0), Verdict::Kept);
        assert_eq!(verdict(false, 0), Verdict::Unjustified);
        assert_eq!(verdict(true, 1), Verdict::Repeated);
    }
}
