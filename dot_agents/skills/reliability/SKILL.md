---
name: reliability
description: >-
  Prevent and diagnose intermittent failures in implementations, tests, hooks, and automation by controlling time, concurrency, state, ordering, randomness, external boundaries, and environment.
  Use for flaky behavior or changes that introduce these dependencies, not prose-only edits.
---

# Reliable behavior

Make correctness depend on explicit inputs, owned state, and verified transitions.
Find every uncontrolled dependency in the affected behavior, and remove it, give it a clear contract, or control it at a narrow boundary.
Determinism never requires removing legitimate concurrency, deadlines, or production randomness.

## Design the boundary

State the observable invariant, admitted inputs, failure behavior, and assumptions before implementation.
Separate decision logic from clocks, schedulers, randomness, filesystems, processes, and services.
Pass the relevant capabilities explicitly instead of adding global mutable test switches.
Use types, ownership, checked constructors, and exhaustive states to prevent invalid transitions.
Bind observations, caches, approval evidence, and completion to the exact operation, revision, resource, and generation they describe.

| Dependency | Guidance |
| --- | --- |
| Deadlines, calendars, expiry, backoff, seeds | [Time and randomness](references/time-and-randomness.md) |
| Threads, async work, locks, cancellation, ordering | [Concurrency and ordering](references/concurrency-and-ordering.md) |
| Files, databases, interrupted writes, cleanup | [Persistence and lifecycle](references/persistence-and-lifecycle.md) |
| APIs, processes, platforms, reproducible inputs | [External boundaries and environment](references/boundaries-and-environment.md) |
| Reproduction, fault injection, model checking, evidence | [Tests and diagnostics](references/tests-and-diagnostics.md) |

Use one authoritative boundary for a shared invariant, and inspect its other producers and consumers instead of scattering defensive conditionals.
Read the library and OS contract before relying on scheduling, atomicity, durability, or cancellation behavior.

## Verify the mechanism

For a discovered defect, use `systematic-fixes` to keep a failing reproducer and establish its cause.
Control the clock, input seed, schedule, resource identity, or injected failure that triggers it.
When OS behavior matters, exercise the supported native platforms through `multi-machine`.
Use `development-assurance`, `rust-invariants`, and fitting proof or model tools for the affected contract.

A rerun, extra sleep, larger timeout, extra retry, or serial execution alone fixes nothing.
Repair the responsible dependency or protocol, then show that the original failure stops and valid behavior still works.
Keep bounded test timeouts as hang diagnostics, and keep polling or retries only where the external contract requires them.
Preserve primary errors, causal logs, and cancellation or cleanup outcomes without exposing credentials.

Use `ci-budget` to finish local verification before an authorized publication.
Report the fixed mechanism, its regression evidence, and any boundary that stays unverified.
