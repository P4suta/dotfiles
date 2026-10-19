# Authentication

## Keep the credential boundary

Use `gh` to inspect existing workflows, environment protections, configured secret names, and the repository's OIDC subject settings.
Use the approved credential store, or a reviewed fixed-target import job that keeps selected values private in memory.
Keep build and test jobs free of secrets when the repository already isolates signing or publication.
Handle artifact downloads and other untrusted inputs first, and fetch credentials only in the protected job that consumes them.

## Shared release apps

When one GitHub App serves many repositories, store its client ID and private key once in a purpose-specific shared config.
Expose references to them through each repository's existing credential names.
When release-please and release-plz use different GitHub Apps, keep one shared config per App, and reference only the App the repository uses.
Mint installation tokens scoped to the consuming repository and the permissions each step needs.
The shared App key holds authority across all the App's installations, so every repository that receives it shares one trust boundary.
See [GitHub's installation token guide](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app).

## OIDC

Use a separate Doppler service account for each repository and credential purpose.
Grant read-only access to the required project environments, and no workplace-wide role.
Configure its identity with the GitHub provider, the exact audience and subject, and claims that bind the repository ID, permitted workflow, ref, and event.
Avoid broad owner or repository wildcards.
Check the current [platform limits](https://docs.doppler.com/docs/platform-limits) and the existing identity count before adding identities across many projects.

Read the repository's subject configuration:

```bash
gh api repos/OWNER/REPO/actions/oidc/customization/sub
```

Never assume the example `repo:OWNER/REPO` prefix, because GitHub also issues immutable prefixes with numeric owner and repository IDs.
A job with a GitHub environment gets that environment in its subject instead of the branch, so restrict the `ref` claim on its own.
A changed subject format can break other identity integrations, so inspect them first.
When an allowlist blocks the official Doppler action, add only its selected full commit SHA, and keep every existing entry and the pinning rule.
Never widen the allowlist to a wildcard or turn off pinning to start a verification job.

Store the Doppler identity ID in GitHub Actions variables as public configuration.
Grant `id-token: write` only to jobs that authenticate or already need attestations.
Use the official `dopplerhq/secrets-fetch-action`, pinned to a verified full commit SHA, with `auth-method: oidc`, the identity ID, project, and config.
Set `inject-env-vars: false`, and pass each step output only to its consumers.
The action leaves Doppler metadata and values marked `unmasked` visible, and never print values to test masking.

## Fixed tokens and sync

Restrict a read-only service token to one config, and pass it to the official action through its `doppler-token` input.
Store it in the matching protected GitHub environment secret, commonly `DOPPLER_TOKEN`.
Use one token per repository and purpose instead of one owner or personal token for all CI.
Keep its expiry, rotation, and revocation in the project's maintenance procedure.

Native GitHub Secrets sync updates repository or environment secrets centrally and keeps a workflow's `${{ secrets.NAME }}` interface.
Inspect the integration's destination scope and current sync limits before choosing it.
Keep repository-global and environment scopes, existing names, and unrelated fields.
Use a purpose-specific source config and `import_option: none` when Doppler already holds the approved canonical values.
Verify the destination repository ID, environment name, enabled sync, and a successful first sync before calling the consumer migrated.
Choose one method per consumer, and never let a failed OIDC login fall back to broader credentials.

## Import values held only in GitHub

Use a temporary reviewed Rust job when the owner authorizes moving GitHub credentials without opening the vault.
Compile the exact repository ID, ref, Doppler project and config, and selected secret names into the adapter, and accept no runtime target arguments.
Require `workflow_dispatch`, a verified source SHA, the original environment reviewer gate, and a one-hour read/write token limited to the destination config.
Build and test without credentials, then inject only the selected values into the final transfer step.
Capture bounded CLI/API output privately, keep existing nonempty destination fields, and verify exact raw and computed equality without printing values or hashes.
A tag-only environment may need a temporary exact branch policy for the reviewed non-publishing job, so remove only that policy afterward.
Never create a version tag to reach release credentials.
Before merging preparation or cleanup PRs, inspect all automation that pushes to `main` trigger, especially implicit publication and forced tag creation.
Pause only publishing automation, only to honor a no-release instruction, then restore its state after the final cleanup merge without dispatching it.
Revoke the temporary token, delete the migration authorization secret, remove the temporary code through a normal PR, and restore any narrowed action allowlist.
Use native Secrets sync for its GitHub mirrors, and delete old GitHub credential copies only for consumers that now fetch from Doppler.

## Verify the migration

Reject empty outputs and unresolved references before signing or minting publication authority.
Confirm that the intended workflow, ref, and environment fetch their own config, and that other repositories and purposes fail.
Use a non-publishing verification path when one exists, and keep the project's human approval steps.
Credential migration grants no release or tag.
Never turn off immutability, rewrite release tags, or recreate a published release to get past a failure.
Remove old GitHub secrets only after the Doppler path works and the approved store still holds the original values.
Report local checks apart from real CI authentication, signing, and publication evidence.

See the [official action](https://github.com/DopplerHQ/secrets-fetch-action), [OIDC identities](https://docs.doppler.com/docs/service-account-identities), [service tokens](https://docs.doppler.com/docs/service-tokens), and [GitHub Secrets sync](https://docs.doppler.com/docs/github-actions).
