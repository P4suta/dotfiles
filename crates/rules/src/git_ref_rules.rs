//! Which forced Git operations lose nothing the owner keeps.

/// The ref namespaces whose deletion loses work: branches, tags, notes, and the stash.
const PROTECTED: [&[u8]; 4] = [b"refs/heads/", b"refs/tags/", b"refs/notes/", b"refs/stash"];

/// The prefix of every full ref name.
const REFS: &[u8] = b"refs/";

/// Whether `git update-ref -d` may delete `name`, a ref that a tool owns, such as `refs/codex/turn-diffs/x`.
/// `HEAD` and a short name resolve to a branch, so only a full name outside the protected namespaces qualifies.
#[must_use]
pub fn deletable(name: &[u8]) -> bool {
    name.starts_with(REFS) && !PROTECTED.iter().any(|prefix| name.starts_with(prefix))
}

/// Whether the guard admits an operation it refuses elsewhere, in a repository whose Git directory lies under the temporary directory.
/// A push leaves the throwaway repository, so it keeps every refusal.
#[must_use]
pub const fn throwaway_admits(under_temporary: Option<bool>, pushes: bool) -> bool {
    matches!(under_temporary, Some(true)) && !pushes
}

/// Whether the guard admits `git checkout --force -- <paths>`, which Lefthook runs to hide the unstaged hunks of partly staged files.
/// Only the bare pathspec form qualifies, and only when Lefthook's saved patch holds every unstaged change of those paths, so the owner keeps every hunk it discards.
#[must_use]
pub const fn saved_discard_admits(pathspec_only: bool, saved: Option<bool>) -> bool {
    pathspec_only && matches!(saved, Some(true))
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_saved_discard(pathspec_only: bool, saved: Option<bool>) {
        assert_eq!(
            saved_discard_admits(pathspec_only, saved),
            pathspec_only && saved == Some(true)
        );
        assert!(!saved_discard_admits(false, saved));
        assert!(!saved_discard_admits(pathspec_only, None));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_forced_checkout_passes_only_when_its_discarded_hunks_are_saved() {
        let pathspec_only: bool = kani::any();
        let saved: Option<bool> = kani::any();
        spec_saved_discard(pathspec_only, saved);
        kani::cover!(saved_discard_admits(pathspec_only, saved));
        kani::cover!(!saved_discard_admits(pathspec_only, saved));
    }

    pub fn spec_deletable(name: &[u8]) {
        let full = name.get(..REFS.len()) == Some(REFS);
        let protected = PROTECTED
            .iter()
            .any(|prefix| name.get(..prefix.len()) == Some(*prefix));
        assert_eq!(deletable(name), full && !protected);
    }

    pub fn spec_throwaway(under_temporary: Option<bool>, pushes: bool) {
        assert_eq!(
            throwaway_admits(under_temporary, pushes),
            under_temporary == Some(true) && !pushes
        );
        assert!(!throwaway_admits(under_temporary, true));
        assert!(!throwaway_admits(None, pushes));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(13)]
    fn only_a_full_unprotected_ref_is_deletable() {
        let bytes: [u8; 12] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        let name = &bytes[..length];
        spec_deletable(name);
        kani::cover!(deletable(name));
        kani::cover!(!deletable(name) && name.starts_with(REFS));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_throwaway_repository_never_admits_a_push() {
        let under_temporary: Option<bool> = kani::any();
        let pushes: bool = kani::any();
        spec_throwaway(under_temporary, pushes);
        kani::cover!(throwaway_admits(under_temporary, pushes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification() {
        for name in [
            &b""[..],
            b"HEAD",
            b"main",
            b"refs/",
            b"refs/heads/main",
            b"refs/tags/v1",
            b"refs/notes/hosts",
            b"refs/stash",
            b"refs/codex/turn-diffs/1",
            b"refs/remotes/origin/main",
            b"refs/heads",
        ] {
            proofs::spec_deletable(name);
        }
        for under_temporary in [None, Some(false), Some(true)] {
            for pushes in [false, true] {
                proofs::spec_throwaway(under_temporary, pushes);
                proofs::spec_saved_discard(pushes, under_temporary);
            }
        }
    }

    #[test]
    fn a_tool_ref_is_deletable_and_owner_refs_are_not() {
        assert!(deletable(b"refs/codex/turn-diffs/x"));
        assert!(deletable(b"refs/remotes/origin/topic"));
        for name in [
            &b"refs/heads/main"[..],
            b"refs/tags/v1",
            b"refs/notes/hosts",
            b"refs/stash",
            b"HEAD",
            b"topic",
        ] {
            assert!(!deletable(name), "{name:?}");
        }
        assert!(throwaway_admits(Some(true), false));
        assert!(!throwaway_admits(Some(true), true));
        assert!(!throwaway_admits(Some(false), false));
    }
}
