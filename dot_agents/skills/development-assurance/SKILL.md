---
name: development-assurance
description: >-
  Develop behavior test-first and maintain strong tests, traces, reproducibility, and failure diagnostics across every phase.
  Use for implementation and verification planning, not to invent tests for a prose-only edit.
---

# Development assurance

Treat diagnosability and reliable verification as part of the product in every phase, ahead of development speed.
Save time by checking the whole affected contract at once, batching independent work, and fixing shared causes once.
Design for end users, library consumers, maintainers, and people investigating failures.
Choose useful defaults, and make routine engineering decisions within the user's scope without asking.
Ask only when missing information changes the intended behavior or crosses an authorization boundary.

For a behavior change, follow Test-Driven Development (TDD): define the observable contract, write a focused failing test, watch it fail, write the behavior, and refactor with the tests passing.
For a defect, reproduce it first, then use `systematic-fixes` to remove its cause and related cases.
Never call a test TDD when it first ran after the implementation.
For documentation-only edits, check accuracy and examples instead of adding artificial tests.

Keep fast deterministic specifications for pure rules, adapter contract tests at real boundaries, and integration or end-to-end tests for the workflows that need them.
Cover failure, interruption, cancellation, cleanup, retries, and concurrent transitions where the contract includes them.
Use realistic fixtures, consumer examples, fault injection, property tests, fuzzing, or model checking to answer a concrete uncertainty.
For every implementation change, use `formal-assurance` to assess the affected contract and require source-bound proofs wherever they apply.
Choose a verifier that checks the implementation language, or refine the code explicitly to a Lean 4 model.
Tests and independent references supplement the proof, and genuine semantic or empirical boundaries need checked, justified alternatives.
Use `mutation-testing` to check that tests detect broken behavior, and `resource-coordination` before a campaign that occupies a shared machine.
Keep mocks faithful to the external contract, and check on the real system what a mock leaves unproven.
Coverage percentages, snapshots of internal structure, repeated green runs, and a successful compile never stand in for the promised behavior.

Make failures reproducible and attributable.
Use structured typed errors and trace events with stable operation or correlation identities, explicit phases and outcomes, and causal context.
Keep the primary error while reporting cleanup or recovery failures.
Test the diagnostic contract when a caller or maintainer relies on it.
Never log credentials, private inputs, bearer capabilities, or sensitive payloads.

Build developer tools that remove real friction: a focused reproducer, a deterministic fixture, an explain or doctor command, a replayable trace, a test seam, or a failure-injection harness.
Keep one source of truth for executable contracts, and put consequential design reasoning in an Architecture Decision Record (ADR).
Run the repository's lint, type check, test, and security gates, and never weaken them to finish a change.
Before a push, exercise the affected platforms on the owner's Mac, Windows, and Linux machines through `multi-machine` and `domyjob`.
Use CI to confirm the same contract and the hosted-only checks after local assurance, and never push a known platform gap to see what breaks.
Use `concise-source` to express intent in code and keep exceptions narrow.
Finish known necessary corrections through `ideal-first-development`, because an acknowledged limitation or handoff leaves the work incomplete.
Report what passed, the evidence for it, and any unverified boundary.
