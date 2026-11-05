# Release-please configuration

Use the [release-please action documentation](https://github.com/googleapis/release-please-action) and [manifest configuration schema](https://github.com/googleapis/release-please/blob/main/schemas/config.json) for the installed action's inputs and configuration.
Resolve the intended upstream release to a commit before pinning it.
The action version and the bundled release-please version have separate release histories.

## Strategy and bootstrap

The [strategy reference](https://github.com/googleapis/release-please/blob/main/docs/customizing.md) defines language strategies and extra-file updaters.
The `simple` strategy maintains `version.txt` and `CHANGELOG.md` and can suit a native app with private implementation packages.
Use a language strategy when its package and dependency update behavior matches the public release contract.
For a Rust monorepo, check the [manifest and cargo-workspace plugin guidance](https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md) before choosing package boundaries.

The manifest stores the last released version for each configured path.
Distinguish that state from an unreleased version already present in source files.
For initial setup, record the intended bootstrap revision and inspect the proposed first version before merging.
Don't edit the manifest to force an ordinary version bump.
Use the documented `Release-As` mechanism when an authorized release needs a deliberate version choice.

## Version synchronization

Use typed JSON, YAML, XML, or TOML extra-file updaters when their supported paths identify the required fields.
Use the generic updater only with its documented annotations.
Inspect [the TOML updater](https://github.com/googleapis/release-please/blob/main/src/updaters/generic-toml.ts) when using JSONPath over workspace or lockfile arrays.
Verify that the selected entries cover owned packages and leave third-party versions unchanged.
An updater warning or unchanged file doesn't establish synchronization.

Workspace inheritance can leave a lockfile or dependent constraint unchanged even when the workspace version changes.
Require a locked dependency resolution and the product's version checks on the generated PR.
If a separate lockfile calculation job is necessary, run candidate code with read-only access.
Publish only its validated inert lockfile from a separate trusted job after checking the exact PR head.
Keep arbitrary branch code away from the App write token.

## Tokens and workflow boundaries

The action documentation explains that PRs created with `GITHUB_TOKEN` don't trigger ordinary workflow events.
Use an existing GitHub App installation token when release PRs need those CI events.
Restrict the installation token to the destination repository and the permissions the selected operations use.
Read actual organization and repository permissions before adding a scope.

The action's `skip-github-release` input permits proposal maintenance without creating release tags.
For draft-producing workflows, inspect `draft` and `force-tag-creation` for the pinned version instead of assuming a draft creates its tag.
The action's release outputs describe a created release, while PR outputs describe a proposal.
Handle missing outputs before parsing JSON.
A conditional step can still parse an expression before its script runs, so don't parse an empty output with `fromJSON`.

Read [GitHub's workflow event rules](https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-when-your-workflow-runs/triggering-a-workflow) and [release immutability](https://docs.github.com/en/repositories/releasing-projects-on-github/about-immutable-releases) when connecting proposals to publication.
Validate the tag, full commit SHA, numeric draft release ID, target commit, candidate manifest, and asset hashes before a publication dispatch.
Use the protected default branch's controller with the selected source as inert input.
Keep the original build and signing provenance when a later run completes an existing draft.

## Recovery

Inspect the existing release PR and release labels before creating another proposal.
Check the configured package path, manifest version, bootstrap history, Conventional Commit parsing, and updater diagnostics when a proposal omits expected changes.
After an uncertain API result, inspect the exact PR or draft before retrying.
Keep an existing submission or draft when its identities and hashes match.
A changed or published target requires a new authorized release rather than replacement assets.
