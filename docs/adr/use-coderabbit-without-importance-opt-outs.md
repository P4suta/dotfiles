---
status: accepted
date: 2026-11-09
---
# Use CodeRabbit without importance opt-outs

## Context

The owner wants automatic review within the existing subscription.
This amends [importance routing](route-coderabbit-by-current-pr-importance.md), [high PR exemptions](require-no-cli-review-for-high-prs.md), and [readiness](0013-ready-only-after-passing-checks.md), and replaces the [repository exception](require-pr-review-by-repository-policy.md).

## Decision

Use CodeRabbit without importance thresholds, suppression commands, or opt-in labels.
Enable reviews for ready PRs and updates.
Preserve project checks, source identity, supported findings, billing limits, and owner pauses.
A pause blocks the operation without editing its PR description.
Ready accepts an empty check rollup when GitHub requires no checks.
Pending, missing required, and failed checks still block readiness.
Merge checks stay unchanged.

Follow the [automatic review controls](https://docs.coderabbit.ai/configuration/auto-review) and [configuration reference](https://docs.coderabbit.ai/reference/configuration).

## Alternatives

Selective review saves capacity but contradicts the owner's choice.
Manual opt-in requires repeated commands.

## Consequences

Retire suppression rules and probes; retain billing limits.
Remove suppression text from open PR descriptions and use automatic reviews after readiness or updates.
The [automatic review workflow](use-automatic-coderabbit-reviews-without-command-generation.md) removes command generation and admits incremental reviews.
Reconsider scope if included capacity blocks completed changes.

## Confirmation

Tests must accept ordinary descriptions without inserting suppression commands.
