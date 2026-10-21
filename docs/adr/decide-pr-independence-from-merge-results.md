---
date: 2026-10-22
status: proposed
---
# Decide PR independence from merge results

## Status

Proposed under [issue 308](https://github.com/P4suta/dotfiles/issues/308).

## Context

Two branches can change the same file without conflicting.
A caller's declaration of independence can also admit changes that conflict when either PR merges first.
PR creation needs evidence from the branch contents before it permits independent work.

## Decision

Keep shared commits as a stack dependency.
For each open PR with overlapping paths, the [Git adapter](../../crates/pr-workflow/src/lib.rs) runs `git merge-tree --write-tree` on both heads.
Git finds their merge bases and checks the complete trees without changing the index or worktree, as its [merge-tree contract](https://git-scm.com/docs/git-merge-tree) specifies.
Exit status zero means a clean merge, one means a conflict, and any other outcome stops creation as an execution failure.

The [pure rule](../../crates/rules/src/pr_rules.rs) permits disjoint paths and clean merges when no commits are shared.
A conflict refuses creation and names the conflicting PR and `pr-workflow stack create`.
Remove the caller override.

Path overlap alone rejects compatible edits.
A caller override cannot establish mergeability.
Merging in the checkout would disturb local work and require rollback.

## Consequences

Each overlapping PR adds one local Git merge check before publication.
Git's merge implementation and repository objects remain external trust assumptions.
The [integration tests](../../crates/pr-workflow/tests/pr_workflow.rs) exercise real Git merges, execution failures, multiple open heads, and preservation of local changes and the index.
The rule's exhaustive tests, Kani harness, and rejecting probes check the decision over its complete input domain.
Later head updates can change mergeability, so this check establishes independence only for the inspected heads.
