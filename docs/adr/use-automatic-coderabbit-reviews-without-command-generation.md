---
status: accepted
date: 2026-11-09
---
# Use automatic CodeRabbit reviews without command generation

## Context

The owner wants ordinary automatic reviews after readiness and updates, without service commands in PR documents or comments.
This replaces the generation modes in [checked PR authoring](0004-checked-pull-request-authoring.md) and refines [automatic review admission](use-coderabbit-without-importance-opt-outs.md).

## Decision

Remove command generation, insertion, and document validation for CodeRabbit directives.
Always create drafts with authored titles and descriptions.
Use the provider's [automatic reviews](https://docs.coderabbit.ai/configuration/auto-review), including incremental reviews, with `auto_pause_after_reviewed_commits: 0`.
Accept authenticated review metadata ending at the current head without requiring every incremental run to cover the canonical base or assessment paths.
Preserve completed current-head status, source binding, supported findings, required checks, owner pauses, and subscription limits.

## Alternatives

Keeping manual generation and full-review requests would preserve optional capabilities but contradict the requested workflow.

## Consequences

Remove the generation flag and prepare documents locally.
Provider metadata remains an external dependency.
Reconsider admission if the provider changes its completion protocol.

## Confirmation

Regression tests must accept ordinary incremental reviews and preserve unfinished-review and unresolved-finding refusals.
