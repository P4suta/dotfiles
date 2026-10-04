---
name: ideal-first-development
description: >-
  Work backward from the complete intended user and developer experience and finish related work as a coherent verified change.
  Use for broad implementation planning, stalled incremental work, and completing an authorized outcome.
---

# Work Backward from the Finished Contract

Define the intended complete experience for end users, library consumers, maintainers, and people diagnosing failures.
Derive the observable behavior, invariants, supported transitions, and completion evidence from that experience before optimizing around today's implementation.
Choose the best coherent design within the authorized objective rather than asking the owner to settle routine implementation details.
Keep the promised behavior concrete enough to demonstrate; an imagined perfect system with no boundary is not an executable contract.

Inspect the affected producers, consumers, variants, platforms, and verification mechanisms together.
Identify shared causes and the authoritative boundaries that can eliminate an entire defect family.
Prepare the necessary failing specifications, representative fixtures, compatibility checks, and diagnostic seams before scattering local fixes.
Batch independent reads and checks and reuse valid evidence to reduce elapsed time without reducing assurance.
Delegate to other agents only when the current instructions authorize delegation.

Implement the common invariant and migrate all affected paths in a coherent reviewable change.
Use types, exhaustive enums, ownership, private constructors, and state-specific evidence to make prohibited operations unavailable.
For external conditions, validate exact identities and bytes at the boundary and fail explicitly when the evidence no longer applies.
Use TDD for behavior changes and `development-assurance` for the complete verification contract.
Use `formal-assurance` for mandatory assessment of every implementation change's affected contract, source-bound proofs wherever applicable and checked alternatives at genuine semantic or empirical boundaries.
Do not delay a known necessary correction merely to split the work into many tiny conversational steps.

An identified gap is work to resolve, not a reason to stop after writing a limitation, follow-up issue, or handoff document.
Continue through implementation, project gates, actionable failures, and authorized integration until the intended outcome is usable and verified.
If a check fails, correct its cause and repeat the checks affected by the correction.
Do not repeatedly rerun passing checks without a changed assumption or unresolved concern.
If a real missing input, unavailable external service, or authorization boundary prevents completion, finish independent work and ask for the exact missing prerequisite.
Do not fabricate evidence, bypass protections, publish a forbidden release, or expand into unrelated projects to claim completion.

Use `systematic-fixes` when an observed bug exposes a design or proof gap.
Preserve consequential decisions in ADRs and keep working notes out of the product.
Report the achieved outcome and its supporting evidence accurately; documentation of an unresolved defect does not make the task complete.
