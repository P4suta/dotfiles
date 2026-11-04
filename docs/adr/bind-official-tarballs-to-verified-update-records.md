---
date: 2026-10-30
status: proposed
---

# Bind official tarballs to verified update records

## Status

Proposed.

## Context

The npm manager cannot resolve an official SheetJS CDN tarball as a registry version.
Its unsupported record remains in the Renovate report alongside a successful custom manager lookup of the same declaration.
Rejecting that duplicate prevents the official distribution from reaching the normal freshness decision.
The registry distribution does not provide the selected release.

## Decision

Permit coverage only for the explicit official CDN and upstream Git repository pair in `renovate/tarball.rs`.
Read the regular manifest blob from the exact offered Git revision through the existing source adapter.
Require one unique dependency declaration, one exact replacement member, matching resolved versions, and a successful counterpart lookup without warnings or exclusions.
Keep counterpart updates in the normal freshness gate.
Unknown sources, ambiguous declarations, malformed JSON, and missing evidence remain refusals.

The production scalar decision is checked by Kani with rejecting probes for each missing fact.
Native Git tests bind the adapter to the offered blob despite a changed working tree and index.
The report fuzz target also exercises the canonical tarball parser.
The proofs establish the decision under validated facts, not the truth of upstream tags or the safety of package contents.
Reconsider the source adapter when Renovate changes its extraction semantics or SheetJS changes its official distribution.

## Consequences

The repair preserves strict dependency tracking without downgrading or mirroring the package.
Supporting another distribution requires a reviewed source pair and equivalent adapter evidence.
Arbitrary ignore rules and name-only counterpart matching remain insufficient evidence.
