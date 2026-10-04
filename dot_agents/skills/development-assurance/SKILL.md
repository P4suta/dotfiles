---
name: development-assurance
description: >-
  Develop behavior with TDD and maintain strong tests, traces, reproducibility, and failure diagnostics across every phase.
  Use for implementation and verification planning, not to invent tests for a prose-only edit.
---

# Development Assurance

Treat diagnosability and reliable verification as part of the product in every phase.
Development speed is secondary to the usefulness of these capabilities.
Reduce elapsed time by checking the complete affected contract together, batching independent work, and fixing shared causes once.
Design for end users, library consumers, maintainers, and people investigating failures.
Choose useful defaults and complete routine engineering decisions autonomously within the user's scope.
Ask only when missing information changes the intended behavior or an actual authorization boundary.

For a behavior change, define the observable contract, write a focused failing test, observe the intended failure, implement the behavior, and refactor with the tests passing.
For a defect, first reproduce it and then use `systematic-fixes` to address its cause and related cases.
Do not claim a test followed TDD if it was only run after implementation.
For documentation-only edits, verify accuracy and examples rather than adding artificial tests.

Keep fast deterministic specifications for pure rules, adapter contract tests at real boundaries, and integration or end-to-end tests for the workflows that require them.
Cover failure, interruption, cancellation, cleanup, retries, and concurrent transitions where they are part of the contract.
Use realistic fixtures, consumer examples, fault injection, property tests, fuzzing, or model checking when they answer a concrete uncertainty.
For every implementation change, use `formal-assurance` to assess the affected contract and require source-bound proofs wherever applicable before completion.
Choose a verifier that checks the language implementation or establish an explicit refinement to a Lean 4 model; tests and independent references supplement the proof, and genuine semantic or empirical boundaries require checked, justified alternatives.
Use `mutation-testing` to check whether tests detect broken behavior and `resource-coordination` before a campaign that substantially occupies a shared machine.
Keep mocks faithful to the external contract and use real-system checks for assumptions a mock cannot establish.
Do not substitute coverage percentages, snapshots of internal structure, repeated green runs, or a successful compile for the behavior being promised.

Make failures reproducible and attributable.
Prefer structured typed errors and trace events with stable operation or correlation identities, explicit phases and outcomes, and useful causal context.
Preserve the primary error while reporting cleanup or recovery failures.
Test the diagnostic contract when a caller or maintainer relies on it.
Never log credentials, private inputs, bearer capabilities, or sensitive payloads to make troubleshooting easier.

Expand the developer tools that remove real friction: a focused reproducer, deterministic fixture, explain or doctor command, replayable trace, test seam, or failure-injection harness.
Keep one source of truth for executable contracts and put consequential design reasoning in ADRs.
Use the repository's lint, typecheck, test, and security gates; do not weaken them to finish a change.
Before push, exercise the affected supported platforms on the owner's actual Mac, Windows, and Linux machines through `multi-machine` and `domyjob`.
Use CI to confirm the same contract and hosted-only checks after local assurance; do not push a known platform gap merely to ask CI what breaks.
Use `concise-source` to express intent in code and keep exceptions narrow.
Finish known necessary corrections through `ideal-first-development`; an acknowledged limitation or handoff is not completion.
Report what was verified, what evidence establishes it, and any specific unverified boundary.
