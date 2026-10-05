---
name: adr
description: >-
  Record consequential architectural decisions and maintain their history with ADRs.
  Use when choosing or changing a durable design, not for routine edits, task notes, or duplicate system documentation.
---

# Architecture decision records

An Architecture Decision Record (ADR) states why the system has an important constraint.
Read the project's existing decisions before proposing a design.
Keep the project's location, numbering, status vocabulary, and indexes.
For a new collection, use `docs/adr/NNNN-short-decision.md`.

Write a record when a choice affects a public contract, dependency direction, state model, trust boundary, persistence, compatibility, or a major operational constraint.
A bug fix needs an ADR only when it changes such a decision.
Routine refactors and implementation details need none.

State the problem, the constraints, the decision, the rejected alternatives with reasons, and the resulting costs and benefits.
Use `Status`, `Context`, `Decision`, and `Consequences` sections, and add alternatives where they clarify the choice.
Name the decision, not the work that led to it.
Link to code, invariants, tests, or specifications instead of copying them.
Distinguish measured evidence, assumptions, proposals, and implemented behavior.

Land a new decision with the change that relies on it, and check its enforceable claims with types or tests.
Mark a record accepted only when the user accepts the decision.
Correct factual errors in place.
Record a changed decision in a new linked ADR, and mark the old record superseded in whole or in part.
Never rewrite an old record to claim the current design as the original decision.

Keep instructions, reference material, and rationale in their existing homes.
Never write summaries, handoffs, progress logs, or verification narratives that repeat an ADR or the code.
Give enough reasoning to revisit the choice.
