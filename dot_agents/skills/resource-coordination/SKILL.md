---
name: resource-coordination
description: >-
  Plan and coordinate expensive verification on shared development machines with explicit owner permission and bounded resource use.
  Use for mutation testing, long proofs, fuzzing, benchmarks, and multi-agent machine contention.
---

# Expensive Work and Shared Machines

Distinguish ordinary project checks from work likely to occupy a machine for hours, saturate CPU or memory, fill storage, interfere with interactive use, or consume paid signing capacity.
Inspect the maintained tool's dry run, selected scope, previous measurement, and currently known jobs before launching expensive work.
Do not obtain an estimate by starting the full expensive workload.
Ordinary bounded checks remain part of the authorized development task.

For a substantial workload, request explicit permission with its purpose, selected targets, machines, estimated duration and uncertainty, concurrency, and stop condition.
Keep the request concrete and prefer the complete useful experiment over a series of poorly scoped requests.
Reuse an already granted permission for the same workload and budget; ask again only when its actual scope or occupancy materially changes.
Permission to implement, commit, push, or make CI pass is not by itself permission to monopolize the owner's machines for hours.
A paid signing rehearsal or publication also needs its own actual authorization.

The owner may authorize unattended execution while away or asleep.
Use that exception only when unattended execution has been explicitly delegated, the owner's absence is established by an explicit status or instruction, and the workload stays within the agreed machines, budget, and stop conditions.
Elapsed silence alone is not an answer to a pending required permission request.
Prefer the least disruptive available host, conservative concurrency, and a persistent job that can be stopped and inspected.
If absence or the allowed budget is uncertain, keep the expensive job pending and finish independent work.

When the current task explicitly authorizes agent coordination, use the current `domyjob` chat manual to discover the agents using the intended machines and exchange a short resource request.
State the machine, workload, CPU and memory budget, expected interval, job identity when available, and how to stop or release it.
Require an affirmative agreement from the relevant known machine users before relying on their availability; their silence does not reserve the machine.
Machine-user agreement prevents contention and does not replace the owner's required authorization.
Do not send unrelated messages, spawn new agents, or grant them write access merely to coordinate capacity.
If the tooling has no enforced reservation, treat chat agreement as cooperative scheduling rather than an exclusive operating-system lock and recheck before starting.

Start authorized local jobs through the project's maintained runner and remote jobs through `domyjob`.
Retain the local process or remote machine and submission identities, and report progress at meaningful phase changes.
Honor an incoming request to pause or release resources within the tool's safe cancellation contract.
Do not launch duplicate jobs after a transport interruption or leave orphan workloads when changing scope.
On completion or cancellation, collect the evidence, verify the actual final state, and release the agreed capacity.
Continue the original development task through its remaining checks and authorized integration.
