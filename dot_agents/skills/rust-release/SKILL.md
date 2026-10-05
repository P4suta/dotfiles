---
name: rust-release
description: >-
  Configure or verify Rust version proposals and crates.io publication, including workspaces and trusted publishing.
  Use for Rust release setup or failures, not every Rust edit or permission to publish.
---

# Rust release and crates.io

Use `release-workflow` for the lifecycle and `github-repository` for common GitHub settings.
Before changing the flow, inspect the actual workspace, package publish flags, registry state, release-plz configuration, CI platform, and trusted-publisher binding.
Never copy one project's manual tag template into another project's protected-main controller unexamined.

Keep automatic `release-plz release-pr` apart from credential-bearing publication.
Use the release-plz App from its own shared Doppler config, not the release-please App.
Pin the action and tool version, and keep the project's semver and changelog checks.
Version proposals default to drafts, and merging an ordinary PR must never publish crates or create a release tag.
Start from the [proposal-only configuration](assets/release-plz.toml), and inspect existing per-package overrides.
When changing an automatic release train, move `release` into an explicitly approved publication stage before retiring its old trigger.

Prefer [crates.io trusted publishing](https://crates.io/docs/trusted-publishing) with exact repository, workflow, and environment bindings.
Grant `id-token: write` only to the job that authenticates, and keep no long-lived registry token as a hidden fallback.
First confirm trusted-publisher availability for each package and its registration state.
A first publication may need its own authorized narrow bootstrap credential.
Never run that bootstrap only to test setup.
Keep bootstrap input owner-managed, short-lived, and out of logs and repository state.

Build and verify candidate `.crate` archives before registry authentication.
Check the publishable workspace set, version, dependency order, package contents, locked dependencies, and source identity.
Publish dependent workspace packages in a supported order, and wait for registry propagation before verifying consumers.
Use the native platform for crates that package or verify only there.
Bind provenance and checksums to the exact candidate bytes.
If Cargo repackages during publication, verify that its output still matches the approved candidate.

Registry versions stay immutable.
On a partial retry, compare published versions with the approved package bytes, and resume only unpublished matching candidates.
Never overwrite, republish a version with different bytes, or yank to simplify recovery.
Create or complete the GitHub draft only through the common verified immutable lifecycle.
Keep human-created signed-tag flows apart from App-created tag flows, because their creators and signature rules differ.
Both protect existing version tags from update and deletion without bypass.

Check configuration with the repository's Rust gates and a non-publishing candidate check.
Valid workflow syntax, App authentication, or `cargo publish --dry-run` proves no registry publication.
Preserve actual publication authorization, and report the untested boundary explicitly.
