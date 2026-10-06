---
name: registry-publishing
description: >-
  Configure or verify npm, PyPI, Hex, and OCI registry publication with exact candidate identities and supported authentication.
  Use for registry differences in a release, and use rust-release for crates.io.
---

# Registry publication

Use `release-workflow`, `release-evidence`, and `approval-boundaries` for the common contract.
Read the registry's current official documentation and the project's existing package identity, publishing environment, channel, and ownership.
Prefer supported identity-based publishing.
When the registry requires a token, use a narrow owner-managed Doppler credential.
Never invent OIDC support or fall back to a broad static token without saying so.
Build the candidate without publication credentials, then publish only its verified bytes through an explicitly approved stage.

For [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/), verify the supported Node and npm versions, runner, repository URL, calling workflow, environment binding, package visibility, and access settings.
A successful upload proves no provenance, so verify that the actual provider and repository generate it.
Keep public prerelease dist-tags apart from the stable channel, and verify the intended tag after an authorized publish.
Package scripts and private-dependency installation execute code with credentials, so keep them outside the publication job where possible.

For [PyPI trusted publishing](https://docs.pypi.org/trusted-publishers/), bind the workflow and protected environment, and distinguish TestPyPI from production.
Before publishing, build the sdist and supported wheels and inspect metadata, license inclusion, platform tags, and a clean consumer installation.
Use supported digital attestations and verify their subject and publisher identity.
A Python package may keep its own build tool, but new surrounding automation belongs in Rust.

For [Hex](https://hex.pm/docs/publish), check package metadata, included files, supported Erlang, Elixir, or Gleam dependencies, and the exact tarball checksum.
When the registry requires key authentication, use a package-scoped key with only the publication permission.

For OCI, pin image inputs by digest, verify supported platforms, and bind provenance and the SBOM to the image or index digest.
Verify actual registry content.
Publish version and source identifiers before an explicitly authorized mutable convenience tag.
During a retry, check an existing version tag's digest and metadata instead of overwriting it.

Check existing versions first, and distinguish a verified prior upload, a missing target, and an identity mismatch.
After partial success, keep matching immutable versions and resume only missing verified outputs.
A changed archive needs a new authorized version.
Never retry by deleting, replacing, or yanking a version, and never suppress registry errors.
