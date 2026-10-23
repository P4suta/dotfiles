---
status: accepted
date: 2026-10-23
---
# Link issues with repository owner authority

## Status

Accepted.

## Context

Human PR assessments authenticate the original creator before source acceptance.
Repository owners also need to add closing references to existing contributor PRs.
That metadata action needs its own authority without replacing the contributor's assessment or changing the review decision.
The [checked authoring decision](0004-checked-pull-request-authoring.md) already separates document checks, remote observations, and source verification.

## Decision

Add `pr-workflow link-issue --repo OWNER/REPO --pr NUMBER --issue NUMBER` for the authenticated numeric repository owner.
Require an open PR, an open issue in the exact destination, and verified source and destination repository identities within one fork network.
Check the named head and base branches against their current commits.
Keep the original body as an exact prefix and append one closing-reference paragraph, or leave an existing matching reference unchanged.
Preserve the title, creator, draft state, labels, review requests, assessment markers, and CodeRabbit directives.

Send only the body field through GitHub's [PR update API](https://docs.github.com/en/rest/pulls/pulls?apiVersion=2022-11-28#update-a-pull-request).
Recheck the selected issue and PR before that one write attempt, then require a matching acknowledgement and fresh readback.
Stop on an uncertain write or a changed result without retrying.
The normal assessment, review, check, readiness, and merge gates continue to govern source acceptance.

The [production rules](../../crates/rules/src/pr_rules.rs) decide owner admission and paragraph separation.
The [metadata adapter](../../crates/pr-workflow/src/link_issue.rs) checks live identities and publishes the body-only request.
The [shared source binding](../../crates/platform/src/source_identity.rs) verifies repository networks without requiring a contributor checkout for this operation.

## Consequences

Owners can maintain issue references without access to a contributor's signing keys.
The operation trusts authenticated GitHub metadata and the standard string and JSON libraries.
It supplies no new source approval or review completion evidence.

This implementation sends no atomic comparison condition with the metadata write.
A concurrent edit between the final read and the write can still escape detection and lose its changes.
The before-and-after comparisons detect changes they observe, while concurrent remote edits remain an external assumption.
Revisit that boundary if GitHub documents a suitable conditional metadata update and the integration can verify its semantics.

## Alternatives

Requiring a fresh creator-signed assessment for an issue reference preserves one authoring path but prevents owners from maintaining contributor PR metadata themselves.
Replacing the creator's assessment with an owner assessment would broaden source acceptance authority beyond this metadata action.
Issue-side comments retain a discussion reference but don't add the required closing reference to the PR description.
