---
name: adr
description: >-
  Require concise architecture decisions with evidence, fair alternatives, and confirmation.
  Use before consequential design choices and when reviewing or replacing ADRs, not for routine edits or progress logs.
---

# Architecture decision records

Read existing decisions and governing primary sources before choosing a design.
Record one consequential choice before implementation relies on it.
Routine implementation details need no architecture decision record (ADR).

Use short paragraphs under Context, Decision, Alternatives, Consequences, and Confirmation.
Preserve an existing project's equivalent headings, paths, and status vocabulary.
State the problem and decisive constraint, chosen behavior and evidence, strongest possible alternative including the status quo, accepted cost, and observable confirmation.
Give a concrete trigger for reconsideration.
Use `checked-arguments` to challenge the decisive premise and strongest objection.
Keep facts, assumptions, proposals, and implemented behavior distinct.

Stay within the enforced size budget.
Link evidence and implementation details without moving the rationale into an appendix.
Remove repetition, generic benefits, progress logs, and ceremonial sections.
A material evidence gap keeps the record proposed.
Only the owner's acceptance of the reviewed choice permits accepted status.
Reuse explicit acceptance already given.

In personal repositories, name a new record `docs/adr/<decision>.md` with a front-matter date.
Preserve historical filenames.
Record changed decisions in linked successors and supersede the original only after acceptance.
Regenerate an existing index from its records.

Run the shared ADR checker and the project's checks.
Agent write and completion gates must reject oversize or incomplete changed records.
A structural pass proves neither sound reasoning nor factual truth.

[Nygard](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions), [Markdown Architectural Decision Records](https://github.com/adr/madr/blob/4.0.0/template/adr-template.md), and the [Amazon Web Services lifecycle](https://docs.aws.amazon.com/prescriptive-guidance/latest/architectural-decision-records/adr-process.html) supply the record and review foundations.
The enforced size budget is owner policy.
