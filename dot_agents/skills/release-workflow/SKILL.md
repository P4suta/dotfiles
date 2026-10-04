---
name: release-workflow
description: >-
  Design and verify a protected release lifecycle with version proposals, candidate evidence, approval, signing, and immutable publication.
  Use for release workflows and stage boundaries, not routine CI or authority to publish.
---

# Protected Release Workflow

Read the project's actual workflow, registry bindings, rulesets, environments, and accepted ADRs.
Choose a lifecycle that gives users verified artifacts and maintainers a reproducible, diagnosable release.
Use `github-repository` for common settings, `doppler` for credential delivery, `rust-release` for crates.io, and `code-signing` for Apple or Windows distribution.
Keep registry and platform differences in those conditional skills rather than duplicating the common lifecycle.
Use `approval-boundaries` for an AI-assisted approval; production approval requires strong explicit permission for that operation.

Separate version preparation, candidate verification, signing, registry publication, and final GitHub publication.
A release-please App and a release-plz App are separate credentials, even when both are shared across repositories.
Give an automatic proposal job only its own preparation environment and necessary App permissions.
Creating a version proposal does not authorize publication, and a generic feature merge must not accidentally release a version.
Keep release PRs out of automatic dependency-merge rules.

Prefer a controller defined on the protected default branch, with explicit source SHA and version/tag inputs, for high-trust release work.
Build the chosen revision with read-only repository access and no signing or registry credentials.
Validate that the version, protected release ref, source SHA, workflow identity, candidate manifest, and artifacts refer to the same release.
Keep hashes and provenance bound to the original artifact bytes and build run, including after a retry or a later finalization job.
Parse workflow outputs as untrusted inputs and refuse malformed, missing, ambiguous, or mismatched identities.

Use separate fresh jobs and purpose-scoped protected environments for credential-bearing signing and publication.
Do not run candidate-controlled hooks, local actions, installers, or arbitrary repository code in an isolated signing job.
Download the verified candidate, sign or notarize it, and verify the publisher, timestamp, resulting bytes, and platform-native signature.
Publication receives only the verified output and the minimum required capabilities.
Use explicit jobs and environment approvals for independently sensitive stages; preserve stronger existing gates.
For a sole maintainer, permit their own approval rather than configuring an approval nobody can give, while disabling administrator bypass.
Do not add a reviewer to an automatic preparation job merely because its environment has a release-related name.

Use a draft GitHub Release while assembling its complete verified asset set.
Publish only after required candidate checks, signatures, registry results, checksums, and provenance are complete.
Keep Immutable Releases enabled and version tags protected against update and deletion without bypass.
Never replace published assets, rewrite a published tag, or delete and recreate a release to make a retry pass.
For a partial draft, check the existing identity and asset hashes, reuse matching original bytes, and add only missing verified assets.
Reject a mismatched or already published target and diagnose it; use a newly authorized version for a correction.

Implement lifecycle rules in typed Rust repository tooling when an official action does not provide them.
Use exhaustive state and outcome variants, typed candidate identities, and structured phase events.
Test bindings, denied capabilities, partial failure, retry, and immutable targets before connecting credentials.
Validate credential transport through a dedicated read-only verification workflow without signing, tagging, publishing, or invoking the production release controller.
Audit the repository's Actions allowlist as well as workflow YAML; add only reviewed pinned actions and preserve existing restrictions.
Report verified stages accurately and honor the user's current publication boundary.
