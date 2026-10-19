---
status: accepted
date: 2026-10-09
---
# Check review postconditions directly on bounded production calls

## Context

Kani 0.68 cannot compile the review budget contract after MIR inlining.
Without that optimization, its contract instrumentation exceeds the existing proof budget.
The classifier also needs input partitions to finish within that budget.

## Decision

The review rules use ordinary harnesses that call `check_budget` and `classify` directly.
The budget harnesses retain both original postconditions and check an independent exact rolling-window oracle.
Every ledger length from zero through eight retains its full `u64` timestamp domain.
The first future record partitions larger rollback cases without excluding a record.

The classifier retains all twelve words at every original length from zero through three.
Fixed prefixes and explicit enumeration divide that finite domain, and every result of the actual classifier is checked against its independent oracle.
The existing module-wide Kani obligation runs every source harness and rejecting probe.
The two separate function-contract entries are redundant after this replacement and are removed.
This follows the accepted [bounded-harness decision](bounded-harness-over-kani-loop-contracts.md).

## Validation

Every partition must finish within the unchanged proof budget with its original postconditions and reachable cover witnesses.
Native exhaustive cases also compare the numeric oracle with the independent byte specification.
The module inventory refuses a missing production subject or harness.

## Reconsider

Reassess contract instrumentation when the pinned Kani compiler handles these functions within the same budget.
