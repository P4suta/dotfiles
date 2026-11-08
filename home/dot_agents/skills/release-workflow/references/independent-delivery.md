# Independent release operations

## Choose the release units

Start with the deployed artifacts and the operations that change them.
A Web interface and its server configuration may ship in one Worker, while two native apps have separate store builds and publication states.
A dataset can have its own immutable identity even when a service deployment pins that identity.
Infrastructure resources can need a reviewed configuration plan and state management rather than an app version.
Keep those relationships explicit instead of creating one workflow for each directory or forcing every target into one release.

[The deployment automation guidance](https://dora.dev/capabilities/deployment-automation/) recommends automation, repeatable operations, independently deployable services, and backward compatibility.
The recommendation concerns deployment behavior rather than a required repository layout or one orchestration product.
Use the smallest coordination surface that makes the supported operations discoverable.
Existing commands and workflows can remain the implementation behind that surface.

## Plan verification and publication

Record the requested revision, deployment targets, environments, versions, and dataset identities as validated inputs.
Resolve the revision to an immutable commit before checking or building it.
Inventory direct build inputs, shared generators, toolchains, API and data contracts, and runtime dependencies.
Use that inventory to select verification, and record the reason for each check.
Unknown impact requires investigation or the broader applicable checks rather than a silent omission.

An operator can select the target and timing while the release controller enforces the required checks.
A native optimization can require server contract tests without publishing the server.
A Web presentation change needn't publish an unchanged native app.
A shared API or data change must remain compatible with installed clients, or have an explicit transition plan.
Name the evidence and retirement condition that permit removing support for an older client or data format.
Keep optional checks distinct from publication requirements.

Use input-bound evidence to avoid repeating unchanged work.
Bind the result to the exact files, dependencies, execution assumptions, dataset when read, and candidate identity that the check consumed.
Record a skipped unrelated operation as not requested, and record an unchanged target as unchanged.
Neither status supplies a passing result for a required check.

## Prepare versions without coupling publication

[release-please](https://github.com/googleapis/release-please) automates version updates, changelogs, release pull requests, tags, and GitHub Releases.
Its [manifest mode](https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md) supports independently versioned artifacts and grouped or separate release pull requests.
Its [extra-file updaters](https://github.com/googleapis/release-please/blob/main/docs/customizing.md) can update version fields in extra files.
Use those capabilities for version preparation rather than making the tool the authority for deployment dependencies or production approval.
Configure draft or deferred GitHub publication when required evidence must precede publication.
Per-directory packages can miss shared generators and build inputs at the repository root.
Test the actual pinned proposal tool with shared changes and unrelated presentation changes before relying on its path selection.
In [release-please's proposal lifecycle](https://github.com/googleapis/release-please/blob/v17.11.2/src/manifest.ts), merged proposals with pending release labels can block another proposal until publication bookkeeping completes.
For PR-only preparation, record an explicit committed preparation baseline and use preparation-specific completion labels after validating that the baseline includes the merged proposal.
Keep those labels distinct from tagged or published states, and test successive proposals without creating a release.

For apps, keep the user-visible version separate from the build identity.
[Android versioning](https://developer.android.com/studio/publish/versioning) requires increasing version codes for successive releases and prohibits reusing an uploaded code.
[App Store Connect](https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds/) associates uploads with the bundle identifier and version while the build string identifies a build.
Assign identities through maintained project tooling and preserve each identity across retries.
Don't infer uniqueness from workflow-local counters.

## Connect providers without duplicating release logic

[Reusable GitHub workflows](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows) accept inputs and secrets from a caller.
Use them when existing jobs need a shared entry point, while keeping credential-bearing operations in their protected environments.
A shared entry point needn't run every target, and its summary must expose independently completed, failed, and pending operations.
The official [Cloudflare guidance](https://developers.cloudflare.com/workers/ci-cd/) supports automated Worker deployment.
Cloudflare [watch paths](https://developers.cloudflare.com/workers/ci-cd/builds/build-watch-paths/) can reduce unrelated builds, but shared inputs still belong in the project's impact model.

Use existing provider adapters before adopting another framework.
For store delivery, [fastlane's App Store action](https://docs.fastlane.tools/actions/upload_to_app_store/) supports metadata, binaries, and review submission, and its [Google Play action](https://docs.fastlane.tools/actions/upload_to_play_store/) supports app bundles, metadata, screenshots, and tracks.
Adopt those tools when they remove missing provider work without duplicating an existing verified adapter.
Keep upload, review submission, test distribution, and public rollout as distinct outcomes.
Before retrying, compare the provider's accepted build or deployment receipt with the original candidate and its recorded identity.
A failed command doesn't establish that the provider rejected its request.
Reuse a matching accepted operation, retry a confirmed unaccepted operation under the provider's contract, and resolve an unknown outcome before resubmitting.
Account creation, identity checks, policy declarations, and production approval remain separate owner operations when required.

For infrastructure, inspect the actual resource owner, configuration source, state storage, dependencies, and provider preview before choosing an apply mechanism.
Distinguish configuration shipped in a service artifact from infrastructure resources managed outside that artifact.
Keep a preview free of external mutation, validate the identity and freshness of its reviewed plan before applying, and record changes already completed if a later operation fails.
Don't promise an automatic rollback for an irreversible migration or external action.

## Verify the operator experience

Exercise a Web presentation change, a native optimization, a shared contract change, a configuration change, and a partial publication failure.
Require the selected target's checks, and verify that unrelated publication remains optional.
Check stale evidence, changed source revisions, malformed target inputs, failed prerequisites, duplicate version allocation, and a retry after one target is already public.
Use formal verification for deterministic selection and admission decisions with an explicit link to the production implementation.
Use provider and workflow tests for event permissions, credentials, scheduling, signing, and actual publication behavior.
Mark live operations that haven't run as unverified.
