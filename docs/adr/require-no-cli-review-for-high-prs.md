---
status: accepted
date: 2026-11-04
---
# Require no CLI review for high PRs

## Context

[Route CodeRabbit by current PR importance](route-coderabbit-by-current-pr-importance.md) made every `high` PR wait for a guarded CLI review and its receipt.
Each update of a `high` PR then needed another review, and a merge that brought the branch up to date with its base needed one more.
These per-PR reviews cost more time than they returned.

## Decision

`low`, `medium`, and `high` assessments select no CodeRabbit review, and `critical` keeps the GitHub PR review.
A guarded CLI review runs on an explicit request.
The assessment levels and their criteria stay as they were, so a later change of route needs no reassessment.

The guard reinstalls the pinned CodeRabbit runtime through mise when the CLI has replaced it, and refuses a review when the restored runtime still reports another version.

## Consequences

A `high` PR merges after its checks without a service review.
A requested review still runs through the guard, and its findings still bind the PR they cover.
