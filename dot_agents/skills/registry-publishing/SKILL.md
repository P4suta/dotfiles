---
name: registry-publishing
description: >-
  Configure or verify npm, PyPI, Hex, and OCI registry publication with exact candidate identities and supported authentication.
  Use for registry differences in a release; use rust-release for crates.io.
---

# Registry Publication

Use `release-workflow`, `release-evidence`, and `approval-boundaries` for the common contract.
Read the registry's current official documentation and the project's existing package identity, publishing environment, channel, and ownership.
Prefer supported identity-based publishing; use a narrowly scoped owner-managed Doppler credential when the registry requires a token.
Do not invent OIDC support or silently fall back to a broad static token.
Build the candidate without publication credentials, then publish only its verified bytes through an explicitly approved stage.

For [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/), verify the supported Node/npm versions, runner, repository URL, calling workflow, environment binding, package visibility, and access settings.
Verify whether provenance is generated for the actual provider and repository; do not infer it from a successful upload.
Keep public prerelease dist-tags separate from the stable channel and verify the intended tag after an authorized publish.
Treat package scripts and private-dependency installation as credential-bearing code execution and keep them outside the publication island where possible.

For [PyPI trusted publishing](https://docs.pypi.org/trusted-publishers/), bind the workflow and protected environment, and distinguish TestPyPI from production.
Build and inspect the sdist and supported wheels before publishing, checking metadata, license inclusion, platform tags, and a clean consumer installation.
Use supported digital attestations and verify their subject and publisher identity.
A Python package's own build backend can remain its supported tool; use Rust for new surrounding automation rather than ad hoc Python scripts.

For [Hex](https://hex.pm/docs/publish), check package metadata, included files, supported Erlang/Elixir or Gleam dependencies, and the exact tarball checksum.
Use a package-scoped key with only the supported publication permission when key authentication is required.
Do not suppress registry errors or replace a published version to recover a workflow.

For OCI, pin image inputs by digest, verify supported platforms, bind provenance and the SBOM to the image or index digest, and verify actual registry content.
Publish version and source identifiers before an explicitly authorized mutable convenience tag.
Validate an existing version tag's digest and metadata instead of overwriting it during a retry.

Check existing versions first and distinguish a verified prior upload, a missing target, and an identity mismatch.
On partial success, retain matching immutable versions and resume only missing verified outputs.
A changed archive needs a new authorized version; deletion, replacement, or yanking is not an automatic retry mechanism.
