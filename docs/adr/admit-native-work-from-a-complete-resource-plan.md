---
status: proposed
date: 2026-10-13
---
# Admit native work from a complete resource plan

## Context

The original host preflight compares free bytes with one fixed workspace threshold.
That comparison does not account separately for build growth, proof outputs, temporary files, safety headroom, or competing allocations.
The retained Linux failure window includes a 34,109,222,792-byte snapshot allocation, without establishing that snapshot as the sole cause of the failure.
An exhausted filesystem can also prevent the controller from preserving a job's terminal evidence.

## Decision

The proposed admission sums declared additional workspace, build, proof, temporary, safety, and competing allocations.
Every allocation declares bytes and file counts.
Checked arithmetic refuses overflow and an empty plan.
Missing byte measurements, missing inode measurements on filesystems that expose them, and insufficient capacity refuse new work.
The adapter measures the home and temporary volumes and uses their least available capacity conservatively.
Windows declares inode capacity inapplicable and retains the byte admission.

The production decision is in `crates/rules/src/resource_rules.rs`.
Its Kani harnesses cover the sum and admission over all admitted integer values.
Rejecting probes exercise missing measurements, overflow, insufficient bytes, exhausted inodes, and empty plans.
Native adapters remain outside the verifier's filesystem semantics and require actual host measurements and regression checks.

The initial declarations in `crates/dotfiles/src/hosts/resources.rs` are conservative estimates.
They are not proofs that arbitrary external processes consume no more than those estimates.
The Linux competing declaration accounts for the retained snapshot observation.
Its value does not configure or cap the snapshot process.
Actual peak measurements and the supported volume inventory must be reconciled before this issue is complete.
An unbounded competing consumer remains an external scheduling constraint to resolve rather than a passing capacity claim.

Admission occurs before new job submission and preserves the existing accepted-job recovery path.
A refused capacity plan does not authorize deleting source, active jobs, toolchains, backups, or unknown artifacts.
Cleanup continues through the maintained eligibility and lock checks.

This extends the host execution decisions under [issue 160](https://github.com/P4suta/dotfiles/issues/160).
The record remains proposed while calibrated resource declarations, native checks, and the remaining lifetime enforcement are incomplete.

## Alternatives

Repeating the fixed workspace check omits independently growing resources.
Using a prior successful retry as a peak bound provides no evidence for a new workload.
Ignoring unavailable inode measurements admits an unchecked filesystem boundary.
Deleting unrelated caches after exhaustion replaces prevention with recovery and risks protected work.

## Consequences

Insufficient predicted capacity becomes a preflight refusal with an inspectable limiting resource.
Conservative declarations can refuse otherwise feasible work until measurements justify a complete plan.
The arithmetic proof establishes admission under its supplied resource facts and declarations.
It does not establish physical limits for uncontrolled consumers or replace ongoing resource coordination.
