---
date: 2026-10-19
status: proposed
---

# Bind inherited Renovate Cargo records to committed workspace metadata

## Context

Renovate 44.52.0 represents a workspace reference with `inherited-dependency` and an empty `currentValue`.
The member reference has no independent lookup, while its workspace declaration supplies the version and update candidates.
Ignoring the marker would lose freshness attribution when a member introduces or changes an inherited reference.

## Decision

Run `cargo metadata --locked --offline --no-deps --format-version 1` against isolated snapshots of the offered commit and the checked baseline.
Decode dependency declarations with the maintained TOML parser and metadata with typed serde contracts.
Require an actual workspace member, one dependency in its kind and target context, and one matching workspace provider in the Renovate report.
Match the dependency key, canonical package, registry, manifest, and workspace lockfile owner.
Validate the provider through the existing report completeness and update policy.
Reject inherited records with independent selected versions, updates, warnings, or inconsistent binding evidence.

Compare the member declaration and Cargo dependency semantics across the two snapshots.
A new reference or changed binding touches the verified provider even when its manifest is unchanged.
Comments leave both semantic values unchanged.
Metadata remains in process memory, and the temporary Git index lives outside the source snapshot, so committed output filenames cannot replace evidence or source files.

The Cargo adapter trusts the installed Cargo executable and its documented metadata format.
Cargo determines workspace membership and inheritance; TOML decoding establishes the declaration that the report claims to describe.
Neither regular expressions nor a JQ projection decide inheritance.
The previous report migration removed the maintained JQ queries, so this contract belongs to the existing Rust validator.

## Consequences

Inherited lookups need Cargo locally and fail closed when either committed snapshot cannot produce consistent metadata.
The adapter does not execute build scripts or fetch dependencies.
Native integration tests use local patched registry packages to exercise actual Cargo without relying on a network or preloaded registry cache.
These tests cover the external Cargo and parser boundary; they are finite regression evidence rather than a formal proof.
The existing pure freshness decision remains unchanged.
Broader proof infrastructure and the complete native platform campaign remain deferred under the owner's current scope.

## Alternatives

Accepting every inherited marker would admit missing providers and hide newly introduced outdated dependencies.
Comparing changed lines alone cannot distinguish a comment from a changed inherited feature or resolve renamed packages and workspace ownership.
Reimplementing Cargo's workspace discovery would duplicate its semantics and weaken the binding to actual committed manifests.
