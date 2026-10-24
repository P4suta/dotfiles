# Credential lifecycle

## Inventory and classify

Read project and config metadata, secret names, GitHub consumers, environment protections, OIDC subjects, and machine-local selections.
Read no secret values for inventory or readiness.
Record what each consumer needs and whether it uses native workload identity, an active stored credential, a retired credential, or an unresolved input.
Check production feature availability for the actual account and package before treating native identity as a migration prerequisite.
Documentation and implementation source establish a supported design, while an available registration path and matching provider configuration establish readiness.
Record an owner's explicit token choice as the active design, with permission, lifetime, rotation, and delivery requirements.
Investigate retirement when the owner reports revoking a key.
Keep private values, tokens, local paths, and machine identities out of shared skills and tracked reports.

## Organize by purpose

Keep each shared credential in one canonical config and use explicit field references in consuming projects.
Separate release Apps, Windows signing, macOS signing, notarization completion, and local automation.
Keep different Apps and publisher identities distinct.
Import only fields the job needs instead of inheriting a whole credential collection.
Use stable purpose names and explain any required legacy field name at its consumer.
Keep public identity selectors, expected publishers, and fingerprints with the project's configuration.

One repository and purpose gets one read-only service account with no workplace role.
Bind OIDC to immutable repository and owner IDs, permitted workflows, refs, events, and the actual environment subject.
Read the existing subject format instead of changing it to fit a template.
Keep workflow identity IDs in public GitHub variables.
A workload can authenticate as every identity whose claims it satisfies.
Use distinct protected subjects or reusable workflow identities when two stages must deny each other's credentials.
Distinct account names with matching claim policies provide no such workload isolation.
Fail when capacity or scope doesn't fit.
Keep each identity within its intended scope.

Secret-store isolation limits who can retrieve a credential.
A shared App key or signing credential still carries the authority of that shared identity after retrieval.
Scope minted App tokens to the current repository and step permissions, and state this remaining trust boundary.

## Separate effects and evidence

| Stage | Evidence | Effects requiring their own task scope |
| --- | --- | --- |
| Inventory | Names, references declared by consumers, access, and delivery metadata | None |
| Authentication | Authorized protected fetch or scoped read-only authentication | Token exchange or credential retrieval |
| Signing | Verified signature on exact candidate bytes | Signing service use |
| Notarization | Bound submission and accepted receipt | Upload and service requests |
| Stapling | Valid ticket and native assessment on preserved artifacts | Local artifact change |
| Publication | Registry or release content matches the approved candidate | Public writes |

A config named `release` or approval to fetch credentials authorizes no signing, tag, registry upload, or release.
Keep authentication verification in a manual workflow with no publishing or signing steps and no downstream publication trigger.
Keep existing environment protection, and name the workflow and requested operation before any approval.
Leave a human gate pending when the owner hasn't delegated that exact approval.
Distinguish metadata, authentication, signing, notarization, and publication results.
Fetching a nonempty token establishes delivery.
Provider permissions, lifetime, and revocation need their own evidence.
Use a read-only provider check when it supports the token's narrow permissions, and state what the check leaves unverified.

## Migrate and retire

Default a maintained migration helper to a metadata-only plan.
Before writes, check exact workplace and repository identities, target scope, conflicts, plan freshness, and remaining identity capacity.
Retrieve values only during an authorized transfer or consuming check, keep them private in memory, and reject conflicting destination values.
Prepare the new path while the old consumer remains recoverable.
Verify a successful intended fetch and denial of another purpose or repository before retiring its old delivery path.
Confirm an explicit access-denied response for the negative check.
Keep the tested identity fixed when verifying its config scope, and assess other eligible identities as another boundary.
Network, authentication, and runner failures prove no permission denial.
A healthy secret mirror proves no workflow cutover.

Stop duplicate writers after verifying destination coverage.
After consumer cutover, disconnect its sync and remove only GitHub credential copies owned by that delivery path.
Preserve unrelated fields and public configuration.
Before removing an empty config or demonstration project, check references, inheritance, syncs, grants, fixed tokens, and local selections.
Treat an unknown dependency as unresolved instead of unused.
Read back each mutation, record partial completion without values, and make reruns preserve verified resources.
Keep account deletion separate from data and consumer cutover.

For credential replacement, prepare the new protected input, update its consumers, verify the authorized boundary, and retire the old input.
Revocation, enrollment resets, and replacement issuance need the owner's scope for that operation.
Never make a broad credential or cached value an automatic recovery path.

Use `rust-release` and `registry-publishing` for registry identity and bootstrap requirements.
Use `code-signing`, `sslcom-esigner`, `apple-signing`, `apple-notarization`, and `macos-pkg` for the native signing and artifact boundaries.
