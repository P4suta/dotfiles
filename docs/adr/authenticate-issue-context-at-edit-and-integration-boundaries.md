---
status: amended
changed_by: [keep-workflow-evidence-local.md]
date: 2026-10-13
---
# Authenticate issue context at edit and integration boundaries

## Context

The checked PR workflow verifies an open issue when `start` runs but originally retains no context that subsequent edits or integration can inspect.
Skill instructions alone cannot enforce issue ordering when a client does not load them.
The owner requires issue preparation before implementation and has made automatic skill maintenance an experimental opt-in.
The enforcement must operate independently of that maintenance mode.

## Decision

The proposed implementation records the checked destination, owner identity, issue, checkout, branch, starting source, and explicitly declared path scope when `pr-workflow start` succeeds.
The platform crate owns the shared context representation and local persistence.
The PR workflow owns authenticated GitHub inspection and context production.
The runtime edit guard consumes local context without introducing a dependency from dotfiles to the PR workflow crate.

Owner-managed local state is a trust boundary.
Repository files, timestamps, and digests are not authentication or proof that an issue preceded an edit.
Tracked context transports therefore require the owner's existing Git signature and live destination identity before integration accepts them.
CI checks the signature against the destination owner's published signing keys and checks the live issue state, source relationship, and changed paths.
An unavailable identity or failed signature check supplies no authorization.
This introduces no new signing identity or secret distribution service.

Supported client edit hooks check the context before implementation mutations.
Read-only discovery and issue preparation remain available without an implementation context.
Codex callbacks use the documented canonical tool names and command packet fields.
Codex can continue a tool call when its callback fails or times out, so callback availability does not establish the integration guarantee.
Several checked issues may declare overlapping path scopes for an authorized combined change.
Path scope does not prove that every semantic change belongs to an issue.
The integration gate checks scope and live issue validity independently of skill loading and review maintenance.

Refreshing the same branch, destination and issue must preserve its previously registered scopes.
Persistence checks scope preservation under the existing owner-state lock before writing history, consuming adoption or replacing active state.
An omitted scope refuses the refresh and names the explicit paths required to preserve it.
This check imports no authority from an unverified previous receipt into a newly signed receipt.

Authorization validates each eligible prior source lineage once for the captured dirty snapshot.
Each dirty path still requires its own registered scope or unchanged approved baseline fingerprint.
The final snapshot and checkout identity checks remain independent of that validation schedule.

An ordinary start refuses dirty content outside previously checked scopes or an unchanged explicitly approved prior-work baseline.
Migrating authorized pre-existing work requires an exact, one-time owner-local approval through `--adopt-existing`.
The approval binds the destination, issue, scopes, checkout, branch, starting source, and every dirty fingerprint, and successful use consumes it into local history.
A missing approval produces a reviewable request and refuses the start; the request cannot authorize itself.
Approved prior work does not become evidence of newly enforced issue ordering.
Ordering observations apply to supported edits after a checked start.
Edits outside instrumented clients receive the integration scope and identity checks, without a claim that their chronology was observed.

This extends [ADR 0006](0006-global-personal-issue-prerequisite.md).
The implementation and its rejecting regressions are tracked in [issue 170](https://github.com/P4suta/dotfiles/issues/170).
This record remains proposed while the implementation and verification are incomplete.

## Alternatives

Repeating instructions in skills leaves enforcement dependent on client loading and agent memory.
Trusting a repository JSON file or its digest lets untrusted content fabricate authorization.
Treating a later issue check as proof of an earlier start misrepresents the available evidence.
Creating a second signing infrastructure adds credentials and operational dependencies without establishing a stronger owner identity.

## Consequences

The ordinary edit and integration paths gain an inspectable issue prerequisite.
The authenticated online boundary requires current GitHub access and fails when that access cannot establish the required identity.
Existing work retains its original authorization rather than receiving fabricated ordering evidence.
The implementation must preserve local discovery, supported client behavior, signature validation, and combined-issue scopes through regression checks.
