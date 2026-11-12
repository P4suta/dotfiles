---
name: release-workflow
description: >-
  Design independently selectable release operations with version proposals, candidate evidence, approval, signing, and immutable publication.
  Use for release workflows and stage boundaries, not routine CI or authority to publish.
---

# Protected release workflow

Read the project's actual workflow, registry bindings, rulesets, environments, and accepted ADRs.
Inventory the artifacts, configuration, deployed resources, consumers, and installed clients before choosing release units.
One repository, directory, product name, or shared version doesn't require one publication operation.
Code and configuration that ship in the same artifact belong to the same release unit.
Use [independent delivery](references/independent-delivery.md) for apps, services, datasets, and infrastructure with independent releases.
Use `github-repository` for common settings, `doppler` for credential delivery, `rust-release` for crates.io, and `code-signing` for Apple or Windows distribution.
Use `approval-boundaries` for an approval.
Use `release-please` for its manifest configuration, version proposals, and recovery.
Production approval requires strong explicit permission for that operation.

Separate version preparation, candidate verification, signing, registry publication, and final GitHub publication.
Keep required verification separate from the operator's choice of publication target and timing.
Derive affected checks from actual inputs, shared contracts, and runtime dependencies, and require them for the selected candidate.
Use path filters to limit executions and input-bound evidence to establish which dependencies changed.
Reuse passing evidence for unchanged declared inputs and assumptions, and refuse missing or stale evidence.
Let an operator request extra targets while keeping required checks mandatory.
Keep unrelated release units independently deployable, and preserve compatibility with consumers that haven't upgraded.
Use an explicit migration plan when an incompatible change requires ordered operations.

Use version-proposal tools for artifacts that need versioned releases and changelogs.
Include shared generators and root build inputs when determining each artifact's version changes.
For preparation without publication, maintain an explicit preparation baseline and reconcile merged proposals without claiming a tag or release exists.
Keep app display versions separate from store build identities, and assign build identities without reuse.
Group version proposals only when the release cadence benefits from grouping.
Keep Web deployment and infrastructure changes available on demand without requiring an unrelated app release.
A release-please App and a release-plz App hold separate credentials, even when shared across repositories.
Give an automatic proposal job only its own preparation environment and the App permissions it needs.
A version proposal grants no publication authority, and a generic feature merge must never release a version.
Keep release PRs out of automatic dependency-merge rules.

For high-trust release work, prefer a controller defined on the protected default branch with explicit source SHA and version/tag inputs.
Build the chosen revision with read-only repository access and no signing or registry credentials.
Validate that the source SHA, workflow identity, candidate manifest, artifacts, and any versioned release ref refer to the same release.
Bind hashes and provenance to the original artifact bytes and build run, and keep that binding through retries and later finalization jobs.
Parse workflow outputs as untrusted inputs, and refuse malformed, missing, ambiguous, or mismatched identities.

Run credential-bearing signing and publication in separate fresh jobs with purpose-scoped protected environments.
Never run candidate-controlled hooks, local actions, installers, or arbitrary repository code in an isolated signing job.
Download the verified candidate, sign or notarize it, and verify the publisher, timestamp, resulting bytes, and platform-native signature.
Publication receives only the verified output and the capabilities it needs.
Use explicit jobs and environment approvals for independently sensitive stages, and preserve stronger existing gates.
For a sole maintainer, permit self-approval and turn off administrator bypass.
Never add a reviewer to an automatic preparation job because its environment has a release-related name.

For a target distributed through GitHub Releases, assemble the complete verified asset set in a draft release.
Publish only after required candidate checks, signatures, registry results, checksums, and provenance pass.
For versioned GitHub publication, keep Immutable Releases on and protect version tags from update and deletion without bypass.
Never replace published assets, rewrite a published tag, or delete and recreate a release to make a retry pass.
For a partial draft, check the existing identity and asset hashes, reuse matching original bytes, and add only missing verified assets.
Reject and diagnose a mismatched or already published target, and use a newly authorized version for a correction.

Reuse maintained provider tools and verified adapters.
Build missing repository-specific lifecycle rules into typed Rust tooling.
Use exhaustive state and outcome variants, typed candidate identities, and structured phase events.
Offer a read-only plan that names the source, requested operations, required checks, dependencies, and intended external effects.
Report every operation's outcome with explicit states for partial completion and unchanged targets.
Keep retries bound to the original candidate and reuse matching completed operations instead of repeating publication.
Reconcile provider receipts before retrying an external operation whose response failed or disappeared.
Distinguish accepted, rejected, and unknown provider outcomes, and stop an unresolved ambiguous write before resubmitting it.
Test bindings, denied capabilities, partial failure, retry, and immutable targets before connecting credentials.
Validate credential transport through a dedicated read-only verification workflow that never signs, tags, publishes, or invokes the production release controller.
Audit the repository's Actions allowlist and workflow YAML, add only reviewed pinned actions, and preserve existing restrictions.
Report verified stages accurately and honor the user's current publication boundary.
