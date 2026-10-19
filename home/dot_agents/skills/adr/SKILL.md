---
name: adr
description: >-
  Record consequential architectural decisions and maintain their history with ADRs.
  Use when choosing or changing a durable design, not for routine edits, task notes, or duplicate system documentation.
---

# Architecture decision records

Read the project's existing decisions before proposing a design.
Preserve existing filenames, references, collection locations, and status vocabulary.
In personal repositories, name new records `<decision>.md` and add `date: YYYY-MM-DD` to their front matter.
Use `docs/adr/` for a new collection.
Follow the contribution rules of another owner's repository.

Where an index generator exists, derive a deterministic order from record metadata.
After merging concurrent additions, keep every source record and regenerate a conflicted index.
Resolve competing edits to the same record explicitly.

Write an ADR when a choice affects a public contract, dependency direction, state model, trust boundary, persistence, compatibility, or a major operational constraint.
Routine refactors and implementation details need none.

State the problem, constraints, decision, rejected alternatives with reasons, and costs and benefits, under `Status`, `Context`, `Decision`, and `Consequences`.
Name the decision, not the work behind it.
Link to code, invariants, tests, or specifications instead of copying them.
Separate measured evidence, assumptions, proposals, and implemented behavior.

Land a new decision with the change that relies on it.
Mark it accepted only when the user accepts it.
Correct factual errors in place.
Record a changed decision in a new linked ADR, and mark the old one superseded or partly superseded.
Never rewrite an old ADR to claim the current design as the original decision.
Never write summaries, handoffs, progress logs, or verification narratives that repeat an ADR or the code.
