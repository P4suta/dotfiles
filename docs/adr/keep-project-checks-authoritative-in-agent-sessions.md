---
date: 2026-11-06
status: proposed
---
# Keep project checks authoritative in agent sessions

## Context

Projects own their tests, proofs, and release gates.
The [request contract proposal](require-verification-results-before-implementation-completion.md) duplicates their configuration, misclassifies test sources, and couples independent checkouts through a session lock.

## Decision

Agent hooks enforce issue scope and reply prose without verification contracts.
Keep shared ADR content checks independent of the optional runner and catalog inspection.
Use project commands and report actual results and unresolved boundaries.
Keep the explicit runner and its locks for requested execution receipts.

## Alternatives

Extending source classifiers retains duplicate project knowledge.
Shorter lock holds reduce contention but preserve mandatory parallel plans.
Removing project checks discards required evidence.

## Consequences

Scope and test adequacy remain review judgments.
Project and publication gates remain authoritative.
Reconsider automatic integration when projects supply a verification interface requiring no parallel plan.

## Confirmation

Exercise missing plans and held locks while retaining issue-scope, ADR content, and reply prose refusals.
Acceptance of the reviewed choice remains pending.
