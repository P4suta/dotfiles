#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssessmentChange {
    Keep,
    Resign,
    Reassess,
}

/// `same_change` holds when every line of both diffs that `patch_line` classifies as a change matches.
#[must_use]
pub const fn assessment_change(
    same_binding: bool,
    same_change: bool,
    same_identity: bool,
) -> AssessmentChange {
    if !same_change || !same_identity {
        AssessmentChange::Reassess
    } else if same_binding {
        AssessmentChange::Keep
    } else {
        AssessmentChange::Resign
    }
}

/// The part one line of a `git diff --binary --full-index` patch plays in the change it describes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchLine {
    /// A file header, a mode, a binary payload, or a hunk's context, added, or removed line.
    Change,
    /// A hunk header, whose line numbers and section heading move when the base edits another region of the file.
    HunkStart,
    /// The blob names of a text file, which move with every edit of the file while its hunks describe the change.
    TextBlobs,
}

const HUNK_HEADER: &[u8] = b"@@ ";
const BLOB_NAMES: &[u8] = b"index ";
const OLD_TEXT_FILE: &[u8] = b"--- ";

/// The part `line` plays, given the `next` line of the same patch.
/// Only a text file's header follows its blob names with `--- `, because a binary file follows them with its payload.
#[must_use]
pub fn patch_line(line: &[u8], next: Option<&[u8]>) -> PatchLine {
    if line.starts_with(HUNK_HEADER) {
        PatchLine::HunkStart
    } else if line.starts_with(BLOB_NAMES)
        && next.is_some_and(|next| next.starts_with(OLD_TEXT_FILE))
    {
        PatchLine::TextBlobs
    } else {
        PatchLine::Change
    }
}

/// One commit that the live PR head holds beyond the local branch and the base that the head contains.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeldCommit {
    /// The commit has exactly two parents.
    pub merge: bool,
    /// Its first parent is the local branch head or another held commit.
    pub follows_branch: bool,
    /// The base contains its second parent.
    pub merges_base: bool,
}

/// How the live PR head relates to the local branch of the same name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveHead {
    /// The local branch points at the live head.
    Branch,
    /// The live head extends the local branch only through merges of its base, as GitHub's update merge does.
    BaseUpdate,
    /// The live head holds other work, or the walk over its held commits stopped early.
    Diverged,
}

