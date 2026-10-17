---
status: accepted
date: 2026-10-09
---
# Compose the bounded Git argument proofs through a scalar cursor

## Context

The Git guard must retain every configuration setting before the subcommand and its exact source span.
String allocation and variable slice construction make the combined bounded proof exceed its budget.
The loop-contract pilot did not establish an unbounded proof within that budget.

## Decision

The production scan returns an allocation-free iterator over borrowed settings.
Its word classifier produces scalar configuration facts, and its cursor advances through those facts.
The runtime adapter owns the settings only after this scan.

Kani checks every literal word against an independent grammar over all admitted following words.
The actual cursor is checked over all word sequences up to four words, and the iterator adapter checks exact borrowed values, kinds, and spans.
Native exhaustive sequences compare the complete iterator with the same independent grammar.
Configuration policy proofs partition the six values into three pairs and retain every original subcommand and both real outcome witnesses.

## Validation

The proof inventory executes every word, cursor, adapter, and policy partition under the unchanged proof budget.
The attribution corpus and runtime tests cover the external Git boundaries.
The existing [loop-contract decision](bounded-harness-over-kani-loop-contracts.md) records the limit beyond the bounded domain.

## Reconsider

Reassess the composition when the supported grammar or cursor changes, or when the verifier can check the complete scan at a larger bound within the same budget.
