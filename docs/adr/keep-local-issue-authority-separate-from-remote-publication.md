---
status: proposed
date: 2026-10-13
---
# Keep local issue authority separate from remote publication

## Context

The live GitHub prerequisite in [the authenticated context decision](authenticate-issue-context-at-edit-and-integration-boundaries.md) cannot prepare a project without a remote repository.
Local development needs the same issue and scope discipline while credentials and network access remain absent.
Repository files cannot authenticate implementation authority, and local approval cannot establish permission to publish.

## Decision

Provide an explicit local provider with issue creation, selection, status, and change checks.
Local initialization may create Git metadata and represents an unborn branch separately from a real source commit.
It creates no GitHub identity, signing key, or placeholder source commit.
The platform owns local records and guard consumption, while the PR workflow owns commands and later remote promotion.
Both providers use the existing scoped context decision.

Store the local project binding, issues, selections, and immutable history in owner-managed state outside the project.
Lock read-modify-write operations and write history before atomically replacing the active state.
Content identities link records and detect unmatched state replacement; they are not authentication against an owner rewriting that storage.
Retain interrupted candidate history without presenting it as a completed transition in the active chain.
Branch and real-source ancestry checks remain independent of issue status and path scope.
An unborn selection binds its first observed real commit under the state lock and then requires that commit's tree and ancestry.
The local provider activates only after explicit preparation and with no configured remote, never after a failed GitHub request.

Explicit promotion uses the normal live remote prerequisite, existing Git signing identity, source checks, and scoped start.
It links actual local history as prior work and never invents earlier remote authorization.
Local-only records cannot satisfy the signed remote transport or publication boundary.
The commands and executable examples are documented in [local issue development](../reference/local-issue-development.md), and implementation is tracked in [issue 173](https://github.com/P4suta/dotfiles/issues/173).
This decision remains proposed while implementation verification is incomplete.

## Alternatives

Creating a GitHub repository before implementation introduces network and credential requirements into local preparation.
Treating repository JSON as authority allows untrusted project content to create permission.
Reusing a remote receipt with invented identities or a fabricated head misrepresents the available facts.

## Consequences

Local projects gain the same registered issue and path prerequisite without remote access.
Required project verification remains separate and mandatory.
Owner-managed storage and supported hook execution remain explicit trust boundaries.
Remote publication still requires authenticated live evidence and owner authorization.
