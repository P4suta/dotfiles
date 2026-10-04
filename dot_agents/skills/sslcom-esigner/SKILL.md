---
name: sslcom-esigner
description: >-
  Configure or verify Windows Authenticode signing with an existing SSL.com eSigner certificate, persistent certificate TOTP, and timestamping.
  Use for eSigner specifics, not account login MFA or TLS certificates.
---

# SSL.com eSigner

Use `code-signing` for the credential and verification contract, `doppler` for CI delivery, and `approval-boundaries` before an AI-assisted signing approval.
Read [the shared Windows procedure](../code-signing/references/windows.md) and the repository's intended publisher and formats.
Reuse the valid existing order and issued certificate rather than purchasing or resetting credentials to solve a missing local value.
Confirm enrollment, Credential ID, signing capability, and the intended publisher from public metadata.

Prepare named owner-managed inputs for username, password, Credential ID, and the persistent certificate TOTP seed.
Keep login MFA, a rotating one-time code, certificate TOTP, and an enrollment PIN distinct.
The owner handles 1Password and QR codes directly.
Use the service's reviewed pinned action or supported tooling in a fresh credential-bearing job with a verified candidate and no candidate-controlled code execution.
Keep private values out of arguments, logs, and reports.

Verify supported signing algorithms and trusted timestamp behavior against [SSL.com's current guide](https://www.ssl.com/guide/esigner-signing-credential-guide/).
Diagnose Java or certificate-chain failures using the supported runtime and trust store; do not bypass TLS verification.
Verify the actual signed executable and installer with native Windows Authenticode checks and the project's timestamp policy.
Check publisher identity and treat warning or nonzero verification outcomes according to the actual native tool contract.
Report credential readiness separately from a completed signing rehearsal, which consumes the service's normal signing allowance.
