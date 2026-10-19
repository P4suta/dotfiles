---
status: accepted
date: 2026-10-09
---
# Optimize checked MIR for the pinned Kani verifier

## Context

The bounded implementation refinements combine several functions of the pure decision core.
The unoptimized MIR retains enough temporary storage and redundant checks to exceed the 30-second harness budget.
The same five-node planner refinement finishes in 20.85 seconds at MIR optimization level three, with overflow checks and debug assertions enabled.

## Decision

The standalone Kani command appends `--edition=2024 -C overflow-checks=yes -C debug-assertions=yes -Z mir-opt-level=3` to its compiler flags.
The pinned nightly compiler applies its semantics-preserving MIR transformations before the pinned Kani and CBMC verifiers check the result.
Overflow checks, debug assertions, the default solver, memory encoding, required covers, rejecting probes, and the proof budget remain required.
Both inventory discovery and verification use the same command constructor.
Function-contract runs disable only the MIR inlining pass, because Kani 0.68 otherwise loses the generated contract closure during code generation.
The existing reaper contracts verify with this pass selection and all original checks.

## Validation

Every harness and rejecting probe runs through the normal proof gate with these flags.
The gate requires a reachable successful assertion, every verification property, every cover witness, and the existing 30-second maximum.

## Reconsider

Reassess the setting when the pinned compiler or Kani version changes.
