//! Which checkouts need an issue context before an implementation edit.

/// An edit needs an issue context unless its checkout has no remote at all and the owner never opted it in with `pr-workflow local init`.
/// A scratch repository has nowhere to publish to, so nothing the issue context protects is at stake.
/// An unknown remote answer keeps the issue context, so a failing Git query never admits an edit.
#[must_use]
pub const fn needs_issue_context(has_remote: Option<bool>, local_provider: bool) -> bool {
    local_provider || !matches!(has_remote, Some(false))
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_needs_issue_context(has_remote: Option<bool>, local_provider: bool) {
        let needed = needs_issue_context(has_remote, local_provider);
        assert_eq!(needed, local_provider || has_remote != Some(false));
        assert!(needs_issue_context(Some(true), local_provider));
        assert!(needs_issue_context(None, local_provider));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_checkout_without_any_remote_or_local_provider_skips_the_issue_context() {
        let has_remote: Option<bool> = kani::any();
        let local_provider: bool = kani::any();
        spec_needs_issue_context(has_remote, local_provider);
        kani::cover!(!needs_issue_context(has_remote, local_provider));
        kani::cover!(needs_issue_context(has_remote, local_provider));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification() {
        for [local_provider] in crate::domain::flags() {
            for has_remote in [None, Some(false), Some(true)] {
                super::proofs::spec_needs_issue_context(has_remote, local_provider);
            }
        }
        assert!(!needs_issue_context(Some(false), false));
        assert!(needs_issue_context(Some(false), true));
    }
}
