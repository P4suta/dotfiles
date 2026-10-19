---
date: 2026-10-14
status: proposed
---
# Partition Cargo artifacts by physical source root

## Context

Alternating captured checkouts through one Cargo target reused a dependency from a different source.
The actual A/B/A regression reported the returning dependency as fresh while its consumer failed because the current symbol was absent.
An installed-tool source stamp and a passing verifier receipt do not establish the identity of every retained compiler artifact.

## Decision

The [target selector](../../crates/platform/src/cargo_target.rs) partitions Code artifacts by the canonical physical source root.
Its key hashes raw operating-system path bytes under a named SHA256 domain.
Missing or nondirectory roots and relative cache bases refuse.

The Code environment retains a stable target within each physical root.
Remote worker bootstrap retains its existing job-owned build target.
Cargo acquisition storage and verified materials remain separate.

Captured source retains its existing immutable protection.
This partition does not detect content replacement with preserved timestamps within one mutable root.
Such replacement requires a separate target or an explicit refresh.

## Alternatives

One shared target permits the reproduced collision.
Cleaning every target discards unrelated artifacts and defeats ordinary warm builds.
The installed-tool build hash omits part of the compiler's all-target input closure and cannot represent that closure.

## Consequences

The [actual Cargo regression](../../crates/platform/tests/cargo_target_partition.rs) preserves source timestamps and checks both correct symbols and fresh artifact events within each root.
The same sequence and native path controls remain required on supported hosts.

Canonical filesystem identity and SHA256 collision resistance are external assumptions.
This partition grants no verifier-result reuse or whole-native readiness.
