---
status: accepted
date: 2026-10-10
---
# Retain applied profile drift as advisory evidence

## Context

An installed release can contain the defect that a candidate corrects.
Requiring that release to match its source blocks validation of the correction.
This amends the applied profile requirement in [the host gate decision](isolated-code-and-native-host-gates.md).

## Decision

The real host stage MUST apply and verify the candidate in scratch storage and fail on every candidate installation or fetch failure.
The applied profile observation MUST remain read-only and advisory, including configuration and verification errors.
The gate output and the real stage's Git note MUST retain every drifted target and any verification error in the `applied` field.
Only the code and candidate results determine [admission](../../crates/rules/src/hosts_rules.rs).

The [adapter](../../crates/dotfiles/src/host_checks.rs) reads the second column of the pinned chezmoi status output, which compares actual targets with their rendered source.
Its persistent state and caches stay in scratch storage.
The [native fixture](../../crates/dotfiles/src/host_checks.rs) checks multiple drifted targets and preservation of host files.
The [gate fixture](../../crates/dotfiles/tests/hosts.rs) checks advisory errors, visible drift, note persistence, and strict candidate failures.
These filesystem, process, and status-format boundaries rely on the pinned tools and native checks; the existing admission proofs remain unchanged.

## Alternatives

Failing on installed drift prevents a correcting candidate from passing.
Omitting installed verification removes evidence of host drift.
Applying the candidate to the real home changes the host during validation.

## Consequences

A passing note establishes code and candidate success while retaining installed drift for inspection.
Older notes retain their interpretation, and every new worker report requires the applied observation.
Normal profile application remains the rollout path.
