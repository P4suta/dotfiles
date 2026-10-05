---
name: registry-publishing
description: >-
  Configure or verify npm, PyPI, Hex, and container registry publication with exact candidate identities and supported authentication.
  Use for registry differences in a release, and use rust-release for crates.io.
---

# Registry publication

Use `release-workflow`, `release-evidence`, and `approval-boundaries` for the common contract.
Read the registry's current official documentation and the project's existing package identity, publishing environment, channel, and ownership.
Prefer supported identity-based publishing.
When the registry requires a token, use a narrow owner-managed Doppler credential.
Never invent OpenID Connect support or fall back to a broad static token without saying so.
Build the candidate without publication credentials, then publish only its verified bytes through an explicitly approved stage.

For [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/), verify the supported Node and npm versions, runner, repository address, calling workflow, environment binding, package visibility, and access settings.
Verify that the actual provider and repository generate provenance, because a successful upload proves none.
Keep public prerelease dist-tags apart from the stable channel, and verify the intended tag after an authorized publish.
Treat package scripts and private-dependency installation as credential-bearing code execution, and keep them outside the publication island where possible.

For [PyPI trusted publishing](https://docs.pypi.org/trusted-publishers/), bind the workflow and protected environment, and distinguish TestPyPI from production.
Before publishing, build and inspect the sdist and supported wheels for metadata, license inclusion, platform tags, and a clean consumer installation.
Use supported digital attestations and verify their subject and publisher identity.
A Python package may keep its own build tool, but new surrounding automation belongs in Rust.

For [Hex](https://hex.pm/docs/publish), check package metadata, included files, supported Erlang, Elixir, or Gleam dependencies, and the exact tarball checksum.
When the registry requires key authentication, use a package-scoped key with only the publication permission.
Never suppress registry errors or replace a published version to recover a workflow.

For container images, pin inputs by digest, verify supported platforms, bind provenance and the software bill of materials to the image or index digest, and verify actual registry content.
Publish version and source identifiers before an explicitly authorized mutable convenience tag.
During a retry, check an existing version tag's digest and metadata instead of overwriting it.

Check existing versions first, and distinguish a verified prior upload, a missing target, and an identity mismatch.
After partial success, keep matching immutable versions and resume only missing verified outputs.
A changed archive needs a new authorized version.
Deletion, replacement, and yanking never serve as automatic retries.
