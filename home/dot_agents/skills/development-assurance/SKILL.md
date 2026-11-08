---
name: development-assurance
description: >-
  Develop behavior with TDD and maintain strong tests, traces, reproducibility, and failure diagnostics across every phase.
  Use for implementation and verification planning, not to invent tests for a prose-only edit.
---

# Development assurance

Make failures diagnosable and verification reliable in every phase, ahead of development speed.
Check the whole affected contract at once, batch independent work, and fix shared causes once.
Make routine engineering decisions within the user's scope without asking.
Ask only when missing information changes the intended behavior or crosses an authorization boundary.

For a behavior change, follow TDD: define the observable contract, write a focused failing test, watch it fail, write the behavior, and refactor with the tests passing.
Use the project's maintained verification commands and keep their actual results with the checked source.
Complete the applicable checks required at the current project stage before reporting implementation completion.
Checks assigned to later CI or release stages keep their own timing.
Use the [optional execution runner](references/verification-contract.md) when ordered native receipts help the task.
Routine edits and replies need no parallel JSON plan.
For a defect, reproduce it first, then use `systematic-fixes` to remove its cause and related cases.
Never call a test TDD when it first ran after the implementation.
For documentation-only edits, check accuracy and examples instead of adding artificial tests.

Keep fast deterministic specifications for pure rules, adapter contract tests at real boundaries, and integration or end-to-end tests for the workflows that need them.
Cover failure, interruption, cancellation, cleanup, retries, and concurrent transitions where the contract includes them.
Use realistic fixtures, fault injection, property tests, fuzzing, or model checking to answer a concrete uncertainty.
Use `formal-assurance` for project-required proofs and questions that need formal methods.
Choose a verifier that checks the implementation language, or refine the code explicitly to a Lean 4 model.
Genuine semantic or empirical boundaries need checked, justified alternatives.
Use `mutation-testing` for affected behavior when the project requires it or the user requests it, and complete its full inventory at the project's release gate.
Use `resource-coordination` before a campaign that occupies a shared machine.
Keep mocks faithful to the external contract, and check on the real system what a mock leaves unproven.
Coverage percentages, snapshots of internal structure, repeated green runs, and a successful compile never stand in for the promised behavior.

Use structured typed errors and trace events with stable operation or correlation identities, explicit phases and outcomes, and causal context.
Keep the primary error while reporting cleanup or recovery failures.
Never log credentials, private inputs, bearer capabilities, or sensitive payloads.

Keep one source of truth for executable contracts, and put consequential design reasoning in an ADR.
Run the repository's fast lint, typecheck, test, and security gates on the current host, and never weaken them to finish a change.
Leave other platforms and heavy campaigns to hosted CI and the release gate, where `ci-budget` places them.
Before an authorized push, complete the fast gate and report hosted checks still pending.
Use `concise-source` to express intent in code and keep exceptions narrow.
Finish known necessary corrections through `ideal-first-development`.
Confirm completion with a short reason naming the checks that passed.
Run missing required checks instead of ending with an unverified disclaimer.
Repair the environment or use a justified equivalent.
If still blocked, ask the user for a concrete decision before claiming completion.
Don't require detailed evidence files, signed declarations, claim ledgers, or per-answer proofs.

Keep owner-side tooling within the boundary in `formal-assurance`.