/// `held` lists every commit of the live head outside the local branch and the base, when `complete` holds.
#[must_use]
pub fn live_head(at_branch: bool, held: &[HeldCommit], complete: bool) -> LiveHead {
    if at_branch {
        LiveHead::Branch
    } else if complete
        && !held.is_empty()
        && held
            .iter()
            .all(|commit| commit.merge && commit.follows_branch && commit.merges_base)
    {
        LiveHead::BaseUpdate
    } else {
        LiveHead::Diverged
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaseUpdate {
    Current,
    Merge,
    Restack,
}

#[must_use]
pub const fn base_update(behind: bool, stacked: bool, queued: bool) -> BaseUpdate {
    if !behind {
        BaseUpdate::Current
    } else if stacked {
        BaseUpdate::Restack
    } else if queued {
        BaseUpdate::Current
    } else {
        BaseUpdate::Merge
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Settlement {
    Waiting,
    Merged,
    Ejected,
    TimedOut,
    HeadChanged,
    Closed,
}

impl Settlement {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Merged => "merged",
            Self::Ejected => "ejected",
            Self::TimedOut => "timed_out",
            Self::HeadChanged => "head_changed",
            Self::Closed => "closed",
        }
    }
}

#[must_use]
pub const fn settlement(
    same_head: bool,
    lifecycle: Lifecycle,
    queue: Queue,
    expired: bool,
) -> Settlement {
    if !same_head {
        Settlement::HeadChanged
    } else if matches!(lifecycle, Lifecycle::Merged) {
        Settlement::Merged
    } else if matches!(lifecycle, Lifecycle::Closed) {
        Settlement::Closed
    } else if matches!(queue, Queue::Removed) {
        Settlement::Ejected
    } else if expired {
        Settlement::TimedOut
    } else {
        Settlement::Waiting
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Lifecycle {
    Open,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Queue {
    NotRequested,
    Present,
    Removed,
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// Only hunk headers and the blob names right before a text file's `--- ` line leave the compared change, so a context, added, removed, or binary line never does.
    pub fn spec_patch_line(line: &[u8], next: Option<&[u8]>) {
        let role = patch_line(line, next);
        let hunk = line.get(..3) == Some(&b"@@ "[..]);
        let blobs = line.get(..6) == Some(&b"index "[..])
            && next.is_some_and(|next| next.get(..4) == Some(&b"--- "[..]));
        assert_eq!(role == PatchLine::HunkStart, hunk);
        assert_eq!(role == PatchLine::TextBlobs, !hunk && blobs);
        assert_eq!(role == PatchLine::Change, !(hunk || blobs));
        if matches!(line.first(), Some(b'+' | b'-' | b' ' | b'\\')) {
            assert_eq!(role, PatchLine::Change);
        }
    }

    /// A live head other than the local branch passes only through a complete, nonempty walk of base update merges.
    pub fn spec_live_head(at_branch: bool, held: &[HeldCommit], complete: bool) {
        let result = live_head(at_branch, held, complete);
        let mut updates = complete && !held.is_empty();
        for commit in held {
            updates = updates && commit.merge && commit.follows_branch && commit.merges_base;
        }
        assert_eq!(result == LiveHead::Branch, at_branch);
        assert_eq!(result == LiveHead::BaseUpdate, !at_branch && updates);
        assert_eq!(result == LiveHead::Diverged, !at_branch && !updates);
    }

    fn spec_assessment(binding: bool, diff: bool, identity: bool) {
        let decision = assessment_change(binding, diff, identity);
        assert_eq!(decision == AssessmentChange::Reassess, !(diff && identity));
        assert_eq!(
            decision == AssessmentChange::Keep,
            binding && diff && identity
        );
        assert_eq!(
            decision == AssessmentChange::Resign,
            !binding && diff && identity
        );
    }

    fn spec_base(behind: bool, stacked: bool, queue: bool) {
        let update = base_update(behind, stacked, queue);
        assert_eq!(update == BaseUpdate::Restack, behind && stacked);
        assert_eq!(update == BaseUpdate::Merge, behind && !stacked && !queue);
        assert_eq!(update == BaseUpdate::Current, !behind || !stacked && queue);
    }

    fn spec_settlement(head: bool, lifecycle: Lifecycle, queue: Queue, expired: bool) {
        let result = settlement(head, lifecycle, queue, expired);
        let expected = match (
            head,
            lifecycle == Lifecycle::Merged,
            lifecycle == Lifecycle::Closed,
            queue == Queue::Removed,
            expired,
        ) {
            (false, _, _, _, _) => Settlement::HeadChanged,
            (true, true, _, _, _) => Settlement::Merged,
            (true, false, true, _, _) => Settlement::Closed,
            (true, false, false, true, _) => Settlement::Ejected,
            (true, false, false, false, true) => Settlement::TimedOut,
            (true, false, false, false, false) => Settlement::Waiting,
        };
        assert_eq!(result, expected);
        let key = match expected {
            Settlement::Waiting => "waiting",
            Settlement::Merged => "merged",
            Settlement::Ejected => "ejected",
            Settlement::TimedOut => "timed_out",
            Settlement::HeadChanged => "head_changed",
            Settlement::Closed => "closed",
        };
        assert_eq!(result.key(), key);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn assessment_carry_requires_the_same_diff_source_and_trusted_signer() {
        let [binding, diff, identity] = kani::any();
        spec_assessment(binding, diff, identity);
        let decision = assessment_change(binding, diff, identity);
        kani::cover!(decision == AssessmentChange::Resign);
        kani::cover!(decision == AssessmentChange::Reassess);
    }

    /// The role depends on the first six bytes of the line and the first four of the next line, so these lengths reach every role and every prefix boundary.
    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn only_hunk_headers_and_text_blob_names_leave_the_compared_change() {
        let line: [u8; 7] = kani::any();
        let next: [u8; 5] = kani::any();
        let length: usize = kani::any();
        let next_length: usize = kani::any();
        kani::assume(length <= line.len() && next_length <= next.len());
        let present: bool = kani::any();
        let next = present.then(|| &next[..next_length]);
        spec_patch_line(&line[..length], next);
        let role = patch_line(&line[..length], next);
        kani::cover!(role == PatchLine::HunkStart);
        kani::cover!(role == PatchLine::TextBlobs);
        kani::cover!(role == PatchLine::Change && length == line.len());
    }

    /// Generating the nine flags unrolls nine iterations, so the bound covers that loop and the three-commit walks after it.
    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn a_live_head_off_the_branch_needs_a_complete_walk_of_base_update_merges() {
        let flags: [bool; 9] = kani::any();
        let commits = [0, 3, 6].map(|start| HeldCommit {
            merge: flags[start],
            follows_branch: flags[start + 1],
            merges_base: flags[start + 2],
        });
        let length: usize = kani::any();
        kani::assume(length <= commits.len());
        let [at_branch, complete] = kani::any();
        spec_live_head(at_branch, &commits[..length], complete);
        let result = live_head(at_branch, &commits[..length], complete);
        kani::cover!(result == LiveHead::BaseUpdate && length == commits.len());
        kani::cover!(result == LiveHead::Diverged && complete && length == commits.len());
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_queue_owns_base_updates_but_stacks_still_restack() {
        let [behind, stacked, queued] = kani::any();
        spec_base(behind, stacked, queued);
        let update = base_update(behind, stacked, queued);
        kani::cover!(update == BaseUpdate::Merge);
        kani::cover!(update == BaseUpdate::Restack);
    }

    /// The longest settlement key, `head_changed`, has 12 bytes, so the bound covers every key comparison.
    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(13)]
    fn merge_settlement_never_credits_a_different_head_or_a_queue_acknowledgement() {
        let [head, expired] = kani::any();
        let lifecycle: Lifecycle = kani::any();
        let queue: Queue = kani::any();
        spec_settlement(head, lifecycle, queue, expired);
        let result = settlement(head, lifecycle, queue, expired);
        kani::cover!(result == Settlement::Merged);
        kani::cover!(result == Settlement::Ejected);
    }

    #[test]
    fn every_assessment_and_base_transition() {
        for head in [false, true] {
            for diff in [false, true] {
                for source in [false, true] {
                    spec_base(head, diff, source);
                    spec_assessment(head, diff, source);
                }
            }
        }
    }

    #[test]
    fn every_patch_line_meets_its_specification_and_examples_hold() {
        use alloc::vec::Vec;
        let stems: [&[u8]; 12] = [
            b"", b"@@", b"@@ ", b"@ @ ", b"index", b"index ", b"Index ", b"--- ", b"-- ", b"+",
            b" ", b"\\",
        ];
        let lines: Vec<Vec<u8>> = stems
            .iter()
            .flat_map(|stem| {
                crate::domain::words(b"@ -\n", 2)
                    .into_iter()
                    .map(move |tail| [*stem, tail.as_slice()].concat())
            })
            .collect();
        for line in &lines {
            spec_patch_line(line, None);
            for next in &lines {
                spec_patch_line(line, Some(next));
            }
        }
        for (line, next, role) in [
            (
                &b"@@ -12,7 +13,7 @@ fn main() {\n"[..],
                None,
                PatchLine::HunkStart,
            ),
            (
                b"index 1111..2222 100644\n",
                Some(&b"--- a/shared.txt\n"[..]),
                PatchLine::TextBlobs,
            ),
            (
                b"index 1111..2222 100644\n",
                Some(b"GIT binary patch\n"),
                PatchLine::Change,
            ),
            (b"index 0000..e69d\n", None, PatchLine::Change),
            (
                b"--- a/shared.txt\n",
                Some(b"+++ b/shared.txt\n"),
                PatchLine::Change,
            ),
            (
                b"-index removed\n",
                Some(b"--- removed\n"),
                PatchLine::Change,
            ),
            (b" @@ context\n", None, PatchLine::Change),
            (
                b"new mode 100755\n",
                Some(b"index 1111..2222\n"),
                PatchLine::Change,
            ),
        ] {
            assert_eq!(patch_line(line, next), role);
        }
    }

    #[test]
    fn every_live_head_up_to_three_held_commits() {
        for flags in crate::domain::flags::<11>() {
            let [at_branch, complete, rest @ ..] = flags;
            let commits = [0, 3, 6].map(|start| HeldCommit {
                merge: rest[start],
                follows_branch: rest[start + 1],
                merges_base: rest[start + 2],
            });
            for length in 0..=commits.len() {
                spec_live_head(at_branch, &commits[..length], complete);
            }
        }
    }

    #[test]
    fn every_merge_settlement() {
        for head in [false, true] {
            for lifecycle in [Lifecycle::Open, Lifecycle::Merged, Lifecycle::Closed] {
                for queue in [Queue::NotRequested, Queue::Present, Queue::Removed] {
                    for expired in [false, true] {
                        spec_settlement(head, lifecycle, queue, expired);
                    }
                }
            }
        }
    }
}
