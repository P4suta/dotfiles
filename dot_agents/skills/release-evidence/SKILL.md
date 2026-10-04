---
name: release-evidence
description: >-
  Produce and verify release checksums, build provenance, SBOMs, dependency assurance, and installation evidence for the exact distributed artifacts.
  Use for release completeness and supply-chain evidence, not authorization to publish.
---

# Release Evidence

Use `release-workflow` for stage boundaries and inspect the actual distribution contract.
Cover every shipped executable, package, architecture, registry archive, installer, and container image that users can obtain.
Keep an explicit typed candidate manifest with source SHA, version, target, artifact name, digest, builder identity, run and attempt, and required verification outcomes.
An artifact absent from that inventory is not implicitly verified.

Build from the selected protected source with pinned toolchains and locked dependencies.
Verify version consistency, supported targets, licensing, declared MSRV or runtime support, and the actual package contents.
Run appropriate tests, security and license gates, and consumer installation or upgrade checks.
Keep vulnerability exceptions narrow, reasoned, and visible; a generated report without an assessed result is not a passing gate.
Use `renovate` for maintained version updates, not an unreviewed last-minute upgrade during publication.

Generate checksums over final distribution bytes after packaging, signing, and stapling.
Generate a machine-readable SPDX or CycloneDX SBOM from the actual resolved and shipped dependency set.
Account for embedded native libraries and bundled runtimes as well as language packages.
Verify the SBOM parses, names the right product and version, and matches the candidate rather than a later rebuild.
Use `cargo-deny`, `cargo-audit`, or other project-appropriate maintained tools to assess license and vulnerability policy, including the relevant non-Rust components.

Create build provenance with a reviewed pinned attestation action or the repository's verified builder.
Bind the attestation subject to the artifact digest and check the expected repository, source revision, workflow identity, event and ref policy.
Use [GitHub's attestation verification](https://docs.github.com/en/actions/concepts/security/artifact-attestations) or the registry's supported verification against actual downloaded bytes.
Provenance establishes origin and build claims; it does not establish that the source is correct or vulnerability-free.
An SBOM attestation binds an inventory to a subject; it does not replace inventory accuracy or vulnerability assessment.

Signing and packaging can change bytes after the original build.
Keep the original provenance and bind the transformation's inputs and outputs explicitly; attest the final distribution where supported.
Do not attribute an old artifact to newer source code merely because a later job finalized it.
Preserve attestation bundles and required receipts with the release so verification survives CI artifact retention.
Use the supported offline verification path and current trusted roots when offline consumers are part of the distribution contract.

Check native signatures, notarization and stapling through `code-signing`, then exercise the installable product on its supported native platform.
Check fresh installation, version reporting, an actual useful invocation, upgrades, and cleanup when they are part of the public promise.
Keep SBOMs, checksums, provenance, release notes, and binaries complete before publishing an immutable GitHub draft.
Verify the published download or registry bytes only after a separately authorized publication; never publish as a verification shortcut.
