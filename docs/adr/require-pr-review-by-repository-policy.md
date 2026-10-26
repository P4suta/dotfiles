---
status: proposed
date: 2026-10-26
---
# Require PR review by repository policy

Partly supersedes [importance routing](route-coderabbit-by-current-pr-importance.md) and [owner-local workflow evidence](keep-workflow-evidence-local.md).
Their signed assessments, findings, capacity boundaries, and owner-controlled pauses remain applicable.

## Context

Importance selects one review route for the actual diff.
A repository can also require CodeRabbit PR review for every change.
Changing importance to obtain that route would misstate the consequences of the change.
The existing workflow keeps its review records in owner-local state, and its title exception names a repository and a reason explicitly.

## Decision

Load an optional repository exception from `~/.local/state/pr-workflow/review-policies/owner__repo/policy.json`.
Require the exact repository, `review: always_pr`, and a nonempty one-line reason.
Fold repository case when locating and checking the file, bound its size, and refuse malformed or nonregular files.
An absent file preserves importance routing.
Keep the policy outside tracked projects and published documents.

The pure [routing core](../../crates/rules/src/review_routing_rules.rs) selects PR review for every level under this policy.
The [workflow adapter](../../crates/pr-workflow/src/review_policy.rs) keeps the signed assessment and current-head review checks.
Create and edit accept no standing suppression, and readiness and merge require actual full PR review of the current head.
No critical opt-in label or CLI receipt selects this route.
A PR pause blocks publication and integration without inserting suppression, and only the owner resumes it.
Default critical readiness keeps its existing behavior.

Remember the applied policy beside each PR's last verified assessment.
Earlier records default to importance routing.
Changing a prior CLI route still carries its authenticated findings, while a high assessment reviewed through PR creates no historical CLI obligation.
Read-only checks can't overwrite the earlier CLI route before that history check.

The local exception neither configures the service nor establishes allowance.
The repository's effective service settings must enable automatic draft reviews without positive label requirements.
PR and CLI pools, adaptive thresholds, and pause scopes remain separate.

## Alternatives

A tracked policy would expose a personal workflow choice and require a project change to select an owner-side route.
A command-line exception would need repetition for every operation.
A hardcoded repository exception would require a binary release whenever another repository adopts the policy.
An importance escalation would give the assessment a different meaning.

## Consequences

Each owner machine records its own policy once.
Unit tests enumerate both policies and every importance level, and the routing harness imports production functions.
Boundary tests cover unsuppressed publication, current-head readiness and merge, separate pauses, repository isolation, malformed input, and prior CLI history.
Service configuration, provider authenticity, signing trust, and changing allowance remain external assumptions.
