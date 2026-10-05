# Global personal issue prerequisite

Status: accepted.

## Context

A personal PR needs a recorded problem and a bounded scope before implementation.
External repositories and personal forks keep the destination project's contribution policy.

## Decision

Compile one reviewed owner policy into the shared `pr-workflow` executable.
Read exact GitHub repository metadata, and require the configured login and the numeric owner identity to agree.
Apply the prerequisite only to owned repositories that fork nothing, whatever the administrative permissions.
Before a personal PR's creation, edit, ready transition, or live final check, require a selected open issue in that destination and a visible closing reference.
Keep local-file document checks offline, and offer a read-only `start` command to prepare from an issue.
Require a live authenticated user, read quota headers on serialized REST reads, keep a nonzero reserve, and never retry automatically or publish from a stale cache.
Point the shared skill and global client instructions at preparation and evidence-based scope review.

## Assurance and boundaries

Required source-bound Kani harnesses prove the production `issue_gate` and `plan` functions, with reachable accepted and refused cases and a false issue-bypass control.
The quota proof covers the production reserve predicate.
Required native process tests cover GitHub response parsing, exact issue identity, fork handling, and visible Markdown references.
They also cover authentication refusal, invalid or low quota headers, API failures, and the absence of mutations on refusal.
These tests use isolated API responses, so the truth of authenticated GitHub metadata, authorization, and concurrent remote changes stay external assumptions.
The API offers no transaction that joins issue checks and publication, so the command reads live prerequisites right before publishing and inspects the result.
The checker proves nothing about issue timing, issue quality, scope compliance, or the truth of validation claims.
The shared skill checks those from the issue, the diff, and execution evidence.
The dedicated command forms the enforcement boundary, and a direct `gh` call bypasses it.
A REST preflight reserve neither proves future GraphQL capacity nor stops other consumers and secondary limits from exhausting the account budget.

## Reconsideration

Revisit the owner policy when the account identity changes or another personal owner joins.
Revisit the boundary when a native client hook can enforce the same command without parsing shell text or altering unrelated GitHub operations.
