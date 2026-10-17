---
date: 2026-10-12
status: proposed
---
# Persist remote execution ownership before submission

## Context

A remote gate outlives its collector, so a failed acknowledgement, status request, or log request must preserve the gate's identity and exact source.
Completed stage receipts alone can't identify an execution interrupted before its first receipt.

## Decision

The existing host check stores a typed pending execution and its frozen Git bundle in the common Git directory before submitting with domyjob's existing submission identity.
The record binds the host, family, commit, tree, gate, stage inputs, reused origins, bundle digest, and exact submission program.
A submitted execution resumes by its existing identity.
An unacknowledged submission repeats the same identity, snapshot, and program, relying on domyjob's rejection of conflicting work under that identity.
An installed tool refresh cannot change that replayed request.

A family lock spans recovery, execution, and recording across linked worktrees.
The check validates the retained source before submission and recovery, and preserves pending state when transport, state, log, or packet validation fails.
Independent families collect their results even when another family's collection fails.
Recovery uses the retained host before checking capacity for additional workspaces.
Admission refuses pending executions.
The existing receipt writer records genuine terminal stage results, including failures, before retiring the pending record.
Pending state never supplies a stage result or an execution generation.

A recognized immutable terminal outcome without admissible stage evidence retains a source-bound unverified history entry before changing pending state.
Malformed packets and unsupported status responses retain the same accepted identity.
The history binds the terminal output digest and the exact Notes revision that establishes its execution ordering cutoff.
Earlier actual receipts and their reused copies cannot satisfy either affected stage after that cutoff.
The barrier matches family and stage inputs, and includes the gate for Code.
A passing candidate with different inputs cannot clear another candidate's barrier.

The failed check does not retry an unverified terminal attempt.
The next normal check can prepare a new attempt using a checked, atomically persisted family sequence, which distinguishes attempts even when Notes have not changed.
The obsolete bundle of an unverified attempt is no longer needed for replay; its matching validated terminal history remains mandatory before restart.
Fresh, verified actual receipts after the cutoff restore credit only for their matching inputs.
Recorded current results can retire the obsolete pending record while preserving terminal history.
Reused receipts retain their original actual execution generations.

## Alternatives

A provisional stage note would invent an unmeasured result.
Printing an identity leaves no recovery contract after the collector exits.
A separate import command would require operators to reconstruct source and ordering facts that the collector already knows.

## Consequences

The common Git directory retains one bundle per pending family, a submission sequence, and unverified terminal history.
Interruption can keep that storage until normal recovery completes.
Atomic replacement and synchronized file contents cover collector termination under the native filesystem's semantics; they make no power-loss guarantee.
SSH authentication, host reachability, and domyjob's persistent identity protocol remain external assumptions.
Git source objects and the append-only Notes ancestry must remain available for validation.
The [pending decisions](../../crates/rules/src/hosts_rules.rs), rejecting probes in [the obligation inventory](../../policy/obligations.toml), and [remote interruption regression](../../crates/dotfiles/tests/hosts.rs) cover the supported boundary.
