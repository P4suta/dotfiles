---
name: adr
description: >-
  Record consequential architectural decisions and maintain their history with ADRs.
  Use when choosing or changing a durable design, not for routine edits, task notes, or duplicate system documentation.
---

# Architecture Decision Records

Use ADRs as the durable record of why the system has its important constraints.
Read the project's existing decisions before proposing a design or introducing another architecture document.
Keep the project's location, numbering, status vocabulary, and indexes when they already work.
For a new collection, use `docs/adr/NNNN-short-decision.md` and add an index only when it materially helps discovery.

Write a record when a choice affects a public contract, dependency direction, state model, trust boundary, persistence, compatibility, or a substantial operational constraint.
A bug fix needs an ADR only when it changes such a decision.
Do not make every PR, refactor, or implementation detail an ADR.
Existing projects are examples of useful reasoning, not mandatory architectures or templates for other projects.

State the actual problem and constraints, the selected decision, why plausible alternatives were rejected, and the resulting benefits and costs.
Use `Status`, `Context`, `Decision`, and `Consequences` as the small default structure; include alternatives where they clarify the choice.
Name the decision concretely rather than describing the work that led to it.
Link to the relevant code, invariant, test, or public specification instead of copying it.
Distinguish measured evidence, assumptions, proposals, and implemented behavior.

Implement a new decision with the change that relies on it, and validate its enforceable claims with types or meaningful checks.
Record acceptance only when that decision is actually accepted within the user's authorized scope.
An accepted ADR preserves historical reasoning.
Correct factual errors explicitly, and use a new linked ADR when the decision changes; mark the older record superseded, including partial supersession when appropriate.
Never silently rewrite an old record to claim the current design was always the original decision.

Keep instructions, reference material, and rationale in their existing authoritative homes.
Do not create parallel architecture summaries, handoffs, progress logs, or verification narratives that repeat the ADR or code.
Write outward-facing records in English, one sentence per source line, with enough reasoning to revisit the choice and no fixed length quota.
