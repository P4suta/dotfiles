---
date: 2026-10-13
status: proposed
---
# Run Renovate through a process from the same source build

## Context

The Git wrapper starts for every Git command, while dependency lookups run only when requested.
The complete report validator and immutable manifest comparison enlarged the wrapper beyond its fixed binary ceiling.
The existing command-line interface, repository context, streams, refusal behavior, and validation requirements must remain available.

## Decision

Keep the public `dotguard renovate` and `dotguard renovate-gate` commands.
Delegate them to `dotguard-renovate`, installed beside the wrapper from the same Cargo package.
Resolve the adjacent executable directly and pass the compiled source hash in the same invocation.
The backend compares that hash with its own compiled hash before entering the existing validator.
Missing or mismatched binaries refuse execution.
The process constructor preserves the invoking repository context, and ordinary children of the validator retain their existing isolation.
Unix replaces the frontend process, while Windows relays the child's status and inherited streams.

Install the pair together with the existing build profile and failed-install restoration.
Native candidate provisioning builds and places both binaries from the same candidate.
The hash comparison binds the pair to the same source inputs.
It does not claim that the guard performs the workflow tools' checkout freshness scan.

## Alternatives

Keeping the validator in the wrapper preserves behavior but exceeds the fixed binary ceiling.
Loading a separately managed runtime script adds another deployment and integrity boundary.
A preliminary version probe permits the backend to change between verification and execution.

## Consequences

Ordinary Git commands load a smaller executable.
Dependency lookups retain the complete compiled validator and source comparison behind their existing interface.
Installation must preserve the adjacent pair, and an incomplete update fails closed.
The [CLI regressions](../../crates/dotguard/tests/push_and_signing.rs) exercise normal delegation and rejected missing or mismatched binaries.
The [native provisioning](../../crates/dotfiles/src/host_checks.rs) places the pair in the candidate's owned home.
The [fixed benchmarks](../../xtask/src/benchmarks.rs) remain the measured admission gate.
