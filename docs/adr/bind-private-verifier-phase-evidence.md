---
date: 2026-10-13
status: proposed
---
# Bind private verifier phase evidence

## Context

Build caches avoid repeated compilation but do not establish that a previous verifier result belongs to the current inputs or producer.
The proof campaign also discarded native scratch and container output before a failed attempt could be diagnosed consistently.
An aggregate error formatter alone cannot protect public logs when an inner independent check prints its raw error first.

## Decision

The [phase evidence adapter](../../xtask/src/phase_evidence.rs) gives each measured phase an owned private scope outside candidate Source.
It persists a started record before invocation and retains successful, failed, and incomplete scopes.
Terminal records bind the declared workload, elapsed time, source closure, available commit and tree, producer build identity, and target.
Captured commands bind their configuration digest and write stdout and stderr into the scope while the child runs.
The artifact manifest records labels, output kinds, byte lengths, and digests.
The public report contains an opaque artifact identity and safe phase status.
Active independent checks retain raw errors before the shared aggregate can print them.
Windows creates a private owner SID ACL on each new execution scope.
Unix scopes retain owner-only permissions.

Container phases record the actual inspected image and require the established target semantics.
The Linux amd64 proof and benchmark routes run the inspected immutable image ID with pulling disabled.
Output is retained outside the container before its ordinary removal.
Verifier assertions, inventories, rejecting fixtures, mutation requirements, benchmark flags, and budgets remain authoritative.

The first success reuse implementation covers independently keyed Kani rule subjects and their complete rejecting probes.
Its identity includes subject and probe bytes, conservative transitive engine inputs, locked compiler and dependencies, verifier distribution bytes, flags, and target.
The native engine identity names the thirty-nine Source inputs of its driver, original decoders, obligation parser, process and tool adapters, delegated driver fixture and dependency declarations.
The standalone compiler reads the selected rule module and its generated rejecting module directly.
Unrelated runtime modules and other verifier backends do not enter this identity through a containing directory.
The complete workspace lock and the linked package manifests still bind compiler configuration and shared dependency features.
The required project Cargo config.toml contributes its contents, and the legacy .cargo/config contributes its contents or recorded absence.
Current subject, probe and flag identities remain separate from that shared engine key.

The adapter reads the actual Kani distribution's `rust-toolchain-version` and asks registered Rustup for that exact installed compiler.
The child sets `RUSTUP_AUTO_INSTALL=0` and offers no ambient toolchain fallback or installation.
Missing or malformed selection, failed or unterminated lookup and invalid compiler paths refuse before a reuse decision.
The resolved compiler distribution and its standard libraries contribute their actual bytes through the existing bounded, cycle-aware traversal.
This contribution is distinct from Source pins and from the Kani distribution's own identity.
Native verifier links contribute their target identities and content through a bounded cycle-aware closure traversal.
Verifier and shared engine digests are computed once per Kani campaign.

Reusable anchors are produced only from an actually protected native candidate with matching embedded producer source.
The ledger is outside candidate Source in the owner's managed cache namespace.
Only its protected immutable entries can be considered.
Every reuse reruns the current result decoder over all raw reports and compares exact production and rejecting inventories.
Readers hold the same ledger lock as publishers so a concurrent permission transition cannot be interpreted as an untrusted origin.
A pending attempt prevents an earlier success from covering failed or interrupted work.
The [pure authorization decision](../../crates/rules/src/phase_receipt_rules.rs) distinguishes fresh measurement, verified reuse, and an explicit refusal from producer, origin, and pending-attempt facts.
Its harnesses and rejecting probes cover unauthorized origins, incompatible and incomplete evidence, and attempts to revive earlier success after interruption.
A complete fresh result clears that barrier only after atomic publication and validation.
Malformed, incompatible, incomplete, or untrusted existing anchors are refused before starting expensive measurement.
Old unbound logs have no import path.
Readback reports the original anchor digest and its validated harness durations separately from the current phase's measured readback cost.
Writable local checkouts, Windows, hosted CI, and containers currently measure the affected subjects instead of reusing this ledger.

## Alternatives

A changed file hash alone omits probes, the verifier, shared driver behavior, dependencies, and target assumptions.
An Actions cache or arbitrary JSON digest has no trusted producer and cannot supply verification credit.
Hashing a writable input before and after execution cannot establish the bytes consumed between those observations.
Forwarding raw child output would expose private paths and command material before a later sanitizer runs.
A second signing identity inside isolated Code would add private credential handling to a verifier worker.

## Consequences

The ledger trusts its owner-managed namespace and protected producer constructor.
It excludes deliberate permission overrides or replacement of that namespace by its privileged owner.
Native filesystem enforcement, process status, upstream verifier semantics, and cryptographic digest collision resistance remain explicit external assumptions.
Rustup's native resolution and compiler execution, the operating-system kernel and dynamic libraries outside the captured distributions remain external assumptions.
The explicit Source closure follows the current Kani entry points and must be updated when their actual dependencies change.
Untrusted imported caches and hosted origin logs cannot grant credit.

Kani reuse does not establish generic warm reuse for Miri, fuzz, mutation, measurements, or model checks.
Fresh Lean kernel verification and fixed Linux measurement semantics remain required.
The complete cold and warm campaign, native boundary fixtures, retained-failure privacy checks, and actual publication and readback must provide evidence before issue 165 is complete.
The initial receipt format cannot treat historical execution costs as a newly measured warm result.
