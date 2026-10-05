---
name: sslcom-esigner
description: >-
  Configure or verify Windows Authenticode signing with an existing SSL.com eSigner certificate, a persistent certificate TOTP seed, and timestamping.
  Use for eSigner specifics, not account login MFA or TLS certificates.
---

# Authenticode signing

Use `code-signing` for the credential and verification contract, `doppler` for CI delivery, and `approval-boundaries` before an AI-assisted signing approval.
Read [the shared Windows procedure](../code-signing/references/windows.md) and the repository's intended publisher and formats.
Reuse the valid existing order and issued certificate instead of buying or resetting credentials to fill a missing local value.
Confirm enrollment, Credential ID, signing capability, and the intended publisher from public metadata.

Prepare named owner-managed inputs for username, password, Credential ID, and the persistent certificate TOTP seed.
Keep login MFA, a rotating one-time code, certificate TOTP, and an enrollment PIN distinct.
The owner handles 1Password and QR codes in person.
Use the service's reviewed pinned action or supported tooling in a fresh credential-bearing job with a verified candidate and no candidate-controlled code execution.
Keep private values out of arguments, logs, and reports.

Check supported signing algorithms and trusted timestamp behavior in [SSL.com's current guide](https://www.ssl.com/guide/esigner-signing-credential-guide/).
Diagnose Java or certificate-chain failures with the supported runtime and trust store, and never bypass TLS verification.
Verify the signed executable and installer with native Windows Authenticode checks and the project's timestamp policy, including publisher identity.
Report credential readiness apart from a completed signing rehearsal, which spends the service's normal signing allowance.
