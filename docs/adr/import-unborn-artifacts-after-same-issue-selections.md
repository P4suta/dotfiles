---
status: proposed
date: 2026-10-26
---
# Import unborn artifacts after same-issue selections

## Context

[Local issue authority](keep-local-issue-authority-separate-from-remote-publication.md) supports both selection before edits and attestation of initial artifacts.
An early partial selection blocks the initial import route, while ordinary selection refuses later dirty artifacts outside that scope.
The project needs a route to attest its complete initial baseline without inventing earlier issue chronology or permitting late adoption after a source commit.

## Decision

Permit initial import with prior selections only when every selection names the import's open issue and current branch and still has an unborn source binding.
Continue to require no source refs, no remotes, no prior import or remote promotion, complete staged scope coverage, and an unchanged preview.
The existing owner signing identity attests the reviewed baseline.
Replace prior selections with its explicit scopes under the existing local state lock.
The preview grants no authority, and the import grants only future scoped authority.

Keep ordinary selection's dirty-path check unchanged.
Allowing dirty selection while unborn would skip the reviewed snapshot and signed attestation.
Merging earlier scopes would retain future authority outside the scopes reviewed for import.
Reject conflicting issue, branch, or committed-source selections rather than silently removing them.

The shared admission rule and proof inventory retain the unborn requirement independently of history compatibility.
The [CLI regressions](../../crates/pr-workflow/tests/pr_workflow.rs) cover import after early selection, scope replacement, incompatible histories, and refusal after the first commit.

## Consequences

An unborn project can attest initial artifacts after selecting part of its scope early.
The owner must include every desired future scope in the import.
Existing preview invalidation, signing, and one-time import checks remain in force.
Committed repositories retain the existing protection against late adoption.
