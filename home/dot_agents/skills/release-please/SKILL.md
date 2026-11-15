---
name: release-please
description: >-
  Configure and operate release-please version and changelog proposals, manifest releases, and recovery.
  Use for release-please setup or failures; use release-workflow for candidate verification, signing, and publication authority.
---

# Release-please

Read the repository's release contract, existing configuration, public deliverables, and version sources before choosing a strategy.
Use [upstream configuration and operating guidance](references/configuration.md) for manifest bootstrap, extra files, workflow inputs, and recovery.
Keep repository names, source revisions, secret names, and installed App identities in repository records rather than this skill.

Use `release-workflow` to separate automatic proposals from candidate verification, signing, and publication.
Treat creation of a release PR, merging that PR, creating a draft release, and publishing its assets as distinct operations.
Honor the user's existing authorization for each operation without asking again.
An automatic version proposal grants no release authority.

Select the strategy for the public deliverable rather than the implementation language alone.
Use the manifest to record released versions and a deliberate bootstrap point for an unreleased repository.
Let Conventional Commits drive later versions and changelog entries.
Keep the app version, package metadata, lockfiles, generated version resources, and release tag consistent wherever the product contract requires them to match.
Check the actual updater behavior for workspace inheritance and extra-file paths before relying on it.

Use the existing release-please GitHub App when release PRs must trigger ordinary CI.
Keep its credentials separate from a release-plz App, signing keys, and registry credentials.
Use `doppler` for protected delivery and `github-repository` for environment and repository protections.
Give the proposal job a preparation environment and a short-lived token limited to the destination repository and required permissions.
Keep automatic preparation free of a publication reviewer gate.
Pin actions by verified commit SHA and keep the workflow's default token read-only.

Choose explicitly whether release-please prepares PRs only or also creates tags and draft releases after a release PR merges.
For proposals only, use `true` for the action's `skip-github-release` input.
If it creates a draft, complete verification and asset assembly before final publication.
Keep release PRs outside automatic dependency merging.

Validate configuration with the selected upstream version, inspect the generated PR, and require its current-head checks.
Test version synchronization through the repository's locked build and artifact metadata checks.
Use a separate read-only credential check before enabling an unattended consumer.
Report delivery, provider authentication, proposal creation, draft creation, signing, and publication as separate results.

Resume a failed operation using its original version, source, release ID, and preserved artifact hashes.
Inspect remote state before retrying an uncertain write.
Never rewrite a published tag or replace immutable assets to repair a proposal or failed publication.
