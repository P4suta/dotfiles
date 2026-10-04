---
name: code-signing
description: >-
  Configure, troubleshoot, and verify macOS Developer ID signing and notarization and Windows Authenticode with SSL.com eSigner.
  Use for signing credentials, signed release rehearsals, publisher and timestamp verification, and reuse of existing certificates; not Git signing or TLS certificates.
---

# Code Signing

Prefer the user's existing Apple Developer Program membership and SSL.com eSigner contract when available.
A missing local certificate or secret is not a reason to purchase another subscription.
This is a shared personal skill for Claude Code, Codex, and OpenCode.
Use the host's available CLI, API, and browser tools without requiring a particular client plugin.

## Start with the actual release contract

Read the target repository's instructions, release documentation, and workflow before changing credentials or signing steps.
Identify the intended publisher, distribution format, source revision, secret names, and secret environment.
Preserve the project's packaging and publication design.
Do not change distribution formats or publication targets merely to configure signing.

Check public certificate metadata and configured secret names before asking for information already available.
An active contract does not prove that the right certificate has been issued, its private key is available, or enrollment is complete.
Confirm Apple account role and team rather than inferring them from membership alone.
Reuse valid existing certificates and notarization keys before creating replacements.
Ask only for missing non-secret metadata when the next step depends on it.

Read only the references needed for the task:

- [Apple signing and notarization](references/apple.md) covers Developer ID Application, private keys, notarization API keys, and verification.
- [Windows eSigner signing](references/windows.md) covers certificate enrollment, Credential ID, persistent TOTP, and Authenticode verification.

Use `apple-signing`, `apple-notarization`, and `macos-pkg` for their specific artifact transitions and `sslcom-esigner` for Windows.
Use `release-evidence` for the digest, provenance, SBOM, and consumer verification that must follow those transitions.

Verify current official provider documentation before relying on requirements that may have changed.
Keep project-specific secret names, credential references, signing identities, fingerprints, and execution evidence in the project's own approved documentation or reports.
Do not add repository names, local checkout paths, or project state snapshots to this reusable skill.

## Handle credentials through protected inputs

Keep private keys, certificate exports, passwords, and TOTP secrets in 1Password or another user-approved credential store.
Store non-secret identifiers, status, and credential references in approved project records; keep this skill limited to reusable procedures.
The owner manages 1Password directly; do not open the vault, enumerate items, read fields, or inspect secret QR codes.
Prepare named Doppler inputs and let the owner enter the saved values through the protected UI.
Use `doppler` for purpose-scoped CI delivery and confirm readiness through names-only metadata.
Credentials may be consumed inside an authorized signing command without returning their values to the agent.
Never paste secrets into chat, commits, reports, command arguments, or shell history.

For a project that still requires a GitHub secret, have the owner use `gh secret set NAME --repo OWNER/REPO --env ENVIRONMENT` with its hidden prompt.
Validate the complete input before writing any remote secret.
Do not pipe an unverified or potentially empty credential lookup directly into `gh secret set`.
Shell `pipefail` can report a producer failure after an empty value has already been written; it does not prevent that overwrite.
Create sensitive temporary files with private permissions and remove them after the operation.
Confirm secret names with `gh secret list`; GitHub cannot return their saved values.

Keep account login MFA, eSigner certificate TOTP, and enrollment PINs distinct.
Have the owner recover an existing eSigner enrollment secret rather than resetting it.
A reset can invalidate another machine's signing automation.

## Complete the authorized signing work

Follow the user's existing authorization; do not ask again for actions already approved.
Certificate revocation, credential resets, purchases, and public releases require authorization for that actual action.
Setting credentials or running a rehearsal does not authorize publishing a release.

Build the final executable and embed resources and icons before signing.
Any later binary modification invalidates its signature.
Use the repository's non-publishing release rehearsal when one exists.
Explain that a production signing rehearsal uses the service's normal allowance.
Do not create a tag or merge a release pull request merely to exercise signing.

Verify the actual signed executable and packaged artifacts against the intended publisher and source revision.
Require timestamps, successful Apple notarization where applicable, native signature checks, checksums, and provenance checks provided by the release workflow.
A successful compile or signed Git commit does not prove that a distributable binary is signed.
Follow the project's remote-execution instructions when using native machines for verification.

Report the checks that actually passed and the specific remaining dependency.
Do not declare release readiness while signing, notarization, or artifact verification remains untested.

## Explain signing when it becomes necessary

Explain one actionable step at a time, using the account and certificate the user already has.
Binary signing identifies the publisher and detects changes after signing.
Apple notarization records Apple's automated checks; it is separate from the signing certificate.
Windows UAC requests elevation and is separate from signature validity.
Signing can show a verified publisher but does not remove every UAC or SmartScreen prompt.
Checksums detect artifact changes against a trusted expected value, and build provenance connects an artifact to its source and workflow.
Keep these mechanisms separate when answering the user's questions.
