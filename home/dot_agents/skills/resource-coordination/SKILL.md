---
name: resource-coordination
description: >-
  Plan and coordinate expensive verification on shared development machines with explicit owner permission and bounded resource use.
  Use for mutation testing, long proofs, fuzzing, benchmarks, and machine contention between agents.
---

# Expensive work and shared machines

Ordinary bounded checks belong to the authorized development task.
Use `formal-assurance` for reproducible verdicts and the project's documented pinned container or hosted runner for verification.
Apply its bounded-tool rule through the project's bounded container entrypoint or `dotfiles bounded run`.
A scheduling agreement never permits a native unbounded tool.
Resource allocations and occupancy estimates govern scheduling, never the verdict.
Work that can occupy a machine for hours, saturate CPU or memory, fill storage, disturb interactive use, or spend paid signing capacity needs the steps below.
Before launching it, inspect the maintained tool's dry run, selected scope, previous measurement, and known running jobs.
Never start the full workload to get an estimate.

Request explicit permission with the purpose, targets, machines, estimated duration and uncertainty, concurrency, and stop condition.
Ask once for the complete useful experiment.
Reuse a granted permission for the same workload and resource allocation, and ask again only when its scope or occupancy changes.
Permission to code, commit, push, or make CI pass grants no right to occupy the owner's machines for hours.
A paid signing rehearsal or publication needs its own authorization.

The owner may delegate unattended execution while away or asleep.
Unattended work requires explicit delegation and an explicit status or instruction confirming the owner's absence.
Keep it within the agreed machines, resource allocation, and stop conditions.
Silence never answers a pending permission request.
Prefer the least disruptive host, conservative concurrency, and a persistent job that supports stopping and inspection.
If the absence or resource allocation stays uncertain, keep the expensive job pending and finish independent work.

When the current task explicitly permits coordination between agents, use the current `domyjob` chat manual to find the agents on the intended machines and exchange a short resource request.
State the machine, workload, CPU and memory allocation, expected interval, job identity when available, and how to stop or release it.
Silence reserves nothing, so get an affirmative agreement from the relevant machine users before relying on availability.
Their agreement never replaces the owner's required authorization.
Without an enforced reservation, a chat agreement works as cooperative scheduling, not an exclusive lock, so recheck before starting.

Start verification jobs through the project's maintained runner.
Use `domyjob` only within the owner-side tooling boundary in `formal-assurance`, with owner hosts serving as optional caches.
Keep the runner, process, and submission identities, and report progress at phase changes.
Honor an incoming request to pause or release resources within the tool's safe cancellation contract.
Never launch duplicate jobs after a transport interruption or leave orphan workloads when changing scope.
On completion or cancellation, collect the evidence, verify the final state, and release the agreed capacity.
Then continue the original task through its remaining checks and authorized integration.
