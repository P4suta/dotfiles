---
name: rust-release
description: >-
  Configure or verify Rust version proposals and crates.io publication, including workspaces and trusted publishing.
  Use for Rust release setup or failures, not every Rust edit or permission to publish.
---

# Rust Release and crates.io

Use `release-workflow` for the lifecycle and `github-repository` for common GitHub settings.
Inspect the actual workspace, package publish flags, registry state, release-plz configuration, CI platform, and trusted-publisher binding before changing the flow.
Do not apply one project's manual tag template to another project's protected-main controller blindly.

Keep automatic `release-plz release-pr` separate from credential-bearing publication.
Use the release-plz App from its own shared Doppler config, not the release-please App.
Pin the action and tool version and retain the project's semver and changelog checks.
Draft version proposals are the default; merging an ordinary PR must not publish crates or create a release tag.
The [proposal-only configuration](assets/release-plz.toml) is a starting point, not a substitute for inspecting existing per-package overrides.
When changing an automatic release train, move `release` into an explicitly approved publication stage before retiring its old trigger.

Prefer [crates.io trusted publishing](https://crates.io/docs/trusted-publishing) with exact repository, workflow, and environment bindings.
Grant `id-token: write` only to the job that authenticates; do not retain a long-lived registry token as a silent fallback.
Confirm trusted-publisher availability for each package and registration state first.
A first publication may need a separately authorized narrowly scoped bootstrap credential; never run that bootstrap merely to test setup.
Keep bootstrap input owner-managed, short-lived, and excluded from logs and repository state.

Build and verify candidate `.crate` archives before registry authentication.
Check the publishable workspace set, version, dependency order, package contents, locked dependencies, and source identity.
Publish dependent workspace packages in a supported order and allow registry propagation before verifying consumers.
Use the native supported platform for crates that cannot package or verify on another OS.
Bind provenance and checksums to the exact candidate bytes.
If Cargo repackages during publication, verify that the bytes it produces still match the approved candidate.

An existing registry version is immutable.
On a partial retry, verify already published versions against the approved package bytes and resume only genuinely unpublished matching candidates.
Never overwrite, republish under the same version with different bytes, or yank merely to simplify recovery.
Create or finalize the GitHub draft only through the common verified immutable lifecycle.
Keep human-created signed-tag flows and App-created tag flows distinct: their creation actors and signature requirements differ.
Both protect existing version tags against update and deletion without bypass.

Use the repository's Rust gates and a non-publishing candidate check to validate configuration.
Successful workflow syntax, App authentication, or `cargo publish --dry-run` does not establish a successful registry publication.
Preserve actual publication authorization and report the untested boundary explicitly.
