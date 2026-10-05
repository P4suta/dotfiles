# Apply layered review-capacity budgets

Status: accepted.

## Context

[ADR 0007](0007-reserved-review-capacity.md) kept 20% of the original limit and imposed one attempt per hour and four per day whatever the verified capacity.
Those fixed limits cap usage below a 90% capacity ceiling with a further 20% margin.

## Decision

Use `floor(capacity * 0.9 * 0.8)`, rounding only the final result.
The production `capacity_budget` function computes the ratio 18/25 in u128, so the full u64 range never overflows.
Admission compares the proposed consumed count times 25 with capacity times 18.
After a proposed review, the provider's reported consumed count must fit this budget, and incomplete or inconsistent numeric evidence still refuses.
A capacity of ten admits seven consumed reviews, 49 admits 35, and three admits two.

Cap the managed Advanced CLI at seven attempts per rolling hour and 168 per rolling day.
Count real timestamps instead of imposing a one-hour gap after each attempt, and hold the file lock through preflight and analysis.
Keep a separate persistent quota-probe ledger with the same rolling ceilings and a one-minute gap between probes.
Manual probes spend that probe budget, and a refused preflight still spends its durable review reservation.
Keep all history, pause markers, identity checks, inactive paid usage, and the sole managed executor.
Keep the 1 MiB ledger maintenance barrier and the append-only audit history, and change no retention rules automatically.
An oversize ledger refuses further work without deleting entries.

Apply the same formula in the shared workflow to verified PR allowances and adaptive activity thresholds.
The documented highest-band seven-day threshold of 49 yields 35 events per rolling 168 hours.
Official policy can select a 24-hour or seven-day activity window, so require evidence of the applicable window and current counts.
Keep the owner's PR review pause.

## Consequences

The required Kani gate checks the production arithmetic, admission, rolling count bounds, probe cooldown, and owner pause.
Refusal controls cover an eighth review, an upward-rounded budget, an eighth local attempt, an early probe, and a pause bypass.
Native tests cover ledgers, locks, durable failures, quota decoding, installation, and exact rolling-window boundaries.
Operating-system persistence and clocks, truthful service snapshots, and unmanaged clients stay the external assumptions of ADR 0007.
The ledger lock unlocks before closing its file, so an inherited child handle never prolongs a completed reservation.
A native regression holds an inherited handle across the parent's release, and Kani covers the numeric core.
Reassess the boundary when locking APIs or supported platforms change.
The managed hourly limit stays at seven even when the service reports a higher quota, and a plan upgrade needs an explicit policy change.
