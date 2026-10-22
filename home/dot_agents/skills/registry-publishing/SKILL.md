---
name: registry-publishing
description: >-
  Configure or verify npm, PyPI, Hex, and OCI registry publication with exact candidate identities and supported authentication.
  Use for registry differences in a release, and use rust-release for crates.io.
---

# Registry publication

Use `release-workflow`, `release-evidence`, and `approval-boundaries` for the common contract.
Read the registry's current official documentation and the project's existing package identity, publishing environment, channel, and ownership.
Prefer supported identity-based publishing when production access is available and it fits the owner's selected operating model.
Use `doppler` and its credential lifecycle reference for protected inputs, shared references, OIDC delivery, and retirement.
Check whether native identity has replaced an existing static token before preserving it during a migration.
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
For [Hex Workload Identity](https://hex.pm/docs/workload-identity), verify availability for the actual package before planning its registration or asking the owner to operate a settings page.
The provider can gate this feature and return 404 even when its documentation and route exist.
Treat an unavailable page as unresolved availability until the provider confirms its cause.
When the owner selects this available method, bind the existing package to its exact repository, workflow filename, and protected environment.
Check full package ownership, two-factor authentication, and the immutable repository binding before configuring it.
Discover the OIDC audience from the production registry, exchange once per package with a fresh GitHub token, and keep the short-lived package token private.
Use the client's required Bearer token format and stop if the selected identity exchange fails.
Check the pinned client's workload identity support before choosing its authentication path.
Keep a required exchange adapter in maintained tooling, and pass the short-lived token only to its publishing process.
Use clean client state and reject inherited static keys on the identity path.
Public package identities need an existing package and can't publish its first release.
Treat an authorized bootstrap as a separate operation with a narrow expiring key, then retire its stored copies after the identity path works.
Keep staging and production endpoints distinct.

Support an owner-selected Hex token path while native identity remains unavailable or unsuitable.
Use a key restricted to the existing package without broad API or repository permissions, with an owner-maintained lifetime and rotation path.
Keep it in a purpose-scoped Doppler config and deliver it only to the publishing step.
For a preparation-only task, finish the code and input instructions without requiring key issuance or registration.
Leave live credential checks for the owner's pre-release setup and preserve other consumers' verification progress.
For Gleam, verify the pinned client's `HEXPM_API_KEY` input.
Mix uses a different input and token format.
Reject missing or unresolved inputs and keep saved client logins and alternate keys outside the selected path.
The owner enters a required key through the protected store, as described in `doppler`.
Never ask for its value in chat or access 1Password.
Check provider key metadata or a supported read-only operation without claiming that credential retrieval proves publish authority.
Reconsider native identity when production access becomes available, without adding an automatic authentication fallback.

For OCI, pin image inputs by digest, verify supported platforms, and bind provenance and the SBOM to the image or index digest.
Verify actual registry content.
Publish version and source identifiers before an explicitly authorized mutable convenience tag.
During a retry, check an existing version tag's digest and metadata instead of overwriting it.

Check existing versions first, and distinguish a verified prior upload, a missing target, and an identity mismatch.
After partial success, keep matching immutable versions and resume only missing verified outputs.
A changed archive needs a new authorized version.
Never retry by deleting, replacing, or yanking a version, and never suppress registry errors.
