---
date: 2026-11-06
status: proposed
---

# Verify preserved upstream before reusing findings

## Context

Preserving a dependency for a small repair imports publisher code and comments that never followed the owner's style rules.
Directory exclusions would also hide local changes.

## Decision

Record reviewed archive identities in owner-local configuration.
Verify the archive digest and bounded regular members before treating identical content or existing structural findings as upstream.
Keep new findings and changed comments subject to the normal gates.
Read archives from the same Git snapshot as the surveyed source.

## Alternatives

Reformatting every imported source would obscure the repair.
A directory exemption would leave future authored changes unchecked.
Applying every rule to the complete import would demand unrelated publisher changes.

## Consequences

Contributors keep the original archive and notices.
Owner-local registration attests provenance, without establishing dependency correctness.
Absent, malformed, or changed evidence fails closed.
Reconsider the registration format when another package format requires archive support.

## Confirmation

Tests must reject digest substitution, traversal, links, duplicates, extra occurrences, and new findings.
An unchanged imported comment must remain byte-identical while a newly authored comment still fails its normal check.
Tests check that a populated registry preserves ordinary lint for non-Git projects.
