---
status: amended
changed_by: [keep-workflow-evidence-local.md]
date: 2026-10-13
---
# Route CodeRabbit by current PR importance

Partly supersedes the blanket review obligations and pause exemption in [ADR 0008](0008-layered-review-capacity.md) and [ADR 0015](0015-scoped-coderabbit-pause.md).
Their capacity, billing, ledger, and machine-local pause controls remain applicable to the selected route.

## Context

Routine recoverable changes do not justify consuming either CodeRabbit review pool.
A procedural instruction alone permits an omitted classification, an unsupported escalation, or a stale result after the diff changes.
A successful service process or quota reservation also does not establish a completed review.

## Decision

Require an AI importance assessment for every PR and authenticate it with the existing Git signing identity.
Bind it to the repository, actual merge base, head, and binary diff digest.
Distinguish the actual checkout source from the PR destination and bind their nonzero repository identities to one verified GitHub fork network.
Preserve the guard's exact source-origin boundary and compose its receipt with the checked destination and exact diff.
Read the qualified source branch at publication, and preserve authenticated destination issue authority when a fresh clone has only the signed transport.
Reject omitted, invalid, duplicate, and stale assessments before managed publication.
Use the [shared criteria](../../home/dot_agents/skills/pull-request/references/importance.md) and require concrete changed-code evidence.
Keep severe levels rare rather than escalating by path or diff size.

Route `low` and `medium` to no CodeRabbit review, `high` exclusively to the guarded CLI, and `critical` exclusively to GitHub PR review.
Preserve all existing tests, proofs, required checks, and supported findings.
A pause blocks the required route rather than converting it into completed evidence.
An authenticated CLI receipt includes the actual terminal output and result for the exact committed diff.
PR completion requires actual current-head service evidence and complete unresolved-thread inspection.
Keep private CLI diagnostics out of the public assessment and PR body.
Authenticate original historical packets separately to preserve prior findings when the producer changes.
Their producer identity, version, bounded hash, owner signature, exact prior scope, and original data must remain valid under the current decoder.
This trusts the owner's original record for historical findings and never credits it as current-producer completion.

Keep the documented persistent PR description exclusion for every noncritical route.
Prepare service settings that disable general automatic and incremental reviews and opt in only `review:critical`.
Configuration files are proposed settings until applied and their effective service configuration is read back.
Do not grant a verifier's result to a settings file or a successful publication.

## Alternatives

Reviewing every substantive PR consumes separate pools without regard to concrete consequences.
Requiring both CLI and PR reviews for the same tier duplicates service work.
Escalating by filename or line count overclassifies routine work and misses small consequential changes.
An unsigned label or free-form body cannot bind the decision to the actual diff or its accountable producer.
Automatically retrying a skipped or paused review would spend capacity without satisfying the missing evidence.

## Consequences

The checked workflow can verify identity, classification structure, exact scope, exclusive routing, and required evidence.
It cannot prove that an AI's factual assessment is honest or complete.
Git signing trust, provider authenticity, and the effective account configuration remain external boundaries.
Provider completion is bounded service analysis, not a proof that every changed file or possible defect was examined.
Direct GitHub clients and manual bot commands can bypass the managed workflow.
No current-head review is claimed until the production adapter observes it.
Reassess these boundaries when the provider exposes an authoritative configuration or review API.
