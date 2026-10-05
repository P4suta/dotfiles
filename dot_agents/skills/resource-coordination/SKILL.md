---
name: resource-coordination
description: >-
  Plan and coordinate expensive verification on shared development machines with explicit owner permission and bounded resource use.
  Use for mutation testing, long proofs, fuzzing, benchmarks, and machine contention between agents.
---

# Expensive work and shared machines

Distinguish ordinary project checks from work that can occupy a machine for hours, saturate processors or memory, fill storage, disturb interactive use, or spend paid signing capacity.
Before launching expensive work, inspect the maintained tool's dry run, selected scope, previous measurement, and known running jobs.
Never start the full workload to get an estimate.
Ordinary bounded checks belong to the authorized development task.

For a large workload, request explicit permission with its purpose, targets, machines, estimated duration and uncertainty, concurrency, and stop condition.
Ask once for the complete useful experiment instead of a series of vague requests.
Reuse a granted permission for the same workload and budget, and ask again only when its scope or occupancy changes.
Permission to code, commit, push, or make CI pass grants no right to occupy the owner's machines for hours.
A paid signing rehearsal or publication needs its own authorization.

The owner may delegate unattended execution while away or asleep.
Use that delegation only when the owner granted it explicitly, an explicit status or instruction confirms the absence, and the workload stays within the agreed machines, budget, and stop conditions.
Silence never answers a pending permission request.
Prefer the least disruptive host, conservative concurrency, and a persistent job that supports stopping and inspection.
If the absence or the budget stays uncertain, keep the expensive job pending and finish independent work.

When the current task explicitly permits coordination between agents, use the current `domyjob` chat manual to find the agents on the intended machines and exchange a short resource request.
State the machine, workload, processor and memory budget, expected interval, job identity when available, and how to stop or release it.
Before relying on availability, get an affirmative agreement from the relevant known machine users, because silence reserves nothing.
Their agreement prevents contention but never replaces the owner's required authorization.
Never send unrelated messages, spawn new agents, or grant them write access to coordinate capacity.
Without an enforced reservation, treat a chat agreement as cooperative scheduling, not an exclusive lock, and recheck before starting.

Start authorized local jobs through the project's maintained runner and remote jobs through `domyjob`.
Keep the local process, remote machine, and submission identities, and report progress at meaningful phase changes.
Honor an incoming request to pause or release resources within the tool's safe cancellation contract.
Never launch duplicate jobs after a transport interruption or leave orphan workloads when changing scope.
On completion or cancellation, collect the evidence, verify the actual final state, and release the agreed capacity.
Then continue the original development task through its remaining checks and authorized integration.
