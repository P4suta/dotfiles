---
status: proposed
date: 2026-10-13
---
# Require every fixed proof partition in hosted CI

## Context

The complete hosted campaign includes Kani, Miri, fuzz corpus replay, fixed Linux measurements, Quint, Alloy, and Lean.
Hosted run 38341479953 reached its existing one-hour job limit after Kani, Miri, fuzz replay, and measurement setup, leaving measurements and models incomplete.
The owner retains every required check and has paused hosted mutation testing pending njutest.
A failure must not prevent independent checks from collecting their results.

## Decision

The proposed CI implementation partitions the existing campaign into Kani, runtime checks, and measurements with models.
The runtime partition runs Miri and fuzz replay.
The measurement and model partition runs the unchanged fixed Linux budgets, Quint, Alloy, and Lean.
Each partition calls the existing maintained verifier entry points and retains their input inventories, rejecting controls, and budgets.
The full `just proofs-ci` command continues to run the complete campaign.
Its closed `--phase` argument serves the required hosted matrix.

The matrix contains every known partition exactly once, disables failure-driven cancellation, and permits at most two concurrent jobs.
Every job retains the original sixty-minute limit.
The required aggregate waits for the matrix and refuses any missing, failed, skipped, or cancelled required job.
Tool setup is limited to the partition that needs it.
Build caches use separate partition keys and provide build materials rather than successful verification credit.

The authoritative workflow checker refuses missing or repeated partitions, unknown entries, include or exclude overrides, a changed phase entry point, an increased concurrency limit, and a changed job time limit.
The pure inventory decision has source-bound Kani obligations and rejecting probes.
The workflow regressions exercise actual YAML mutations.
This implementation remains unverified until the focused checks and required hosted run finish.

This extends [the scoped CI campaign decision](local-mutation-and-scoped-ci-proofs.md) under [issue 165](https://github.com/P4suta/dotfiles/issues/165).
The local full mutation campaign remains required.

## Alternatives

Increasing the single job limit would retain one long serial dependency and increase its maximum occupancy.
Omitting phases after a slow proof would weaken the verification contract.
Arbitrary caller-selected subsets without a checked complete matrix would permit missing coverage.
Cancelling remaining partitions after one failure would conceal independent failures and create additional correction cycles.

## Consequences

Independent hosted work can finish while the longest partition continues.
The workflow reports each partition separately and preserves the aggregate publication gate.
Some setup and compilation are duplicated across jobs.
Current measurements must establish the actual cold and warm costs before claiming a performance improvement.
