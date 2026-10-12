---
date: 2026-10-12
status: accepted
---
# Keep fixed processor budgets beside Mac proof campaigns

## Context

A fixed image and architecture don't fix the CPU features that glibc uses to select string operations.
The same Rust compiler, libc, Valgrind, and Gungraun binaries produced different instruction counts on native x86_64 Linux and an Arm Mac's x86_64 translation.
The strip benchmark's 37-instruction difference came entirely from memcpy: 174 instructions on Mac and 137 on Linux.
The classify benchmark's 136-instruction difference came entirely from memcmp: 478 instructions on Mac and 342 on Linux.
Linux selected vector implementations that Mac's CPU feature model didn't advertise.
Changing those ceilings or forcing instructions that the host doesn't support would change the measured contract.

## Decision

This amends the measurement allocation in [local proof campaigns on Mac](local-proof-campaigns-on-mac.md).
Mac's `just proofs` runs Kani, Miri, fuzz corpus replay, mutation testing, and the Quint, Alloy, and Lean models.
It reports that fixed CPU measurements require the separate mandatory Linux code-stage result.
Linux's `just proofs` keeps its complete existing campaign and measurement checks, including the required hosted CI run.
Windows keeps its existing Linux-container proof path.

The canonical code gate selects the extra verification through the [typed family decision](../../crates/rules/src/hosts_rules.rs).
Mac runs the proof campaign, Linux runs the existing fixed instruction, heap, and binary budgets, and Windows runs its native checks.
Every family runs its native checks, even when an extra check fails, and the gate collects every selected failure.
The [dispatch regression](../../xtask/tests/benchmarks.rs) covers every family, success, independent failures, and combined failures.
The rule's Kani harness and rejecting probes refuse an allocation that drops Linux measurements or the Mac campaign.

Fleet admission still requires successful Code and Real receipts on every required family.
Each result keeps its exact source inputs, command, family, and execution generation.
A Mac proof result never authorizes an omitted or failed Linux measurement.
The existing benchmark command, measurements, ceilings, planted regression, and required hosted Linux proof check stay unchanged.

## Consequences

Mac can run the expensive campaign through the ordinary proof command without misreporting complete fleet assurance.
The fixed CPU measurement remains a native x86_64 Linux responsibility.
A portable standalone measurement command needs a separate decision about reproducible CPU dispatch or independently justified target measurements.
