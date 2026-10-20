//! Which repository paths hold the owner's local workflow state, which no commit in any repository may add or change.

/// The directory an earlier `pr-workflow` kept its issue contexts in, inside the working tree.
/// Workflow evidence now lives in the owner's state directory, so nothing under this name belongs in a repository.
pub const PR_WORKFLOW: &[u8] = b".pr-workflow";

/// Whether `path`, a repository-relative path with `/` separators, names the local state directory or lies inside it at any depth.
#[must_use]
pub fn is_local_state(path: &[u8]) -> bool {
    path.split(|byte| *byte == b'/')
        .any(|component| component == PR_WORKFLOW)
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// The same decision written as a scan for a whole component, independent of `split`.
    fn holds_component(path: &[u8]) -> bool {
        let width = PR_WORKFLOW.len();
        let mut start = 0;
        while start + width <= path.len() {
            let before = start == 0 || path.get(start - 1) == Some(&b'/');
            let after = start + width == path.len() || path.get(start + width) == Some(&b'/');
            if before && after && path.get(start..start + width) == Some(PR_WORKFLOW) {
                return true;
            }
            start += 1;
        }
        false
    }

    pub fn spec_is_local_state(path: &[u8]) {
        assert_eq!(is_local_state(path), holds_component(path));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(17)]
    fn only_a_whole_local_state_component_counts() {
        let bytes: [u8; 16] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        let path = &bytes[..length];
        spec_is_local_state(path);
        kani::cover!(is_local_state(path));
        kani::cover!(!is_local_state(path) && path.len() > PR_WORKFLOW.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_whole_local_state_component_counts() {
        for path in [
            &b".pr-workflow"[..],
            b".pr-workflow/contexts.json",
            b"nested/.pr-workflow/contexts.json",
        ] {
            assert!(is_local_state(path), "{path:?}");
            proofs::spec_is_local_state(path);
        }
        for path in [
            &b""[..],
            b"src/main.rs",
            b".pr-workflows/x",
            b"x.pr-workflow/y",
            b"docs/pr-workflow.md",
            b".github/pr-workflow.yml",
        ] {
            assert!(!is_local_state(path), "{path:?}");
            proofs::spec_is_local_state(path);
        }
    }
}
