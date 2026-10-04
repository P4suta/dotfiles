---
name: reliability
description: >-
  Prevent and diagnose intermittent failures in implementations, tests, hooks, and automation by controlling time, concurrency, state, ordering, randomness, external boundaries, and environment.
  Use for flaky behavior or changes that introduce these dependencies, not prose-only edits.
---

# Reliable Behavior

Make correctness depend on explicit inputs, owned state, and verified transitions.
Identify every uncontrolled dependency in the affected behavior and either remove it, give it a clear contract, or control it at a narrow boundary.
Keep the design proportional to the actual contract; deterministic behavior does not require removing legitimate concurrency, deadlines, or production randomness.

## Design the boundary

State the observable invariant, admitted inputs, failure behavior, and assumptions before implementation.
Separate decision logic from clocks, schedulers, randomness, filesystems, processes, and services.
Pass the relevant capabilities explicitly instead of adding global mutable test switches.
Use types, ownership, checked constructors, and exhaustive states to prevent invalid transitions where possible.
Bind observations, caches, approval evidence, and completion to the exact operation, revision, resource, and generation they describe.

Select the guidance that matches the actual dependency:

| Dependency | Guidance |
| --- | --- |
| Deadlines, calendars, expiry, backoff, seeds | [Time and randomness](references/time-and-randomness.md) |
| Threads, async work, locks, cancellation, ordering | [Concurrency and ordering](references/concurrency-and-ordering.md) |
| Files, databases, interrupted writes, cleanup | [Persistence and lifecycle](references/persistence-and-lifecycle.md) |
| APIs, processes, platforms, reproducible inputs | [External boundaries and environment](references/boundaries-and-environment.md) |
| Reproduction, fault injection, model checking, evidence | [Tests and diagnostics](references/tests-and-diagnostics.md) |

Use one authoritative boundary for a shared invariant.
Inspect other producers and consumers of that boundary instead of adding unrelated defensive conditionals everywhere.
Consult the actual library and OS contract before relying on scheduling, atomicity, durability, or cancellation behavior.

## Verify the mechanism

For a discovered defect, use `systematic-fixes` to preserve a failing reproducer and establish its cause.
Choose evidence that distinguishes the broken mechanism from a passing coincidence.
Control the clock, input seed, schedule, resource identity, or injected failure that triggers it.
Exercise the supported native platforms when OS behavior matters, using `multi-machine`.
Use `development-assurance`, `rust-invariants`, and appropriate proof or model tools for the affected contract; keep OS and service trust assumptions explicit.

A rerun, extra sleep, larger timeout, extra retry, or disabled parallelism alone does not establish a fix.
Repair the responsible dependency or protocol, then show the original failure is prevented and valid behavior still works.
Keep bounded test timeouts as hang diagnostics, and keep polling or retries only where the external contract actually requires them.
Preserve primary errors, causal logs, and cancellation or cleanup outcomes without exposing credentials.

Use `ci-budget` to finish local verification before an authorized publication.
Report the mechanism fixed, its regression evidence, and any boundary that remains unverified.
