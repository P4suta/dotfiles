---
status: proposed
date: 2026-10-12
---
# Bind acquisition plans to pinned inputs and reusable material identities

## Context

An installed tool and a version string don't establish which acquisition bytes can be reused.
The project lockfile also omits material for some backends and declares provenance that needs its own verifier.
[The mise lockfile contract](https://mise.jdx.dev/dev-tools/mise-lock.html) documents those backend limits.
Complete native offline installation requires the broader integration tracked in #161.

## Decision

The maintained `dotfiles artifacts plan`, `acquire`, and `verify` commands share one typed [plan reader](../../crates/dotfiles/src/artifacts/plan.rs).
Its declared scope consists of project mise pins, the pinned mise bootstrap, and the workspace Cargo lockfile.
The command can select named tools and supported targets explicitly.
An ordinary target selection excludes tools declared only for other operating systems before looking up their installation materials.
An explicit incompatible tool selection remains a refusal.
Each artifact carries its target, version, HTTPS source, expected SHA-256, and a digest of the pinned source identity and installation options.
Unsupported backends, missing targets, unsupported checksums, floating requests, mismatched options, and unresolved dependencies produce diagnostics and prevent acquisition credit.
Publisher attestations and cosign declarations require their corresponding verification and remain refused until that support exists.

The plan digest binds the scope, selected inputs, and source file digests.
Per-artifact provenance binds its own reviewed lock entry and request, so an unrelated lockfile edit preserves reuse of identical material.
This provenance records the checksum-pinned source claim that the reader checks.

The [cache boundary](../../crates/dotfiles/src/artifacts/cache.rs) hashes bytes before returning a privately constructed verification capability.
It publishes complete content with atomic no-clobber operations and exposes an opened read-only file to consumers.
Existing corrupt content and corrupt provenance receipts refuse acquisition before a network request.
A missing receipt can be reconstructed from matching pinned metadata and rehashed bytes.
Missing bytes receive at most the decision core's bounded acquisition attempts, and exhaustion retains the first transport failure.
Incomplete downloads stay outside the published object namespace and use the shared cleanup reporter.

The CLI uses the native curl executable with its configuration and URL globbing disabled, HTTPS-only redirects, a connection deadline, an acquisition deadline, and a byte limit.
It checks the release before fetching and refuses a transport older than 8.4.0, because [curl's byte limit contract](https://curl.se/docs/manpage.html#--max-filesize) requires that release to bound unknown-length responses.
Its output describes acquisition coverage and verified material.
Native installation and its stage receipts are governed by the native workflow and the integration in #161.

The Cargo-specific `cargo-plan`, `cargo-acquire`, and `cargo-verify` commands capture the package union of every source manifest and lock, including independent fuzz, benchmark, fixture, and shared-tool workspaces.
They capture both Cargo configuration filenames and refuse source overrides, unresolved locks, conflicting package identities, and external path dependencies.
The source closure and exact package digests bind the resulting material capability.

The [Cargo consumer](../../crates/dotfiles/src/artifacts/consume.rs) copies the verified open archive into private storage and hashes the captured bytes from offset zero before decoding them.
It bounds gzip members, decoded bytes, physical tar headers, extension metadata, paths, and the total prepared source inventory.
It refuses trailing data, links, sparse and special entries, duplicate and case-colliding destinations, supplied directory checksums, and mismatched package identities.
Portable archive member components admit bounded ASCII names while preserving arbitrary file contents.
Generated directory-source checksums bind each extracted file to the original package checksum.

The prepared Cargo configuration selects that index-free directory source with offline resolution.
Every configured command rechecks the captured source and prepared material.
The native code environment retains the prepared materials and their source protection through its child commands.
An unknown ancestor configuration or an alternate prepared-home configuration refuses environment-only consumption.
This readiness covers Cargo package inputs and supplies no compiler, tool installer, native stage, or complete offline-environment result.

## Assurance boundary

The [decision core](../../crates/rules/src/artifact_rules.rs) owns bounded acquisition, transport capability admission, digest and provenance admission, complete nonempty inventory admission, canonical SHA-256 decoding, and supported immutable version grammar.
It also owns explicit target selection and portable archive component admission.
The [proof inventory](../../policy/obligations.toml) requires source-bound Kani harnesses and rejecting examples for those properties.
[Parser fuzzing](../../fuzz/fuzz_targets/artifact_plan.rs) checks the actual reader, valid and refused corpora, scoped identities, and reporting round trips.
The [material replay](../../fuzz/fuzz_targets/artifact_materials.rs) exercises the source closure and the same archive consumer with a separate finite budget.
Its maintained corpus generator supplies positive material and source cases and rejecting archive and configuration cases.

Cryptographic hashing, TOML and URL semantics, filesystem operations, executable behavior, and transport cross the pure verifier boundary.
The implementation relies on the locked libraries, native filesystem contracts, and curl's documented transport limits at those boundaries.
Native tests exercise poisoning, provenance tampering, incomplete publication recovery, concurrent publication, and retry exhaustion.
Fresh installation, offline backend closure, publisher provenance verification, and live-access assurance require their own integration evidence before #161 can complete.
Native fixtures exercise a fresh Cargo installation and binary execution without a registry index and refuse missing materials before native preparation or the code gate.
The complete native-profile material locks, compiler components, tool dependency graphs, publisher verification, and OCI build materials still need their own consumed-byte integration.

## Alternatives

Installed-tool reuse doesn't establish acquisition availability for a fresh destination.
A container alone doesn't establish native installation behavior.
Trusting a sidecar receipt without checking the bytes would admit a poisoned cache.
Binding each artifact to the whole lockfile would invalidate reusable content when an unrelated entry changes.

## Consequences

The acquisition commands can explain missing material before an expensive verification campaign is scheduled.
The supported project scope can reuse verified content across candidates without sharing installation state.
An unresolved plan exits unsuccessfully and supplies no verified acquisition capability.
The broader native material closure can adopt this boundary incrementally while retaining its required installation and live-access checks.
