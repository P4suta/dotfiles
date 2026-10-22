---
date: 2026-10-22
status: proposed
---
# Assurance follows change provenance

## Status

Proposed.
Amends [verification in proportion to code shape](verification-proportional-to-code-shape.md).

## Context

A merge can import gaps that already exist in another parent.
Comparing its index only with `HEAD` attributes those gaps to the merge resolution.
A push to an existing branch can make the same mistake when it imports commits from the upstream default branch.
Readiness also requires every PR to close an existing gap, which forces unrelated repairs into dependency and documentation changes.

## Decision

Keep the session's complete gap report and the refusal of newly introduced gaps and weakened verification budgets.
Remove the requirement to close an unrelated existing gap before making a PR ready.
The stop hook keeps its rule for files edited during the turn.

For a pending merge, include every validated `MERGE_HEAD` parent in the comparison's known gap set.
At stop, importing a file identical to a merge parent doesn't count as editing that file.
An edit that differs from all parents still counts.
Resolve Git's per-worktree merge state through `git rev-parse --git-path` and fail on malformed or missing parent objects.

When a push to an existing branch introduces a merge, also survey the merge base with the upstream default commit advertised by Git.
This accounts for shared upstream history without trusting remote-tracking refs or treating unpushed local parents as accepted history.
Record the inherited commit identities beside the catalog, owner-map, and primary comparison identities.
No merge, dependency update, or documentation change receives a blanket exemption.

Verify an exact revert through Git's three-way merge with the reverted commit as the base.
Construct two unreferenced comparison objects with that shared parent and the existing trees.
This preserves Git's rename handling on versions that lack `merge-tree --merge-base`.
The objects have fixed metadata, change no refs or index, and never enter published history.
Distinguish a merge conflict from a Git execution error rather than treating either as an ordinary negative result.

## Alternatives

Skipping merge checks would admit violations introduced by conflict resolution.
Trusting all parents during push would admit violations created on unpushed local branches.
Forcing an arbitrary existing repair into every PR expands its scope without verifying the changed behavior.

## Consequences

Unchanged debt remains visible without blocking unrelated maintenance.
New gaps and weakened budgets still refuse readiness.
The adapter trusts Git's object resolution and advertised upstream default, with real Git integration tests covering that boundary.
The production verdict and gap-set predicates remain bound to their Kani harnesses and rejecting probes.
