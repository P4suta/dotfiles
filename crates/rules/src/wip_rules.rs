/// The number of open PRs one person may keep in a repository.
/// The limit comes from the recorded override of the repository, else the policy default.
#[must_use]
pub const fn limit(default: u32, repository: Option<u32>) -> u32 {
    match repository {
        Some(limit) => limit,
        None => default,
    }
}

/// The rule creates a new PR only while the open PRs of the creator stay below the limit.
/// Work that would exceed the limit joins an open PR instead.
#[must_use]
pub const fn admits(open: u32, limit: u32) -> bool {
    open < limit
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_limit(default: u32, repository: u32) {
        assert_eq!(limit(default, Some(repository)), repository);
        assert_eq!(limit(default, None), default);
    }

    pub fn spec_admits(open: u32, limit: u32) {
        let admitted = admits(open, limit);
        assert_eq!(admitted, open < limit);
        assert!(!admitted || open.saturating_add(1) <= limit);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_recorded_repository_limit_replaces_the_default() {
        let default: u32 = kani::any();
        let repository: u32 = kani::any();
        spec_limit(default, repository);
        kani::cover!(repository < default);
        kani::cover!(repository > default);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_pr_is_created_only_below_the_limit() {
        let open: u32 = kani::any();
        let limit: u32 = kani::any();
        spec_admits(open, limit);
        kani::cover!(admits(open, limit));
        kani::cover!(!admits(open, limit) && open == limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDARIES: [u32; 5] = [0, 1, 2, u32::MAX - 1, u32::MAX];

    #[test]
    fn every_boundary_meets_the_specification() {
        for first in BOUNDARIES {
            for second in BOUNDARIES {
                proofs::spec_limit(first, second);
                proofs::spec_admits(first, second);
            }
        }
    }

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
