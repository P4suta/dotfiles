---
name: formal-assurance
description: >-
  Require proof-first, source-bound assurance for implementation changes in any language.
  Prefer a verifier that checks the actual language implementation, and use Lean 4 for general theorems with an explicit implementation refinement.
  Use for implementation, bug fixes, proof gaps, and verification gates.
---

# Proof-first assurance

Assess every implementation change and complete the evidence for its affected contract.
Prove deterministic properties with a machine-checked proof wherever a suitable verifier covers them: bounds, arithmetic, representation invariants, pure decisions, state transitions, automation, and failure handling.
Scale the obligations to the changed behavior and the dependencies it needs.
Documentation edits need factual checks, never an artificial theorem.
Never declare an implementation complete while an assessed obligation or its link to production lacks evidence.

Read [scoping and alternatives](references/scoping.md) when a property crosses verifier semantics, makes an empirical claim, or resists repeated proof attempts.
An alternative needs a concrete reason, proofs for the supported core, current substitute evidence, explicit limits, and a reconsideration condition.
Difficulty, a missing installation, a timeout, or a wish to finish never justifies an exception.

Before implementing, state the observable contract, input domain, invalid-input behavior, transitions, and required properties.
Trace each property to the production entry point, the harness or theorem, its assumptions, and an actual verification result.
Distinguish a proof for all admitted inputs from a finite bound or a theorem about an abstract model.
Use types and validated transitions to rule out invalid states, and separate pure decision cores from external effects.
For Rust, use `rust-invariants` for the production boundary.
Use `systematic-fixes` when a counterexample reveals a shared cause.

Prefer a maintained language-native verifier whose semantics cover the production code and the property.
Kani checks executable Rust under its supported semantics and harness assumptions.
Lean 4 checks a theorem about a formal model, so connect that model to the implementation explicitly.
Pin the verifier's version and dependencies.
Skip a second verifier when one establishes the complete property.

Run proofs on the changed implementation or a justified semantics-preserving translation, and import production functions into harnesses.
A detached model, typecheck, test suite, differential comparison, or coverage percentage never replaces a required proof.
State external trust assumptions and prove the adapter's own behavior under them.
Close gaps by repairing the implementation, strengthening the model link, adding invariants, decomposing the proof, or switching verifiers.
Never assume the conclusion, drop hard cases, weaken a promised property, or stub the implementation to pass.
Report an unfinished but achievable proof as unfinished and name the exact obligation.
Never relabel a failed proof as verified.

Read [verifier-specific checks](references/verifiers.md) for the Kani, Lean, or independent-reference workflow.
Treat timeouts, skipped harnesses, and solver failures as missing evidence.

Bind results to the exact source, assumptions, verifier configuration, and target semantics, and reject stale receipts.
Isolate generated proof models and compiler outputs per checkout and run.
Add the assessed requirements to the authoritative project check and CI, with expected proof names, checked alternatives, and completeness checks.
Prove that the gate rejects a false claim or a broken implementation with the expected failure, not any tool error.
Keep integration tests and real boundary checks for properties outside the verifier's semantics.
Statistical claims need null comparisons.

Never add computation to compensate for an unverified correctness argument.
Use `resource-coordination` before a proof run or reference campaign likely to occupy a machine for hours.
Orchestrate them with maintained Rust tooling instead of shell or Python glue.
Record consequential model and trust-boundary choices in an ADR.
