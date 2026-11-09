# PR risk and review

Use CodeRabbit PR review for every importance level.
Importance describes consequences and never skips a review or selects an opt-in label.

A `low` change preserves behavior or has a small recoverable effect.
A `medium` change changes bounded behavior with effective recovery.
A `high` change has a supported severe consequence for authorization, secrets, security, irreversible data, or foundational assurance.
A `critical` change has a supported catastrophic consequence across a broad boundary with difficult or irreversible recovery.
Keep severe levels exceptional and explain the affected behavior and remaining consequences.
A filename, line count, or hypothetical failure supplies no severity.

The authoring tool binds its source assessment to the verified repository, actual merge base, head, and diff.
Keep its records in owner-local state.
Published descriptions explain the change and validation without local workflow metadata or suppression commands.

Inspect CodeRabbit's review of the current head and every unresolved finding.
Keep supported earlier findings when the diff changes.
A skipped, stale, failed, incomplete, or paused review supplies no completion credit.
A pause keeps the operation pending without changing its description.
Project checks and owner authorization remain necessary.

Enable automatic reviews for ready PRs and their updates without label filters.
Keep PR and CLI capacity separate and preserve the subscription's billing limits.
Use guarded CLI reviews for an explicit local review request.

The [automatic review controls](https://docs.coderabbit.ai/configuration/auto-review) describe review eligibility and updates.
The [review commands](https://docs.coderabbit.ai/guides/commands) describe service operations.
