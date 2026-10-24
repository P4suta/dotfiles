---
date: 2026-10-24
status: proposed
---
# Use plan delimiters as proposal context

## Context

The [answer evidence contract](bind-answer-evidence-to-the-current-request-and-draft.md) covers every nonempty draft line.
Planning replies use a balanced `<proposed_plan>` block with Markdown headings, lists, and examples.
Adding a proposal label before its delimiter breaks that format.

## Decision

Use one balanced plan block as the visible context for claims declared as proposals.
Those claims still need a reason and exact line coverage.
Other evidence kinds keep their existing label, source, and proof checks.
Proposals remain excluded from formal premises.

The [layout adapter](../../crates/dotfiles/src/answer.rs) ignores delimiter examples inside fenced code.
The [pure state transitions](../../crates/rules/src/answer_rules.rs) refuse nested, duplicate, and unmatched delimiters.
The complete draft still binds the receipt and Stop check.

Prefixing structural lines changes the required reply format.
Skipping planning replies would accept unchecked content.

## Consequences

Agents can verify a plan without adding labels to its Markdown.
Prose outside the plan still needs its normal evidence and labels.
The gate checks declared evidence, not whether a proposed action is wise or complete.
The [hook regressions](../../crates/dotfiles/tests/answer.rs) cover exact drafts, stale requests, and malformed blocks.
