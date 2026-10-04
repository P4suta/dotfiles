# Global Personal Issue Prerequisite

Status: Accepted.

## Context

Personal PRs need a recorded problem and bounded scope before implementation.
Per-repository rules duplicate this personal requirement, while service review alone does not prevent local publication without an issue.
External repositories and personal forks must retain the destination project's contribution policy.

## Decision

Compile one reviewed owner policy into the shared `pr-workflow` executable.
Read exact GitHub repository metadata and require the configured login and numeric owner identity to agree.
Apply the personal prerequisite only to owned repositories that are not forks, independently of administrative permissions.
Before personal PR creation, editing, ready transitions, or a live final check, require a selected open issue in that destination and a visible closing reference.
Keep local-file document checks offline, and provide a read-only `start` command for issue-first preparation.
Require a live authenticated user, inspect quota headers on serialized REST reads, preserve a nonzero reserve, and avoid automatic retries or stale publication caches.
Connect the shared skill and global client instructions to preparation and evidence-based scope review.

## Assurance and boundaries

The production `issue_gate` and `plan` functions are verified by required source-bound Kani harnesses, with reachable accepted and refused cases and an intentionally false issue-bypass claim.
Native required process tests verify GitHub response parsing, exact issue identity, fork handling, visible Markdown references, and absence of PR mutations on rejection.
The required quota proof covers the production reserve predicate; native tests cover authentication refusal, invalid or low quota headers, API failures, and one authentication attempt per command.
These tests use isolated API responses; authenticated GitHub metadata truth, authorization, and concurrent remote changes remain external assumptions.
Read live prerequisites immediately before publication and inspect the published result; the API does not provide a combined issue-validation and PR-publication transaction.
The checker does not prove issue timing, issue quality, scope compliance, or validation-claim truth.
The shared skill verifies those properties from the issue, actual diff, and execution evidence.
The dedicated command is the enforcement boundary; direct `gh` invocation is not intercepted.
REST preflight reserves do not prove future GraphQL capacity or prevent other consumers and secondary limits from exhausting the account budget.

## Reconsideration

Revisit the owner policy when account identity changes or another personal owner is explicitly added.
Revisit the boundary when a supported native client hook can enforce the same command without parsing shell text or altering unrelated GitHub operations.
