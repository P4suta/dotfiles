---
name: code-signing
description: >-
  Configure, troubleshoot, and verify macOS Developer ID signing and notarization and Windows Authenticode with SSL.com eSigner.
  Use for signing credentials, signed release rehearsals, publisher and timestamp verification, and reuse of existing certificates; not Git signing or TLS certificates.
---

# Code signing

Use the user's existing Apple Developer Program membership and SSL.com eSigner contract.
A missing local certificate or secret never justifies another subscription.

## Start with the release contract

Read the repository's release documentation and workflow before changing credentials or signing steps.
Identify the intended publisher, distribution format, source revision, secret names, and secret environment.
Keep the project's packaging, distribution formats, and publication targets.

Check public certificate metadata and configured secret names before asking for them.
An active contract proves neither an issued certificate, an available private key, nor complete enrollment.
Confirm the Apple account role and team instead of inferring them from membership.
Reuse valid certificates and notarization keys before creating replacements.

Read only the references the task needs:

- [Apple signing and notarization](references/apple.md) covers Developer ID Application certificates, private keys, notarization keys, and verification.
- [Windows eSigner signing](references/windows.md) covers certificate enrollment, the Credential ID, the persistent TOTP secret, and Authenticode verification.

Use `apple-signing`, `apple-notarization`, and `macos-pkg` for Apple artifacts and `sslcom-esigner` for Windows.
Use `release-evidence` for the digests, provenance, SBOM, and consumer verification that follow.
Keep project secret names, credential references, signing identities, fingerprints, and evidence in the project's own records.

## Handle credentials through protected inputs

Keep private keys, certificate exports, passwords, and TOTP secrets in 1Password or another approved credential store.
The owner manages 1Password, so never open the vault, list items, read fields, or inspect secret QR codes.
Prepare named Doppler inputs, and let the owner enter the values through the protected UI.
Use `doppler` for purpose-scoped CI delivery, and confirm readiness through names-only metadata.
Never paste secrets into chat, commits, reports, command arguments, or shell history.

When a project still needs a GitHub secret, have the owner run `gh secret set NAME --repo OWNER/REPO --env ENVIRONMENT` with its hidden prompt.
Never pipe an unverified or empty credential lookup into `gh secret set`, because `pipefail` reports the failure only after the empty value overwrites the secret.
Create sensitive temporary files with private permissions, and remove them afterward.
Confirm secret names with `gh secret list`.

Keep account login MFA, eSigner certificate TOTP, and enrollment PINs distinct.
Have the owner recover an existing eSigner enrollment secret instead of resetting it, because a reset breaks other machines' signing automation.

## Finish the authorized signing work

Certificate revocation, credential resets, purchases, and public releases each need authorization for that action.
Setting credentials or running a rehearsal grants no release.

Build the final executable with its resources and icons before signing, because any later change breaks the signature.
Use the repository's non-publishing release rehearsal when one exists, and tell the user it consumes the service's normal allowance.
Never create a tag or merge a release PR only to exercise signing.

Match the signed executable and packages to the intended publisher and source revision.
Require timestamps, Apple notarization where it applies, native signature checks, checksums, and the release workflow's provenance checks.
Never declare release readiness while signing, notarization, or artifact verification stays untested.
