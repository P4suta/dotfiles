---
name: release-evidence
description: >-
  Produce and verify release checksums, build provenance, SBOMs, dependency assurance, and installation evidence for the exact distributed artifacts.
  Use for release completeness and supply chain evidence, not authorization to publish.
---

# Release evidence

Use `release-workflow` for stage boundaries and inspect the actual distribution contract.
Cover every shipped executable, package, architecture, registry archive, installer, and container image that users can download.
Keep a typed candidate manifest with source SHA, version, target, artifact name, digest, builder identity, run and attempt, and required verification outcomes.
An artifact missing from that inventory counts as unverified.

Build from the selected protected source with pinned toolchains and locked dependencies.
Verify version consistency, supported targets, licensing, the declared MSRV or runtime support, and the actual package contents.
Run fitting tests, security and license gates, and consumer installation or upgrade checks.
Keep vulnerability exceptions narrow, reasoned, and visible.
A generated report without an assessed result never passes a gate.
Use `renovate` for version updates instead of a last-minute upgrade during publication.

Generate checksums over final distribution bytes after packaging, signing, and stapling.
Generate a machine-readable SPDX or CycloneDX SBOM from the resolved and shipped dependency set.
Include embedded native libraries and bundled runtimes.
Verify that it parses, names the right product and version, and matches the candidate rather than a later rebuild.
Assess license and vulnerability policy with `cargo-deny`, `cargo-audit`, or another maintained tool that fits the project, including non-Rust components.

Create build provenance with a reviewed pinned attestation action or the repository's verified builder.
Bind the attestation subject to the artifact digest, and check the expected repository, source revision, workflow identity, and event and ref policy.
Verify downloaded bytes with [GitHub's attestation verification](https://docs.github.com/en/actions/concepts/security/artifact-attestations) or the registry's supported verification.
Provenance establishes origin and build claims, not correct or vulnerability-free source.
An SBOM attestation binds an inventory to a subject without proving the inventory accurate.

Signing and packaging can change bytes after the original build.
Keep the original provenance, bind the transformation's inputs and outputs, and attest the final distribution where supported.
Never attribute an old artifact to newer source because a later job finalized it.
Store attestation bundles and required receipts with the release so verification outlives CI artifact retention.
For offline consumers, use the supported offline verification path and current trusted roots.

Check native signatures, notarization, and stapling through `code-signing`, then exercise the installable product on its supported native platform.
When the public promise covers them, check fresh installation, version reporting, a useful invocation, upgrades, and cleanup.
Keep SBOMs, checksums, provenance, release notes, and binaries complete before publishing an immutable GitHub draft.
Verify the published download or registry bytes only after its own authorized publication, and never publish as a verification shortcut.
