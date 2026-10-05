# Apply layered review-capacity budgets

Status: accepted.

## Context

The review budget takes the largest value under a 90% capacity ceiling with a further 20% margin below that ceiling.
[Reserved review capacity](0007-reserved-review-capacity.md) kept 20% of the original limit and imposed one attempt per hour and four per day whatever the verified capacity.
Those fixed limits cap usage below the two-layer calculation.

## Decision

Use `floor(capacity * 0.9 * 0.8)`, rounding only the final result.
The production `capacity_budget` function computes the ratio 18/25 in u128, so the full u64 range never overflows.
Admission compares the proposed consumed count times 25 with capacity times 18, which avoids division and keeps the same final rounding.
After a proposed review, the provider's reported consumed count must fit this budget, and incomplete or inconsistent numeric evidence still refuses.
A capacity of ten admits seven consumed reviews, 49 admits 35, and three admits two, with no early rounding of the 90% ceiling.

Cap the managed Advanced command-line tool at seven attempts per rolling hour and 168 per rolling day.
Count real timestamps instead of imposing a one-hour gap after each attempt, and hold the file lock through preflight and analysis.
Keep a separate persistent quota-probe ledger with the same rolling ceilings and a one-minute gap between probes.
Manual probes spend that probe budget, and a refused preflight still spends its durable review reservation.
These query ceilings control local traffic and make no claim about a published provider allowance.
Keep all history, pause markers, identity checks, inactive paid usage, and the sole managed executor.
Keep the 1 MiB ledger maintenance barrier and the append-only audit history, and change no retention rules automatically.
An oversize ledger refuses further work without deleting entries, as checked through the production reservation function.

Apply the same formula in the shared workflow to verified pull request allowances and adaptive activity thresholds.
The documented highest-band seven-day threshold of 49 yields 35 events per rolling 168 hours.
Official policy can select a 24-hour or seven-day activity window, so require evidence of the applicable window and current counts.
Keep the owner's pull request review pause.

## Consequences

The required Kani gate checks the production arithmetic, admission, rolling count bounds, probe cooldown, and owner pause with reachable outcomes.
Refusal controls catch an eighth review, an upward-rounded budget, an eighth local attempt, an early probe, and a pause bypass.
Native tests cover ledgers, locks, durable failures, quota decoding, installation, and exact rolling-window boundaries.
Operating-system persistence and clocks, truthful service snapshots, and unmanaged clients stay external assumptions.
The ledger lock unlocks before closing its file, so an inherited child handle never prolongs a completed reservation.
A deterministic native regression holds an inherited handle across the parent's release, and checks exclusion before release and admission after it without retries or sleeps.
Native integration covers this lock and process boundary, and Kani covers the numeric core.
Reassess the boundary when locking APIs or supported platforms change.
The managed hourly limit stays at seven even when the service reports a higher quota, and a plan upgrade needs an explicit policy change.
