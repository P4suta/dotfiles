---
status: amended
changed_by: [merge-after-coderabbit-review-limits.md]
date: 2026-11-10
---
# Honor explicit merges during optional review pauses

## Context

The owner rejects routine manual review commands and service administration during PR work.
Unconditional review holds in [automatic review guidance](use-automatic-coderabbit-reviews-without-command-generation.md) obstruct an explicit merge instruction when optional analysis pauses.

## Decision

Use [automatic incremental reviews](https://docs.coderabbit.ai/configuration/auto-review) without demanding a full review.
When the owner explicitly authorizes merging despite an unavailable optional review, resolve supported findings, pass required checks, and merge through normal GitHub admission with the [verified head](https://cli.github.com/manual/gh_pr_merge).
Preserve repository-required reviews, protections, and owner-imposed publication holds.
Keep service setup and capacity inspection scoped to requested service operations.

## Alternatives

Always waiting for fresh provider analysis could uncover further defects but makes the authorized merge depend on unrequested service operations.

## Consequences

Optional analysis can remain incomplete.
Report its actual state without claiming review completion.
Reconsider when a repository requires CodeRabbit review.

## Confirmation

Exercise the guidance with paused optional analysis and explicit merge authorization, active automatic review, failed required checks, unresolved findings, and absent merge authorization.
