---
status: accepted
date: 2026-11-02
---
# Carry assessments through base update merges that keep the hunks

## Context

The [stack-first decision](stack-first-pull-requests-through-a-merge-queue.md) carries a signed assessment to a new head when a restack or base update keeps the member's diff.
Personal repositories keep strict required checks, as [the queue decision](limit-merge-queues-to-organization-repositories.md) records, so `pr-workflow merge` updates a behind head through GitHub's update merge.
Two checks defeated that carry on real pull requests in P4suta/public-phonebook, as [issue 528](https://github.com/P4suta/dotfiles/issues/528) records.

The carry compared the SHA-256 of the raw `git diff --binary --full-index` bytes.
When the base edits another region of a file that the pull request also edits, the blob names and the hunk line numbers change.
Every hunk still keeps its context, added, and removed lines.
The live source check then compared the local branch with the new head, which only GitHub holds after its update merge.
Each refusal sent the operator to fast-forward the local branch, reassess with the unchanged assessment file, and merge again.

## Decision

A member keeps its diff when both diffs list the same lines after removing hunk headers and the blob names of each text file.
The blob names of a text file are its `index` line, which a `--- ` line follows.
Mode lines, file headers, binary payloads with their blob names, and every context, added, and removed line stay in the comparison, in order, with each hunk's boundary.
The [merge rules](../../crates/rules/src/pr_merge_rules.rs) classify each line, and the [review policy adapter](../../crates/pr-workflow/src/review_policy.rs) digests the lines that the rule keeps.
The carried statement still binds the exact new head, tree, base, raw diff SHA-256, source, and signer.
The merge still names that head with `--match-head-commit`, so only the importance judgment carries.

The live PR head passes the source check when the local branch of the same name points at it.
It also passes when every commit it holds beyond the local branch and its base is a merge of the base onto the local branch.
Such a merge has the local branch or another such merge as its first parent, and the base contains its second parent.
That is the shape of GitHub's update merge, so the check accepts a head that pr-workflow itself asked GitHub to update.
Any other commit, a walk longer than 64 commits, or a failed Git query refuses as before.

## Consequences

`merge` and `stack merge` carry an assessment through base updates that leave the change's lines intact.
The operator no longer fast-forwards and reassesses after each update.
A base edit inside a hunk's three context lines changes the compared lines and still needs a new assessment.
Base edits outside the hunks can interact with the change, as edits to other files already can, and the carried judgment accepts that interaction.
Git's diff format and its ancestry answers remain external assumptions, and the Kani harnesses check the line classification and the source decision under them.

## Alternatives

Comparing raw diff bytes refuses every base update that touches a changed file elsewhere.
`git patch-id` also ignores line numbers, but it ignores whitespace by default and would move the definition of a kept diff into Git's options.
Fast-forwarding the local branch from pr-workflow would move the operator's checkout and conflict with uncommitted work.
Reassessing automatically with the last assessment file would re-sign a judgment for a diff whose hunks changed.
