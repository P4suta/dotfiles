---
status: accepted
date: 2026-10-07
---
# Name new decision records after their decision and place them by date

## Context

Each record's filename carried the next number, which every branch picked by hand.
Concurrent branches took the same number, so main held two records numbered 0014 and two numbered 0020, and parallel branches had to renumber before they merged.

## Decision

Records up to 0023 keep their numbers, so links to them keep resolving.
Every later record goes by the name of its decision, `docs/adr/<decision>.md`, and its front matter names the `date` of the decision.
Two branches pick the same name only for the same decision, and Git then reports the clash as a conflict on one path.
The index lists the numbered records in number order and the dated ones by date and name, so it doesn't depend on which branch merged first.

`just gen` and `just check` refuse a number past 0023 and a number shared by two records.
They also refuse a record with both a number and a date or with neither, and a `changed_by` entry that names no record.
Each refusal names the fix.
The pure `identity` rule in `crates/rules/src/adr_rules.rs` decides a record's identity.
Kani harnesses and a rejecting counterexample check it.
The second 0014 and 0020 records took dated names.

## Consequences

A new record needs no number, and nobody renumbers a branch before it merges.
The index labels a dated record with its date instead of a number.
