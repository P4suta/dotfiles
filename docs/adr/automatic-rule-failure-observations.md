---
status: proposed
date: 2026-10-08
---
# Record rule failures at their execution boundaries

## Context

An incident command alone cannot observe failures that an agent forgets to report.
Issue 98 requires automatic collection and a completion condition that actually reproduces the failure.

## Decision

The shared structured refusal emitter records gate failures in the local skill-ops incident store.
The Git wrapper observes real failed pushes and successful rewrites of published history.
Native client collectors record failed tool responses and structured diagnostics, including object responses.
Malformed input and explicitly identified verification defects use their own source values.
Red tests, rejecting counterexamples, and dry runs remain excluded from observations, while their gates still execute.

An incident identity binds the rule, session, source, and evidence.
Replaying identical evidence is idempotent; conflicting content is rejected.
A mechanized discharge runs one existing Cargo test in an isolated target directory and requires a passing test that emits the incident's rule.
A named test, missing file, or unrelated passing test cannot discharge an incident.
An exemption requires a closed-set reason and a reconsideration condition.

## Verification

The incident unit and native tests cover source classification, exclusion, identity conflicts, replay, real Git failures, and discharge execution.
`crates/rules/src/incident_rules.rs` owns the pure collection and completion decisions, with Kani harnesses and rejecting probes.
