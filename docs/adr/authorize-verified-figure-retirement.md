---
status: accepted
date: 2026-11-07
---
# Authorize verified figure retirement

## Context

Migration preserves history and documents before retirement.
Git's [lease with an explicit expected object ID](https://git-scm.com/docs/git-push) prevents deleting a branch that moved.
The general force and deletion guards reject that operation.

## Decision

Issue a temporary repository-local permit after preservation and reference verification.
Bind a random token, remote, figure branch, and archived object ID to one exact leased deletion.
The Git wrapper and push gates require the matching permit and request.
Other deletions and rewrites remain refused.
Remove the permit when the operation ends.
Deploy the guard, matching policy companion, and workflow through the scoped native installer.

## Alternatives

Forge deletion lacks the documented expected-object lease.
A force waiver authorizes unrelated operations.
A stack lease has a different purpose.

## Consequences

Interrupted operations can leave a permit requiring inspection before recovery.
The permit is a cooperative owner-local boundary.
It does not isolate processes with the owner's filesystem access.

## Confirmation

Exercise the actual Git wrapper and pre-push gate.
Probe installed dispatch with no offered updates.
Reject another token, remote, ref, object, or operation and mixed updates.
Verify that a concurrent remote advance defeats deletion and that the archived ref remains recoverable.
