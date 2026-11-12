---
date: 2026-11-07
status: proposed
---
# Confirm verification with a short reason

## Status

Proposed.
Amends [answer evidence](bind-answer-evidence-to-the-current-request-and-draft.md), [snapshots](compare-agent-stops-with-request-snapshots.md), and [code-shape verification](verification-proportional-to-code-shape.md).

## Context

Detailed records don't establish that the agent's interpretation or premises are correct.

## Decision

Before completion and every push, agents must run applicable project checks, fix failures, and recheck changed inputs.
Require a short confirmation grounded in successful checks.
Resolve required blocked checks or ask the user for a decision before claiming completion.
Don't require evidence files, signed declarations, claim ledgers, or per-answer proofs, including at release.
Explicitly requested evidence and project-owned artifacts retain their scope.

Managed clients receive a verification reminder and retain reply-language and prose checks.
Shared ADR content checks remain independent.
Remove automatic answer-evidence and code-shape completion holds.
Git hooks give the same reminder and continue running project hooks, signatures, and push pauses.
PR readiness retains project checks and reviews without the owner-side code-shape admission check.
Explicit evidence commands remain available.

## Alternatives

Detailed evidence gates are rejected because of their submission cost.
Bare approval omits the agent's reason.

## Confirmation

Hook and profile tests check completion without evidence and preserve unrelated hooks.

## Consequences

The short explanation remains an agent judgment.
Project checks still detect failures within their coverage.
