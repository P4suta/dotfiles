---
date: 2026-10-15
status: amended
changed_by: [proof-verdicts-never-depend-on-time.md]
---
# Prove portable component scans with erased witnesses

## Context

Archive component validation must preserve its complete byte domain and reject Windows device aliases and destination redirects.
The earlier full scan unrolling supplied no completed destination or device proof.
Whole-prefix quantified invariants also exposed unsupported verifier paths.
The pinned CBMC contract library allocates object-indexed bookkeeping arrays, so its default object capacity adds substantial symbolic work even for short components.

## Decision

Keep the [production component validator](../../crates/rules/src/artifact_rules.rs) as the proof subject.
Express its ordinary scan with an explicit index while retaining the guards, byte order, state updates, and returned decision.
Rust removes the witness parameter, cached witness byte, and decreasing counter outside `cfg(kani)`.
Those ghost values never select an ordinary branch, alter an input, or determine the returned value.

Use an arbitrary byte position to state the scan invariant for one original byte.
The destination harness retains every arbitrary input byte and every original length from zero through 256.
It restricts the selected position only inside the accepted branch, after the existing global assertions.
The invariant establishes that an accepted result contains no forbidden byte at that position.
The decreasing counter and checked index establish termination and safe access independently.
Keep every device family, casing choice, extension byte, and original assertion, including device recognition before its rejection claim.

The [proof driver](../../xtask/src/skill_proofs.rs) selects twelve CBMC object bits explicitly.
The pinned CBMC implementation throws an analysis exception when object identities exceed that capacity.
It never supplies proof credit for that failure.
This setting changes verifier bookkeeping capacity without narrowing the component byte domain.
The driver and its setting contribute to the existing source-bound engine identity, so evidence from another configuration cannot supply the same cache identity.
Retain each original receipt's effective object capacity and solver with the result.

Require completed verification under the [verdict amendment](proof-verdicts-never-depend-on-time.md), retaining all invariant, unwinding, arithmetic, pointer, and assertion obligations.
Keep the [rejecting probes](../../policy/obligations.toml) tied to their original false assertions.
An invariant failure, capacity error, timeout, or another tool failure cannot replace an intended rejection.
Loop abstraction can overapproximate reachability, so its satisfied covers alone do not establish concrete traces.
The ordinary component tests retain concrete accepted and rejected executions alongside the full-domain safety proofs.

## Alternatives

Raising timeouts or dropping difficult bytes would change the required assurance rather than repair the proof.
A separate simplified validator would leave the deployed implementation unproved.
A quantified prefix invariant keeps a stronger summary but adds unsupported semantics and unnecessary solver work for the selected-byte property.
Changing only the SAT backend did not remove the measured bookkeeping overhead.

## Consequences

The ordinary public API keeps its original single argument.
The indexed scan changes the ordinary implementation shape, so its verification must identify that implementation rather than claim byte equality with the previous iterator loop.
Native tests and every affected production harness and rejecting probe remain required before completion.
Another subject may exceed the smaller object capacity and must then fail without credit until its verification plan addresses that requirement.
Elapsed measurements remain advisory; verifier and capacity errors still supply no proof evidence.
