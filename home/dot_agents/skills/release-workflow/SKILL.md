---
name: release-workflow
description: >-
  Design and verify a protected release lifecycle with version proposals, candidate evidence, approval, signing, and immutable publication.
  Use for release workflows and stage boundaries, not routine CI or authority to publish.
---

# Protected release workflow

Read the project's actual workflow, registry bindings, rulesets, environments, and accepted ADRs.
Use `github-repository` for common settings, `doppler` for credential delivery, `rust-release` for crates.io, and `code-signing` for Apple or Windows distribution.
Use `approval-boundaries` for an AI-assisted approval.
Production approval requires strong explicit permission for that operation.

Separate version preparation, candidate verification, signing, registry publication, and final GitHub publication.
A release-please App and a release-plz App hold separate credentials, even when shared across repositories.
Give an automatic proposal job only its own preparation environment and the App permissions it needs.
A version proposal grants no publication authority, and a generic feature merge must never release a version.
Keep release PRs out of automatic dependency-merge rules.

For high-trust release work, prefer a controller defined on the protected default branch with explicit source SHA and version/tag inputs.
Build the chosen revision with read-only repository access and no signing or registry credentials.
Validate that the version, protected release ref, source SHA, workflow identity, candidate manifest, and artifacts refer to the same release.
Bind hashes and provenance to the original artifact bytes and build run, and keep that binding through retries and later finalization jobs.
Parse workflow outputs as untrusted inputs, and refuse malformed, missing, ambiguous, or mismatched identities.

Run credential-bearing signing and publication in separate fresh jobs with purpose-scoped protected environments.
Never run candidate-controlled hooks, local actions, installers, or arbitrary repository code in an isolated signing job.
Download the verified candidate, sign or notarize it, and verify the publisher, timestamp, resulting bytes, and platform-native signature.
Publication receives only the verified output and the capabilities it needs.
Use explicit jobs and environment approvals for independently sensitive stages, and preserve stronger existing gates.
For a sole maintainer, permit self-approval and turn off administrator bypass.
Never add a reviewer to an automatic preparation job because its environment has a release-related name.

Assemble the complete verified asset set in a draft GitHub Release.
Publish only after required candidate checks, signatures, registry results, checksums, and provenance pass.
Keep Immutable Releases on, and protect version tags from update and deletion without bypass.
Never replace published assets, rewrite a published tag, or delete and recreate a release to make a retry pass.
For a partial draft, check the existing identity and asset hashes, reuse matching original bytes, and add only missing verified assets.
Reject and diagnose a mismatched or already published target, and use a newly authorized version for a correction.

Build lifecycle rules into typed Rust repository tooling when no official action provides them.
Use exhaustive state and outcome variants, typed candidate identities, and structured phase events.
Test bindings, denied capabilities, partial failure, retry, and immutable targets before connecting credentials.
Validate credential transport through a dedicated read-only verification workflow that never signs, tags, publishes, or invokes the production release controller.
Audit the repository's Actions allowlist and workflow YAML, add only reviewed pinned actions, and preserve existing restrictions.
Report verified stages accurately and honor the user's current publication boundary.
