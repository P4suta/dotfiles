---
name: formal-assurance
description: >-
  Require proof-first, source-bound assurance for implementation changes in any language.
  Prefer a verifier that checks the actual language implementation, and use Lean 4 for general theorems with an explicit implementation refinement.
  Use for implementation, bug fixes, proof gaps, and verification gates.
---

# Proof-first Assurance

Every implementation change requires an explicit assurance assessment and completed evidence for its affected contract.
Machine-checked proofs are the default requirement for deterministic properties within supported verifier semantics, including automation and failure handling.
Require them for bounds, arithmetic, representation invariants, pure decisions and state transitions whenever a suitable verifier can establish those properties.
Scale the obligations to the changed behavior and the dependencies needed to establish it; changing one adapter does not create an obligation to prove an entire legacy application or external service.
Documentation-only edits require factual validation rather than an artificial theorem.
Do not declare implementation complete while an assessed requirement or its connection to production remains missing.
Read [scoping and alternatives](references/scoping.md) when a property crosses verifier semantics, expresses an empirical claim, or repeated proof attempts do not establish the intended guarantee.
An alternative requires a concrete reason, preserved proof obligations for the supported core, current substitute evidence, explicit limits and a reconsideration condition.
Difficulty, an unavailable installation, a timeout or a desire to finish is not by itself an exception.

Before implementing, state the observable contract, input domain, invalid-input behavior, transitions, and properties that must hold.
Include preservation of existing behavior, overflow and bounds, error propagation, cancellation, cleanup, and concurrency where they affect that contract.
Trace each required property to the production entry point, the harness or theorem, its assumptions, and an actual verification result.
Distinguish a property proved for all admitted inputs from a finite bound or a theorem about an abstract model.
Do not define the contract after seeing what the verifier happens to accept.
Use types and validated transitions to eliminate invalid states, and isolate pure decision cores from external effects when that improves provability.
For Rust, use `rust-invariants` for the production boundary.
Use `systematic-fixes` when a counterexample reveals a shared cause.

Choose complementary methods for the actual property.
Prefer a maintained language-native verifier or proof-producing implementation tool when its supported semantics cover the production code and required property.
Select tools by the guarantee they establish, including integer semantics, termination, concurrency, and external effects, rather than familiarity or language name alone.
Rust commonly uses Kani; another suitable Rust verifier is acceptable when it preserves the required contract.
Lean 4 is suitable for language-independent mathematics, induction, algorithms, and refinement arguments.
Kani checks properties of executable Rust under its supported semantics and harness assumptions.
Lean 4 checks a stated theorem about a formal model; connect that model to the implementation explicitly.
Use official documentation to establish the chosen verifier's supported features and pin its version and dependencies.
Do not require both Kani and Lean when one establishes the complete required property.

Run the required proofs against the actual changed implementation or a justified semantics-preserving translation.
Prefer importing production functions to copying an algorithm into a harness.
A detached model, successful typecheck, test suite, differential comparison, or coverage percentage cannot replace an applicable required proof.
Make external trust assumptions explicit and prove the adapter's own behavior under those assumptions; a driver, operating system, or remote API is not thereby proved.
First close gaps by repairing the implementation, strengthening the model connection, adding invariants, decomposing the proof, or choosing a suitable verifier.
Do not assume the desired conclusion, remove difficult cases, weaken a promised guarantee, or replace the implementation with a stub to make verification pass.
If a required proof remains feasible but incomplete, preserve that status, report the exact obligation, and continue independent authorized work.
If the assessment establishes a real semantic or empirical boundary, enforce its justified alternative through the same required check; do not relabel an unsuccessful proof as verified.

Read [verifier-specific checks](references/verifiers.md) for the selected Kani, Lean or independent-reference workflow.
Keep assumptions, bounds and abstractions explicit, require reachable contracts, and treat timeouts, skipped harnesses and solver failures as missing evidence.

Keep proofs, harnesses, and reference code versioned with the contract and fail required checks when they drift or disappear.
Bind results to the exact source, assumptions, verifier configuration, and target semantics; stale receipts do not satisfy the gate.
Isolate generated proof models and compiler outputs for the exact checkout and verification run; a shared build cache must not make different checkouts or negative cases reuse stale proof models.
Make the assessed requirements part of the authoritative project check and CI, with expected proof names, checked alternatives and completeness checks rather than success inferred from an empty run.
Keep a stronger whole-system proof claim separate from completion of the scoped implementation contract; neither may silently stand in for the other.
Use a deliberately false claim or representative broken implementation to verify that the gate rejects a counterexample; require the expected failure rather than accepting any tool error.
Retain integration tests and real boundary checks for guarantees outside the verifier's semantics.
Statistical claims still require appropriate null comparisons, and a heuristic's accuracy or runtime superiority needs its own evidence.

Resolve the assessed requirements and investigate counterexamples before a large search, benchmark, or production rollout.
Do not escalate computation to compensate for an unverified correctness argument.
This ordering does not authorize stopping existing jobs, consuming cloud credit, or changing resource limits without the user's task context.
Use `resource-coordination` before a proof run or reference campaign likely to occupy a machine for hours.
Use maintained Rust tooling to orchestrate them; a verification reference in another language does not authorize shell or Python glue.
Record consequential model and trust-boundary choices in an ADR and resolve gaps in the authorized work before claiming completion.
