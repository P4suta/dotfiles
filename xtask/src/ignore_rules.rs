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
        if p < p_end && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < p_end && pattern[p] == b'*' {
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
    while p < p_end && pattern[p] == b'*' {
        p += 1;
    }
    p == p_end
}

/// Whether a `.chezmoiignore` or `.chezmoiremove` pattern matches a target path.
/// A `**` segment matches any number of whole segments, `*` and `?` stay inside one segment, and every other byte matches itself.
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
    while index < text.len() && text[index] != b'/' {
        index += 1;
    }
    index
}

fn globstar(pattern: &[u8], start: usize, end: usize) -> bool {
    end == start + 2 && pattern[start] == b'*' && pattern[start + 1] == b'*'
}

/// A pattern stays only when its listing's justification holds and no earlier line already lists it.
/// An ignore pattern needs a target that the source declares, and a remove pattern needs to miss every managed target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Kept,
    Unjustified,
    Repeated,
}

pub const fn verdict(matched: bool, earlier_occurrences: usize) -> Verdict {
    if earlier_occurrences > 0 {
        Verdict::Repeated
    } else if matched {
        Verdict::Kept
    } else {
        Verdict::Unjustified
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

/// Every pattern of up to three bytes, wildcards included, matches every path of up to two bytes without fault.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(7)]
fn a_literal_pattern_matches_only_itself() {
    let (pattern, pattern_length) = any_text::<3>(true);
    let (path, path_length) = any_text::<2>(false);
    let pattern = &pattern[..pattern_length];
    let path = &path[..path_length];
    let wild = count(pattern, b"*?") > 0;
    let result = matches(pattern, path);
    if !wild {
        assert_eq!(result, same(pattern, path));
    }
    kani::cover!(result && wild);
    kani::cover!(!result && wild);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(7)]
fn a_star_stays_inside_one_segment_and_a_globstar_crosses_them() {
    let (path, path_length) = any_text::<2>(false);
    let path = &path[..path_length];
    let crosses = count(path, b"/") > 0;
    assert!(matches(b"**", path));
    let star = matches(b"*", path);
    assert_eq!(star, !crosses);
    assert_eq!(matches(b"?", path), path_length == 1 && !crosses);
    assert!(!matches(b"a/**", path) || (path_length > 0 && path[0] == b'a'));
    kani::cover!(star);
    kani::cover!(!star);
}

/// `verdict` reads only whether a pattern matched, so every match result stands for every pattern and path.
#[cfg(kani)]
#[kani::proof]
fn a_pattern_is_kept_only_when_it_matches_and_appears_first() {
    let found: bool = kani::any();
    let earlier: usize = kani::any();
    let kept = verdict(found, earlier);
    assert_eq!(kept == Verdict::Kept, found && earlier == 0);
    kani::cover!(kept == Verdict::Kept);
    kani::cover!(kept == Verdict::Repeated && found);
}

#[cfg(test)]
mod tests {
    use super::*;

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
