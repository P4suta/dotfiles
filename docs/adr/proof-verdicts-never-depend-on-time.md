---
status: accepted
date: 2026-10-20
---
# Proof verdicts never depend on time

## Status

Accepted.
This record amends [bounded harnesses](bounded-harness-over-kani-loop-contracts.md), [checked MIR](optimize-checked-kani-mir.md), [review postconditions](direct-bounded-review-postconditions.md), [finite parser evaluation](finite-shell-parser-ctfe.md), [the obligation manifest](checked-obligation-manifest.md), and [source covers](preserve-kani-source-covers.md).
It also amends [Miri interpretation](retain-miri-sysroots-with-fresh-interpretation.md), [Git argument composition](compose-bounded-git-argument-proofs.md), [portable component scans](prove-portable-component-scans-with-erased-witnesses.md), [skill plan inputs](validated-skill-plan-inputs.md), [push validation](source-bound-push-validation.md), [Mac campaigns](local-proof-campaigns-on-mac.md), and [state graph replay](state-graph-replay-of-finite-quint-models.md).
Their implementation bindings, admitted domains, assertions, and witnesses remain required.

## Context

A proof verdict must be reproducible on every machine with the pinned tools.
Elapsed time changes with host load and machine speed, so it cannot establish or refute a property.
The Kani and Miri gates previously rejected successful verification results above an elapsed threshold.
Several string functions also cited elapsed time to justify exemptions.

## Decision

Kani's completed verification result decides the verdict over each harness's explicit input domain and unwind bound.
Every asserted property and required cover must retain its checked result, including unwinding assertions.
A counterexample must fail its intended assertion.
The `--harness-timeout` option remains only a hang cap.
Reaching it reports "proof did not complete": missing evidence with an inconclusive verdict, never a failed proof, a verified result, or an exemption.
An incomplete invocation cannot publish a successful proof receipt.

Miri checks the complete selected test inventory and its completed outcomes.
Test counts and named selections bound each invocation's work.
Neither host elapsed time nor interpreted duration decides whether the tests passed.
Elapsed measurements remain advisory output.

Former time-based exemptions require bounded harnesses calling the production functions, explicit byte limits, successful unwinding assertions, rejecting probes, and reachable witnesses.
Their claims state "proved for inputs up to L bytes" and describe any narrower content or argument domain.
They make no claim beyond those bounds.
An exemption that remains must cite a concrete bound or unsupported semantic boundary, current substitute evidence, and a reconsider condition independent of a rejected pilot.

Windows paths and variable markers are proved for inputs up to 4 bytes, and template token classification is proved for inputs up to 11 bytes, over every valid UTF-8 input within each bound.
Their unwind bounds are respectively 5, 5, and 12.
Program normalization, editor arguments, awk statements, and provenance scanning retain explicit bounded alternatives beside their production functions.
Their native reference comparisons and rejecting probes remain required; an incomplete experiment supplies no proof credit.

No solver cost ceiling is introduced.
If one becomes necessary, a separate decision must pin a deterministic reported quantity, such as verification-condition counts or SAT variables and clauses, to the toolchain and input domain.
An elapsed measurement cannot supply that ceiling.

## Alternatives

Increasing the elapsed threshold still makes a verdict depend on the host.
Treating a timeout as a counterexample invents evidence the verifier did not produce.
Treating it as an exemption hides an unfinished obligation.
Dropping asserted properties, reachable witnesses, or admitted inputs changes the contract rather than proving it.

## Consequences

Successful proofs remain successful on slower hosts.
A host that reaches the hang cap still needs completed evidence before its gate can pass.
Bounded string proofs establish finite claims; exhaustive native tests and Miri complement their stated limits.
The checked obligation inventory and deterministic baselines continue to reject missing subjects and probes.
