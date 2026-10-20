---
date: 2026-10-19
status: amended
changed_by: [gate-pull-requests-that-skip-local-hooks.md]
---
# Make native verification optional before publication

## Status

The owner accepted optional native checks and lightweight CI on 2026-10-19 under [issue 201](https://github.com/P4suta/dotfiles/issues/201).
This amends the native publication prerequisite in [the earlier CI boundary](native-verification-with-lightweight-ci.md) and [OS family push evidence](0020-os-family-push-evidence.md).
A [hosted dependency gate](gate-pull-requests-that-skip-local-hooks.md) later added a check for pull requests that skip the local hooks.

## Context

Mandatory native campaigns delay every publication while proofs and cross-host checks complete.
The owner prioritizes publishing usable dotfiles and accepts optional native verification.
Interrupted and unrun checks still need accurate outcomes.

## Decision

The committed `policy/native-publication.toml` declares whether publication requires native evidence.
Pre-push and PR readiness read the policy from the exact candidate commit.
Missing policy preserves mandatory native admission, and malformed policy refuses publication.
An uncommitted edit leaves a candidate's policy unchanged.
Dotfiles sets `require_native = false`.

Explicit native verification keeps its existing evidence validation, failure records, source binding, and manual commands.
Optional admission records no passing native result.
CI reports the checked-out source on three operating systems and requires successful lightweight orchestration.
GitHub requires those three reports and their aggregate, with no retired rehearsal jobs.
Signing, issue authority, staged checks, current-head required checks, and review policy remain active.

## Alternatives

Waiting for every full campaign preserves stronger publication evidence and the observed delays.
Skipping hooks or fabricating native receipts would hide the actual policy and verification outcomes.
The committed policy instead makes the owner's chosen boundary explicit and reviewable.

## Consequences

A successful publication no longer establishes complete native verification.
PR validation must distinguish actual results from interrupted or unrun checks.
The owner can require native publication evidence again by committing `require_native = true`.
