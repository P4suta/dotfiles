/// How many open PRs one author may hold in a repository: the repository's recorded override, else the policy default.
pub const fn limit(default: u32, repository: Option<u32>) -> u32 {
    match repository {
        Some(limit) => limit,
        None => default,
    }
}

/// A new PR is created only while the author's open PRs stay below the limit, so work that would exceed it joins an open PR instead.
pub const fn admits(open: u32, limit: u32) -> bool {
    open < limit
}

#[cfg(kani)]
#[kani::proof]
fn a_recorded_repository_limit_replaces_the_default() {
    let default: u32 = kani::any();
    let repository: u32 = kani::any();
    assert_eq!(limit(default, Some(repository)), repository);
    assert_eq!(limit(default, None), default);
    kani::cover!(default != repository);
}

#[cfg(kani)]
#[kani::proof]
fn a_pr_is_created_only_below_the_limit() {
    let open: u32 = kani::any();
    let limit: u32 = kani::any();
    let admitted = admits(open, limit);
    assert_eq!(admitted, open < limit);
    assert!(!admitted || open.saturating_add(1) <= limit);
    kani::cover!(admitted);
    kani::cover!(!admitted && open == limit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_limit_of_one_refuses_a_second_open_pr() {
        let limit = limit(1, None);
        assert!(admits(0, limit));
        assert!(!admits(1, limit));
        assert!(!admits(2, limit));
    }

    #[test]
    fn a_repository_override_decides_instead_of_the_default() {
        assert_eq!(limit(1, Some(3)), 3);
        assert!(admits(2, limit(1, Some(3))));
        assert!(!admits(3, limit(1, Some(3))));
        assert!(!admits(0, limit(1, Some(0))));
    }
}
