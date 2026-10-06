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
pub fn action(head: Option<&[u8]>, installed: bool, recorded: Option<&[u8]>) -> Action {
    match head {
        None => Action::Clone,
        Some(head) if installed && recorded.is_some_and(|recorded| same(recorded, head)) => {
            Action::Current
        }
        Some(_) => Action::Build,
    }
}

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
fn a_build_is_skipped_only_for_the_installed_recorded_revision() {
    let (head, recorded): ([u8; 2], [u8; 2]) = (kani::any(), kani::any());
    let (head, recorded) = (any_revision(&head), any_revision(&recorded));
    let installed: bool = kani::any();
    let result = action(head, installed, recorded);
    assert_eq!(
        result == Action::Current,
        installed && head.is_some() && recorded.is_some() && same(head.unwrap(), recorded.unwrap())
    );
    kani::cover!(result == Action::Current);
    kani::cover!(result == Action::Build && installed);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(4)]
fn a_missing_checkout_is_always_cloned() {
    let (head, recorded): ([u8; 2], [u8; 2]) = (kani::any(), kani::any());
    let (head, recorded) = (any_revision(&head), any_revision(&recorded));
    let installed: bool = kani::any();
    let result = action(head, installed, recorded);
    assert_eq!(result == Action::Clone, head.is_none());
    kani::cover!(result == Action::Clone && installed);
    kani::cover!(result != Action::Clone);
}

#[cfg(test)]
mod tests {
    use super::*;

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
