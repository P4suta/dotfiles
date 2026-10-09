---
status: accepted
date: 2026-10-09
---
# Validate skill catalogs before planning

## Context

A skill plan must reject unknown indices and cycles, include every requested dependency once, and preserve dependency order.
The plan boundary previously combined validation, ordering, filtering, and heap allocation in one call.
The five-node proof needs to cover arbitrary graph edges and ordered, repeated, and invalid requests within the existing proof budget.

## Decision

An opaque `Input` validates the catalog and requests together.
Catalogs of up to eight skills use byte dependency sets and return an opaque `Plan` backed by an array.
Larger catalogs retain the slice algorithm and its vector representation.
Callers read the ordered indices through `Plan::skills`.

The bounded constructor proofs establish the exact dependency and request sets for every catalog size from zero through five.
The planner and in-place proofs establish uniqueness, dependency order, requested coverage, and exact failures over those sets.
Independent list-algorithm equality checks and generated Lean vectors connect the bounded implementation to the general theorem.
The ordinary harnesses follow the coordinator's direction after the [loop-contract pilot](bounded-harness-over-kani-loop-contracts.md).

## Validation

Native exhaustive catalogs also compare the array plan with the independent slice implementation.
The eight- and nine-skill boundary test covers both representations.
The proof gate executes every constructor, planner, equality, and rejecting harness.

## Reconsider

Reassess the representation when catalogs need a different bounded capacity or the verifier changes.
