---
date: 2026-10-29
status: proposed
---
# Compare agent stops with request snapshots

## Status

Proposed.
Amends [assurance follows change provenance](assurance-follows-change-provenance.md).

## Context

The Stop gate must attribute changed files to the current request.
Comparing the entire working tree with the current commit includes work that predates that request.
A conversation can then block on existing uncommitted code, including code in a repository without a first commit.

## Decision

The existing managed `answer begin` hook captures the request directory's repository state.
Store its canonical root and an unreferenced Git comparison commit in the request record.
Use a temporary copy of the real index and stage the working tree into that copy.
Include staged files, tracked ignored files, and untracked files that Git doesn't ignore.
Preserve the real index, branch references, and working files.
Invalidate earlier answer evidence before attempting capture, so a failed capture can't preserve an earlier request's authority.

At Stop, compare the request snapshot with a fresh working tree snapshot from the same repository.
Use the existing gap, ownership, rename, merge-parent, and changed-file decisions.
Existing gaps in unchanged files don't block the request.
New gaps and gaps in files changed during the request still block it.
Keep reply checks active even when the snapshots match.
Keep commit, push, and readiness comparisons unchanged.

A missing legacy snapshot, missing request record, absent request directory, or different repository retains the existing comparison with the current commit.
Malformed records and missing snapshot objects fail the check.
Never reconstruct an earlier baseline from the current files.

## Alternatives

Skipping every Stop code check would admit defects introduced during a request.
Comparing only with the current commit misattributes existing uncommitted work.
Recording only tracked files loses changes to new source files.
Using the real index would change the owner's staging decisions.

## Consequences

Request capture creates Git objects that normal garbage collection can remove.
A collected baseline requires a fresh managed request rather than an exemption.
Concurrent repository edits remain changes to the request's shared working tree.
Separate working trees remain the isolation boundary for independent work.

Git object creation, clean filters, and filesystem state remain external semantic boundaries.
The [native integration tests](../../crates/dotfiles/tests/assure.rs) exercise the production request and Stop commands with real repositories.
The existing pure verdict and gap decisions keep their Kani harnesses and rejecting probes.
These tests don't prove Git's implementation or replace the release proof inventory.
