---
date: 2026-10-25
status: proposed
---
# Compose native campaign arithmetic with Lean ceiling proofs

## Status

Proposed.

## Context

The campaign budget must reserve time for the least number of complete waves.
Its executable arithmetic must also reject invalid inputs and avoid integer overflow.
Those properties share a contract but need different representations.
Repeated native proofs of the combined division and multiplication inequalities remained inconclusive under the per-harness limit.
An incomplete solver result supplies no exception or verification credit.

## Decision

Keep the full `u32` harness count, `u16` parallelism, and `u32` overhead domains.
Reject zero work or parallelism before division.
Compute `quotient` and `remainder` in the input domain, widen before adding the partial-wave increment, then multiply and add overhead in `u64`.
Check the final conversion to `u32`.
The native Kani harness checks the production arithmetic helper for every `u32` quotient, partial-wave Boolean, and `u32` overhead.
The helper domain includes every possible result of the wrapper's division and permits more combinations.
Separate native checks verify every zero-work and zero-parallelism input.

Use Lean natural numbers for the wave inequalities and compose them with the native postcondition.
The `/` and `%` interpretation follows the [Rust unsigned integer operations](https://doc.rust-lang.org/std/primitive.u32.html).
The Lean model keeps the complete positive natural-number domain rather than selecting a smaller integer bound.
Separate zero and nonzero remainders to prove coverage and minimality, then relate the actual budget after removing overhead to those wave counts.

Extend the existing [elapsed refinement](../../xtask/src/elapsed_refinement.rs) instead of introducing another translation mechanism.
Require the exact production wrapper, arithmetic helper, native postconditions, arbitrary input types, imports, constants, and harness attributes before generating the Lean definitions.
Reject altered arithmetic, added assumptions, substituted calls, shadowed names, stale generated claims, and missing or duplicate obligations.
Keep every native Kani contract and kernel-checked Lean theorem mandatory in the complete proof campaign.
Check the theorem axioms through the existing gate.

## Consequences

The composed argument covers the original arithmetic and wave properties without extending execution limits or dropping inputs.
The implementation-to-model boundary depends on Rust's unsigned division semantics, the checked translation, and the pinned verifier toolchains.
Native boundary regressions and rejecting translation mutations remain necessary beside the mathematical theorems.

A detached mathematical model would leave the production refinement unverified.
Increasing timeouts would change resource occupancy without establishing the missing argument.
Reducing integer domains would discard inputs the production interface admits.
