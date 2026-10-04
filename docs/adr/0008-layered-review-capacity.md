# Apply Layered Review-capacity Budgets

Status: Accepted.

## Context

The owner wants the largest review budget consistent with a 90% capacity ceiling and a further 20% margin relative to that ceiling.
[ADR 0007](0007-reserved-review-capacity.md) retained 20% of the original limit and imposed one attempt per hour and four per day independently of verified capacity.
Those fixed limits constrain utilization more than the requested two-layer calculation.

## Decision

Use `floor(capacity * 0.9 * 0.8)`, rounding only the final result.
The production `capacity_budget` function calculates the equivalent ratio 18/25 in u128 to avoid overflow throughout the full u64 range.
Admission compares the proposed consumed count multiplied by 25 against capacity multiplied by 18, avoiding division while enforcing the same final rounding.
After a proposed review, the provider's reported consumed count must fit this budget; incomplete or inconsistent numeric evidence remains a refusal.
A capacity of ten admits seven consumed reviews, a capacity of 49 gives 35, and a capacity of three gives two without prematurely rounding the 90% ceiling.

Set the managed Advanced CLI attempt ceiling to seven per rolling hour and 168 per rolling day.
Count actual timestamps rather than imposing a one-hour gap after every attempt, while retaining the file lock throughout preflight and analysis.
Keep a separate persistent quota-probe ledger with the same rolling ceilings and a one-minute minimum gap to prevent rapid polling.
Manual probes consume that probe budget, and refused preflight still consumes its already-durable review reservation.
These query ceilings are local traffic controls, not an assertion about a published provider API allowance.
Preserve all existing history, pause markers, identity checks, inactive paid usage, and the sole managed executor.
Retain the existing 1 MiB ledger maintenance barrier and append-only audit history; increased admission does not authorize automatic retention changes.
An oversized ledger refuses further work without deleting entries, as verified through the actual reservation function.

Apply the same formula in the shared workflow to verified PR allowances and adaptive activity thresholds.
The documented highest-band seven-day threshold of 49 yields 35 events per rolling 168 hours, but official policy can select a 24-hour or seven-day activity window.
Require evidence of the applicable window and current counts rather than treating this example as an account-wide guarantee.
Keep the owner's PR review pause intact.

## Consequences

The required Kani gate checks exact production arithmetic, admission, rolling count bounds, probe cooldown, and owner pause with reachable outcomes.
Rejecting controls detect admitting an eighth review, rounding the budget up, admitting an eighth local attempt, probing too early, and bypassing a pause.
Native tests cover actual ledgers, locks, durable failures, quota decoding, installation, and exact rolling-window boundaries.
Operating-system persistence and clocks, truthful service snapshots, and unmanaged clients remain the external assumptions documented in ADR 0007.
The owned ledger lock explicitly unlocks before closing its file, so an inherited child handle cannot prolong a completed reservation.
The deterministic native regression holds a real inherited handle across the parent's release and checks both exclusion before release and admission afterward without retrying or sleeping.
This operating-system lock and process boundary is covered by native integration rather than a modeled Kani filesystem; the numeric decision core remains proved, and the boundary must be reassessed if locking APIs or supported platforms change.
The managed hourly limit remains seven if a future service response reports a higher quota; a plan upgrade requires an explicit policy change.
