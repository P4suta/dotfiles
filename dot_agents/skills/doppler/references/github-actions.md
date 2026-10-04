# GitHub Actions authentication

## Preserve the credential boundary

Use `gh` to inspect existing workflows, environment protections, configured secret names, and the repository's actual OIDC subject settings.
GitHub cannot return stored secret values through its API.
Use the approved credential store or a reviewed fixed-target import job that keeps selected values private in memory.
Keep build and test jobs secretless when the repository already isolates signing or publication.
Fetch credentials only in the protected job that consumes them, after artifact downloads and other untrusted inputs have been handled.

## Shared release Apps

When the same GitHub App serves several repositories, store its Client ID and private key once in a purpose-specific shared config and expose references through each repository's existing credential names.
When release-please and release-plz use different GitHub Apps, keep a separate shared config for each App and reference only the App used by the consuming repository.
Keep release-please or release-plz settings, GitHub environments, and Doppler access rules in each consuming repository.
Mint installation tokens scoped to the consuming repository and the permissions required by each step.
The shared App key retains authority across the App's installations, so repositories receiving it share a credential trust boundary.
[GitHub's installation token guide](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app) documents repository and permission restrictions.

## OIDC

Use a separate Doppler Service Account for each repository and credential purpose.
Grant read-only access to the required project environments and no workplace-wide role.
Configure its identity with the GitHub provider, the exact audience and subject, and claims that bind the repository ID, permitted workflow, ref, and event.
Avoid broad owner or repository wildcards.
Check the current [platform limits](https://docs.doppler.com/docs/platform-limits) and existing identity count before scaling repository-purpose identities across many projects.

Read the repository's existing subject configuration:

```bash
gh api repos/OWNER/REPO/actions/oidc/customization/sub
```

Do not assume the example `repo:OWNER/REPO` prefix: GitHub also issues immutable subject prefixes containing numeric owner and repository IDs.
When a job uses a GitHub environment, its subject contains that environment rather than the branch; restrict the `ref` claim separately.
Changing a repository's subject format can affect other identity integrations, so inspect those before changing it.
Inspect the repository's Actions policy before adding an action.
When an allowlist blocks the official Doppler action, add only its selected full commit SHA while preserving every existing entry and the SHA-pinning requirement.
Do not broaden the allowlist to a wildcard or disable pinning to make a verification job start.

Store the Doppler identity ID in GitHub Actions variables; it is public configuration, not a secret.
Give `id-token: write` only to the jobs that authenticate or already require attestations.
Use the official `dopplerhq/secrets-fetch-action`, pinned to a verified full commit SHA, with `auth-method: oidc`, the identity ID, project, and config.
Set `inject-env-vars: false` and pass individual step outputs only to their consumers.
The official action masks ordinary secret values; Doppler metadata and values marked `unmasked` are exceptions.
Never print values to test masking.

## Fixed tokens and sync

A read-only Service Token can be restricted to one config and supplied to the official action through its `doppler-token` input.
Store it in the corresponding protected GitHub environment secret, commonly `DOPPLER_TOKEN`.
Use separate tokens by repository and purpose rather than one owner or personal token across all CI.
Keep its expiry, rotation, and revocation in the project's maintenance procedure.
A Service Account Token is another supported option when its explicitly granted project access is needed.

Native GitHub Secrets sync can centrally update repository or environment secrets while keeping an existing workflow's `${{ secrets.NAME }}` interface.
Inspect the integration's destination scope and current sync limits before choosing that method.
Preserve repository-global and environment scopes, existing names, and unrelated fields.
Use a purpose-specific source config and `import_option: none` when the approved canonical values already exist in Doppler.
Verify the destination repository ID, environment name, enabled sync, and successful initial synchronization before declaring the consumer migrated.
If more than one method is used, choose it explicitly for each consumer; a failed OIDC login should not silently fall back to broader credentials.

## Import existing GitHub-only values

Use a temporary reviewed Rust job when the owner authorizes moving existing GitHub credentials without opening their vault.
Compile the exact repository ID, ref, Doppler project/config, and selected secret names into the adapter; accept no runtime target arguments.
Require `workflow_dispatch`, a verified source SHA, the original environment reviewer gate, and a one-hour read/write token limited to the destination config.
Build and test without credentials, then inject only the selected values into the final transfer step.
Capture bounded CLI/API output privately, preserve existing nonempty destination fields, and verify exact raw and computed equality without printing values or hashes.
A tag-only environment may need a temporary exact branch policy for the reviewed nonpublishing job; retain its tag policies and reviewers and remove only that temporary policy afterward.
Never create a version tag just to gain access to release credentials.
Inspect all main-push automation before merging preparation or cleanup PRs, including implicit publication and forced tag creation.
Pause only publishing automation when necessary to honor a no-release instruction, and restore its prior state after the final cleanup merge without dispatching it.
Revoke the temporary token, delete the migration authorization secret, remove the temporary code through a normal PR, and restore any narrowly changed action allowlist.
Use native Secrets sync for its intended GitHub mirrors; delete superseded GitHub credential copies only for consumers that now fetch directly from Doppler.

## Verify the migration

Validate required outputs for empty values and unresolved references before signing or minting publication authority.
Check that the intended workflow/ref/environment can fetch its own config and that another repository or purpose cannot.
Run the repository's workflow syntax and security checks.
Use a non-publishing verification path when available, and preserve the project's human approval steps.
Credential migration does not authorize creating a release or tag.
Preserve Immutable Releases settings and published release identities.
Never disable immutability, rewrite release tags, or delete and recreate a published release to bypass a migration or verification failure.
Remove old GitHub secrets only after the corresponding path works through Doppler and the original values remain recoverable in the approved store.
Report local validation separately from actual CI authentication, signing, and publication evidence.

[Official Action](https://github.com/DopplerHQ/secrets-fetch-action), [OIDC identities](https://docs.doppler.com/docs/service-account-identities), [Service Tokens](https://docs.doppler.com/docs/service-tokens), and [GitHub Secrets sync](https://docs.doppler.com/docs/github-actions) document the supported alternatives.
