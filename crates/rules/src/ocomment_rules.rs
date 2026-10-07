/// What an always-run ocomment installation does on one apply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    /// No checkout exists, so the step clones the source and builds it.
    Clone,
    /// The checkout moved, the binary went missing, or no record names a build, so the step rebuilds the binary.
    Build,
    /// The recorded build of the installed binary matches the checkout's current revision.
    Current,
}

/// `head` names the checkout's revision when one exists, and `recorded` the revision of the last installation's build.
#[must_use]
pub fn action(head: Option<&[u8]>, installed: bool, recorded: Option<&[u8]>) -> Action {
    match head {
        None => Action::Clone,
        Some(head) if installed && recorded.is_some_and(|recorded| same(recorded, head)) => {
            Action::Current
        }
        Some(_) => Action::Build,
    }
}

fn same(mut left: &[u8], mut right: &[u8]) -> bool {
    loop {
        match (left, right) {
            ([], []) => return true,
            ([first, left_rest @ ..], [second, right_rest @ ..]) if first == second => {
                (left, right) = (left_rest, right_rest);
            }
            _ => return false,
        }
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// Byte equality by index, so the proof stays free of the slice comparison's library code.
    fn equal(left: &[u8], right: &[u8]) -> bool {
        let mut index = 0;
        while let Some(byte) = left.get(index) {
            if right.get(index) != Some(byte) {
                return false;
            }
            index += 1;
        }
        left.len() == right.len()
    }

    pub fn spec_action(head: Option<&[u8]>, installed: bool, recorded: Option<&[u8]>) {
        let result = action(head, installed, recorded);
        let current = match (head, recorded) {
            (Some(head), Some(recorded)) => installed && equal(head, recorded),
            _ => false,
        };
        assert_eq!(result == Action::Current, current);
        assert_eq!(result == Action::Clone, head.is_none());
        assert_eq!(result == Action::Build, head.is_some() && !current);
    }

    #[cfg(kani)]
    fn any_revision(bytes: &[u8; 2]) -> Option<&[u8]> {
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        if kani::any() {
            Some(&bytes[..length])
        } else {
            None
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn a_build_is_skipped_only_for_the_installed_recorded_revision_and_a_missing_checkout_is_cloned()
     {
        let (head, recorded): ([u8; 2], [u8; 2]) = (kani::any(), kani::any());
        let (head, recorded) = (any_revision(&head), any_revision(&recorded));
        let installed: bool = kani::any();
        spec_action(head, installed, recorded);
        let result = action(head, installed, recorded);
        kani::cover!(result == Action::Current);
        kani::cover!(result == Action::Build && installed);
        kani::cover!(result == Action::Clone && installed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_short_revision_meets_the_specification() {
        let revisions = crate::domain::words(b"ab", 2);
        let options: alloc::vec::Vec<Option<&[u8]>> = core::iter::once(None)
            .chain(revisions.iter().map(|revision| Some(revision.as_slice())))
            .collect();
        for &head in &options {
            for &recorded in &options {
                for installed in [false, true] {
                    proofs::spec_action(head, installed, recorded);
                }
            }
        }
    }

    #[test]
    fn only_the_installed_build_of_the_current_revision_is_skipped() {
        let (a, b): (&[u8], &[u8]) = (b"a", b"b");
        assert_eq!(action(None, true, Some(a)), Action::Clone);
        assert_eq!(action(Some(a), true, Some(a)), Action::Current);
        assert_eq!(action(Some(b), true, Some(a)), Action::Build);
        assert_eq!(action(Some(a), false, Some(a)), Action::Build);
        assert_eq!(action(Some(a), true, None), Action::Build);
    }
}
