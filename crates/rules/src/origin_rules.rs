const GITHUB_REMOTE_PREFIXES: [&[u8]; 3] = [
    b"https://github.com/",
    b"git@github.com:",
    b"ssh://git@github.com/",
];

fn after<'a>(mut text: &'a [u8], mut prefix: &[u8]) -> Option<&'a [u8]> {
    while let [expected, rest @ ..] = prefix {
        let [actual, tail @ ..] = text else {
            return None;
        };
        if actual != expected {
            return None;
        }
        (text, prefix) = (tail, rest);
    }
    Some(text)
}

fn before<'a>(mut text: &'a [u8], mut suffix: &[u8]) -> Option<&'a [u8]> {
    while let [rest @ .., expected] = suffix {
        let [head @ .., actual] = text else {
            return None;
        };
        if actual != expected {
            return None;
        }
        (text, suffix) = (head, rest);
    }
    Some(text)
}

fn valid(mut part: &[u8]) -> bool {
    if matches!(part, [] | [b'.'] | [b'.', b'.']) {
        return false;
    }
    while let [byte, rest @ ..] = part {
        if !byte.is_ascii_alphanumeric() && !matches!(byte, b'-' | b'_' | b'.') {
            return false;
        }
        part = rest;
    }
    true
}

fn github_range(url: &[u8]) -> Option<core::ops::Range<usize>> {
    let mut prefixes: &[&[u8]] = &GITHUB_REMOTE_PREFIXES;
    while let [prefix, rest @ ..] = prefixes {
        if let Some(path) = after(url, prefix) {
            let start = url.len() - path.len();
            let path = before(path, b"/").unwrap_or(path);
            let path = before(path, b".git").unwrap_or(path);
            let mut name = path;
            let mut owner_length = 0;
            while let [byte, tail @ ..] = name {
                if *byte == b'/' {
                    break;
                }
                name = tail;
                owner_length += 1;
            }
            let [b'/', name @ ..] = name else {
                return None;
            };
            let owner = path.get(..owner_length)?;
            return (valid(owner) && valid(name)).then_some(start..start + path.len());
        }
        prefixes = rest;
    }
    None
}

fn trim_ascii(mut text: &[u8]) -> &[u8] {
    while let [byte, rest @ ..] = text {
        if !byte.is_ascii_whitespace() {
            break;
        }
        text = rest;
    }
    while let [rest @ .., byte] = text {
        if !byte.is_ascii_whitespace() {
            break;
        }
        text = rest;
    }
    text
}

fn equal(mut left: &[u8], mut right: &[u8], ignore_case: bool) -> bool {
    while let ([first, left_rest @ ..], [second, right_rest @ ..]) = (left, right) {
        if first != second && !(ignore_case && first.eq_ignore_ascii_case(second)) {
            return false;
        }
        (left, right) = (left_rest, right_rest);
    }
    left.is_empty() && right.is_empty()
}

/// The validated GitHub owner and repository, without a transport or `.git` suffix.
#[must_use]
pub fn github_slug(url: &str) -> Option<&str> {
    url.get(github_range(url.as_bytes())?)
}

/// GitHub identity ignores transport, suffix, and ASCII case.
/// Other origins compare exactly.
#[must_use]
pub fn same_origin(stored: &str, current: &str) -> bool {
    let (stored, current) = (stored.as_bytes(), current.as_bytes());
    let (left, right) = (trim_ascii(stored), trim_ascii(current));
    match (github_range(left), github_range(right)) {
        (Some(left_range), Some(right_range)) => {
            match (left.get(left_range), right.get(right_range)) {
                (Some(left), Some(right)) => equal(left, right, true),
                _ => false,
            }
        }
        _ => equal(stored, current, false),
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const ORIGINS: [(&str, u8); 10] = [
        ("https://github.com/Owner/Repo.git\n", 0),
        ("ssh://git@github.com/owner/repo", 0),
        ("git@github.com:OWNER/REPO.git", 0),
        ("https://github.com/owner/repo/", 0),
        ("https://github.com/owner/other.git", 1),
        ("git@github.com:other/repo.git", 2),
        ("https://example.com/owner/repo.git", 3),
        ("git@example.com:owner/repo.git", 4),
        ("/local/owner/repo.git", 5),
        ("https://github.com/owner/repo/extra", 6),
    ];

    pub fn spec_identity(stored: (&str, u8), current: (&str, u8)) -> bool {
        let stored_url = stored.0.strip_suffix("\n").unwrap_or(stored.0);
        assert_eq!(github_slug(stored_url).is_some(), stored.1 <= 2);
        let same = same_origin(stored.0, current.0);
        assert_eq!(same, stored.1 == current.1);
        same
    }

    #[cfg(kani)]
    fn bounded_origin_forms(start: usize, end: usize) {
        let stored = usize::from(kani::any::<u8>());
        let current = usize::from(kani::any::<u8>());
        kani::assume(start <= stored && stored < end && current < ORIGINS.len());
        let same = spec_identity(ORIGINS[stored], ORIGINS[current]);
        kani::cover!((start != 0 || stored != current) && same);
        kani::cover!(!same);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(40)]
    fn bounded_github_origin_forms_match_only_the_same_repository() {
        bounded_origin_forms(0, ORIGINS.len() / 2);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(40)]
    fn bounded_remaining_origin_forms_match_only_the_same_repository() {
        bounded_origin_forms(ORIGINS.len() / 2, ORIGINS.len());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(5)]
    fn bounded_non_github_origins_keep_exact_comparison() {
        let stored: [u8; 4] = kani::any();
        let current: [u8; 4] = kani::any();
        let (Ok(stored), Ok(current)) = (
            core::str::from_utf8(&stored),
            core::str::from_utf8(&current),
        ) else {
            return;
        };
        let same = same_origin(stored, current);
        assert_eq!(same, stored == current);
        kani::cover!(same);
        kani::cover!(!same);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bounded_origin_pair_meets_the_identity_specification() {
        for stored in proofs::ORIGINS {
            for current in proofs::ORIGINS {
                proofs::spec_identity(stored, current);
            }
        }
    }

    #[test]
    fn github_slugs_validate_both_components_and_preserve_their_spelling() {
        assert_eq!(
            github_slug("ssh://git@github.com/Owner-1/Repo_2.git/"),
            Some("Owner-1/Repo_2")
        );
        for invalid in [
            "https://github.com/owner",
            "https://github.com//repo",
            "git@github.com:owner/",
            "https://github.com/./repo",
            "https://github.com/owner/..",
            "https://github.com/owner/repo/extra",
            "https://github.com/owner/repo?query",
            "https://github.com/owner/repo#fragment",
            "https://github.com/owner/repo with spaces",
            "https://github.com/owner/répo",
            "https://github.com.example.com/owner/repo",
        ] {
            assert_eq!(github_slug(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn non_github_origins_never_normalize_case_suffix_or_transport() {
        for (stored, current) in [
            (
                "https://example.com/Owner/Repo.git",
                "https://example.com/owner/repo.git",
            ),
            (
                "https://example.com/owner/repo.git",
                "https://example.com/owner/repo",
            ),
            (
                "git@example.com:owner/repo.git",
                "https://example.com/owner/repo.git",
            ),
            ("/local/owner/repo", "/local/owner/repo.git"),
        ] {
            assert!(same_origin(stored, stored));
            assert!(!same_origin(stored, current));
            assert!(!same_origin(current, stored));
        }
    }
}
