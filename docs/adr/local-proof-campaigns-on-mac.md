---
date: 2026-10-12
status: amended
changed_by: [fixed-cpu-budgets-beside-mac-proof-campaigns.md, native-verification-with-lightweight-ci.md, proof-verdicts-never-depend-on-time.md]
---
# Run local proof campaigns on Mac and keep Linux CI

## Context

The complete candidate needs extensive pure-core verification and independent native checks on every supported family.
Allocating the local proof campaign to the faster available host reduces waiting, while target semantics and deterministic bounds remain part of its contract.
Elapsed measurements are advisory under the [verdict amendment](proof-verdicts-never-depend-on-time.md).

## Decision

This amends the local proof allocation in [source-bound push validation](source-bound-push-validation.md).
The Mac code stage runs the complete existing proof suite through `just push-check`.
Linux and Windows keep every native code and real-host check, and the required hosted proof job remains on Linux.
Each receipt stays bound to its actual family, stage, gate, and source inputs.
Moving the allocation invalidates affected inputs and requires new native results.
A Mac result never replaces a failed Linux receipt.

Kani verifies the pure rule modules on the native Mac target with the pinned verifier, inventories, bounds, assertions, witnesses, and rejecting probes.
The current modules have no target-dependent configuration, native calls, or native-endian byte operations, and the supported Mac and Linux targets both use 64-bit words.
This supports the pure-core allocation.
Native adapters still need their own platform checks, and Linux CI runs Kani for its own target.
Miri on Mac interprets the host, Linux x86_64, and Windows x86_64 tests, preserving Linux interpretation rather than treating the host target as interchangeable.
Fuzz corpus replay and mutation testing run natively on Mac with their existing failure controls and budgets.

The proof and Lean images contain pinned Linux x86_64 binaries, so their builds and runs explicitly select `linux/amd64`.
The gate reuses an existing image only when Docker reports that operating system and architecture.
These images use Linux x86_64 binaries, but glibc still selects string operations from CPU features.
Portable Quint and Alloy images keep their existing native Docker platform and offline model checks.

Mac code and native provisioning run serially to keep their resource use separate.
Linux keeps its existing serial stages, and Windows keeps its existing capacity requirements for concurrent stages.
The [host rule](../../crates/rules/src/hosts_rules.rs), its rejecting probes in [the proof inventory](../../policy/obligations.toml), and [Docker boundary tests](../../xtask/tests/container_proofs.rs) enforce those conditions.

## Alternatives

An independent Mac proof run prepares tools but can't replace an existing Linux code receipt.
Changing only the execution host would omit Linux Miri interpretation and could overlap proofs with Mac provisioning.
Interpreting x86_64 binaries or Valgrind baselines as native Arm measurements would change the measured contract.

## Consequences

The first candidate after this change needs complete new stage results.
Later candidates keep the existing source-bound reuse.
Cold image builds and x86_64 emulation still cost time on Mac, so the improvement requires measurement.
Missing tools, unsupported targets, failed proofs, and absent native evidence continue to refuse the candidate.
