---
status: proposed
date: 2026-10-19
---
# Repository owner assessments for bot pull requests

## Status

Proposed.

## Context

The [review routing decision](route-coderabbit-by-current-pr-importance.md) authenticates assessments through Git signing keys.
Renovate creates PRs through an application account without a human Git signing identity.
Requiring that account to sign the assessment prevents the repository owner from maintaining its dependency updates through the checked workflow.

## Decision

Keep the original creator as the required signer for human PRs.
For a bot PR, require the authenticated repository owner and an explicit source within the same repository.
Refuse other accounts, fork sources, and missing source evidence.
Use the same signer selection for current assessments, edits, and historical findings.
Keep signature verification, exact source and diff bindings, review routing, and required checks.

The [production rule](../../crates/rules/src/review_routing_rules.rs) defines the complete Boolean input domain and its exclusive outcomes.
Its Kani harness and rejecting probes cover the owner and source restrictions.
The [workflow regressions](../../crates/pr-workflow/tests/pr_workflow.rs) exercise the GitHub and signing adapters.

## Consequences

The owner can assess Renovate updates without an application signing key.
GitHub account identity, repository metadata, and registered signing keys remain trusted external inputs.
Collaborators and organization members gain no substitute signing authority.
Bot PRs from forks need a separate trust decision before this workflow can support them.
Unsigned assessments and direct GitHub edits remain outside the authenticated workflow.

Using any collaborator as the signer would broaden the trust boundary beyond this maintenance need.
Skipping signatures for bots would discard accountable assessment evidence.
Copying bot changes into a human PR would duplicate work and obscure the original update history.
