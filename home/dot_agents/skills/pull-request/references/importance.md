# Required PR importance

Assess every proposed PR from its actual current diff before publication.
Use the least severe level justified by concrete remaining consequences after the change's existing protections.
Don't infer importance from a filename, changed-line count, issue priority, or the presence of security, hooks, CI, dotfiles, or proof tooling.

| Level | Consequences | Required CodeRabbit route |
| --- | --- | --- |
| `low` | Behavior-preserving work or a small recoverable effect. | None. |
| `medium` | Ordinary bounded behavior changes with effective recovery and no supported severe failure scenario. | None. |
| `high` | A credible severe failure in an authorization, secret, security, irreversible data, or foundational assurance boundary. | Guarded CLI. |
| `critical` | A credible catastrophic failure across a broad or systemic boundary with difficult or irreversible recovery. | GitHub PR. |

Keep `high` and `critical` exceptional.
Explain the affected code and concrete consequence for every assessment.
For either severe level, also state likelihood, blast radius, and rollback evidence.
A merely hypothetical severe scenario doesn't justify either level.
If an actual catastrophic scenario meets the `critical` criteria, don't classify it as `high` to substitute a CLI review.

The checked workflow binds the signed assessment to the exact repository, merge base, head, and diff.
For a fork contribution, bind both the destination and the actual checkout source to their verified GitHub repository and fork-network identities.
The guarded CLI receipt still belongs to the actual source.
Only the checked composition can permit its use for the verified destination and exact diff.
Missing, duplicate, malformed, or stale assessments refuse publication.
Reassess after any change to that binding.
The signature authenticates an accountable assessment.
That signature doesn't establish the accuracy of AI-generated prose.

Keep `@coderabbitai ignore` as a standalone PR description line for `low`, `medium`, and `high`.
Don't add `review:critical` to those PRs.
Reserve that opt-in label for a current `critical` assessment after local validation and the PR pool's capacity check.
Create drafts before requesting any PR review.
The severe routes remain exclusive: a `critical` PR doesn't require CLI review, and a `high` PR doesn't require PR review.

Review evidence must cover the same current diff as the assessment.
For `high`, use the guard's authenticated committed-review receipt rather than a successful invocation, reservation, or handwritten result.
For `critical`, inspect the actual current-head bot review from the PR merge base, coverage of the assessment's severity-bearing evidence paths, and all paginated unresolved findings.
An incremental-only response or unknown coverage at those paths remains pending until an authorized full PR review covers them.
Skipped, stale, failed, incomplete, rate-limited, or paused required reviews remain pending.
A pause never changes the assessed level or satisfies its required route.
Supported findings from earlier reviews remain obligations after a downgrade.
Reassessing a published `high` route uses `assess --pr NUMBER --prior-cli-receipt PATH` to carry the authenticated original finding history into the new signed assessment.
Missing prior history blocks a downgrade, even when the public body never referenced the prior result.
The historical reader preserves findings from an authentic older producer record and supplies no current-diff completion credit.
The classification doesn't relax any project test, proof, lint, security, or required-check obligation.

Use the prepared CodeRabbit YAML to opt in only `review:critical` and turn off automatic incremental review.
Apply organization settings apart from repository settings, then read back the effective configuration before claiming service adoption.
Arrays may keep inherited labels, so inspect the effective positive-label allowlist instead of trusting a copied fragment.
Later commits need a fresh assessment.
If it selects the PR route, request a review with explicit authorization.
Don't assume that removing and readding a label completes a review of the updated head.
Ordinary GitHub clients, manual bot commands, repository overrides, and unmanaged integrations remain outside this local workflow's enforcement boundary.

The [automatic review documentation](https://docs.coderabbit.ai/configuration/auto-review) defines label opt-in and incremental behavior.
The [review command documentation](https://docs.coderabbit.ai/guides/commands) defines the persistent description exclusion and effective-configuration readback.
