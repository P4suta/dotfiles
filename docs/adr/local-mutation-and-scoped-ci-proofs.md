---
date: 2026-10-13
status: accepted
---
# Keep mutation testing local while CI verifies its declared proof campaign

## Context

The hosted Linux proof campaign reached its 60-minute limit during mutation testing.
Kani, Miri, and all fuzz corpus replays completed, but the remaining measurements and models lacked hosted results.
The CI proof campaign defers mutation testing pending the njutest project.
The Mac candidate gate already runs the complete mutation inventory before publication.

## Decision

This amends the hosted campaign in [fixed processor budgets beside Mac proof campaigns](fixed-cpu-budgets-beside-mac-proof-campaigns.md).
`just proofs` and the Mac candidate gate keep production mutation testing, its assertion-free rejecting fixture, and their existing budgets.
The required Linux job calls `just proofs-ci`.
That command runs Kani, Miri, every fuzz corpus replay and rejecting fixture, fixed Linux measurements, Quint, Alloy, and Lean.
Its declared scope excludes production mutation testing and its mutation fixture.
It never reports a mutation result.
The CI job excludes cargo-mutants during this pause.

Kani retains its serial execution, complete inventory, rejecting probes, target semantics, and per-harness limits.
Native candidate admission still requires the normal source-bound Code and Real results on every host.
The hosted command grants no local candidate evidence or exception to those gates.

## Alternatives

Increasing the runner timeout preserves the serial cost and delays the remaining checks.
Removing mutation testing from the local candidate gate would remove the test-strength evidence this decision retains.

## Consequences

Hosted runs can finish the declared campaign without repeating the local mutation workload.
Local mutation remains mandatory before a normal push.
Revisit hosted mutation when njutest can verify the complete production inventory, rejecting fixture, failure outcomes, and target contract through a maintained gate.
