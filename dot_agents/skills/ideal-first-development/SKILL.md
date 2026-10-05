---
name: ideal-first-development
description: >-
  Work backward from the complete intended user and developer experience and finish related work as a coherent verified change.
  Use for broad implementation planning, stalled incremental work, and completing an authorized outcome.
---

# Work backward from the finished contract

Define the complete experience for end users, library consumers, maintainers, and people diagnosing failures.
Derive the observable behavior, invariants, transitions, and completion evidence from that experience before optimizing around the current implementation.
Choose a coherent design within the authorized goal without asking the owner about routine details.

Inspect the affected producers, consumers, variants, platforms, and verification mechanisms together.
Find shared causes and the authoritative boundaries that remove a whole defect family.
Prepare failing specifications, representative fixtures, compatibility checks, and diagnostic seams before scattering local fixes.
Delegate to other agents only when the current instructions permit it.

Build the common invariant and migrate all affected paths in one coherent reviewable change.
Use types, exhaustive enums, ownership, private constructors, and state-specific evidence to make prohibited operations unavailable.
At external boundaries, check exact identities and bytes and fail explicitly when the evidence no longer applies.
Use TDD for behavior changes, `development-assurance` for the verification contract, and `formal-assurance` for every implementation change.

Resolve an identified gap instead of stopping at a limitation note, follow-up issue, or handoff document.
Continue through implementation, project gates, fixable failures, and authorized integration until the outcome works and passes verification.
When a check fails, fix its cause and rerun the checks the fix affects.
When a missing input, unavailable external service, or authorization boundary blocks completion, finish independent work and ask for the exact missing prerequisite.
Never fabricate evidence, bypass protections, publish a forbidden release, or expand into unrelated projects to claim completion.

Use `systematic-fixes` when a bug exposes a design or proof gap.
Record consequential decisions in ADRs and keep working notes out of the product.
Never count a documented unresolved defect as completion.
