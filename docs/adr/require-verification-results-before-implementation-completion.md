---
date: 2026-11-06
status: deprecated
---
# Require verification results before implementation completion

The proposal wasn't accepted.
The [project check proposal](keep-project-checks-authoritative-in-agent-sessions.md) keeps registration explicit.

## Context

Harness presence leaves execution and TDD ordering unchecked.
The owner requires affected checks before implementation completion and the full inventory before release.

## Decision

Managed sessions register contracts before implementation edits.
The shared runner binds actual check results to the request, source, plan, tools, and command.
Completion refuses missing, stale, empty, failed, or inconclusive evidence.
Behavior changes require observed red-before-green tests and affected mutation results.
Refactors require passing tests before and after the change.
Declared risks add proofs, fuzzing, models, fault injection, or memory checks.
Release requires full-scope evidence.
Project gates remain authoritative.
Check client registration against the installed API.

## Alternatives

Skill reminders leave execution optional.
Running every campaign after every edit wastes resources.
Exit status alone cannot establish nonempty verification.

## Consequences

Unsupported producers require an adapter before completion.
Risk classification and a check's semantic adequacy remain review judgments.
The completion boundary amends [release-only heavy verification](run-heavy-verification-at-the-release-gate.md).

## Confirmation

Require rejection tests for absent evidence, source drift, incomplete coverage, interruption, and invalid TDD order.
Require source-bound proofs and an affected mutation selection for the gate.
