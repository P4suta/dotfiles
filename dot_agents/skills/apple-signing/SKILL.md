---
name: apple-signing
description: >-
  Configure and verify macOS Developer ID Application signing, native identities, temporary keychains, and hardened-runtime timestamps.
  Use for executable signing; use apple-notarization and macos-pkg for later stages.
---

# Apple Developer ID Signing

Use `code-signing` and [the shared Apple procedure](../code-signing/references/apple.md).
Verify current [Apple Developer ID requirements](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/) and the intended publisher and team.
Reuse an existing valid certificate and private key; membership alone does not prove the correct account role or signing identity is available.
Use Developer ID Application for distributed executables and Developer ID Installer for installer packages.

Select the intended valid identity by its exact public certificate selector rather than its position or friendly name.
A public `.cer` alone cannot sign; an owner-managed protected export must contain the matching private key.
Validate compatibility with the actual native importer, certificate chain, and signing ACLs before claiming readiness.
Keep durable exports in the owner's credential store and consume them only in authorized native signing tooling.

Use a private temporary keychain with explicit ownership and restore the original search list and order exactly.
Protect passwords through native APIs or validated stdin transport, never process arguments or verbose output.
Build final resources before signing, use hardened runtime and a trusted timestamp where required, and validate every distributed architecture.
Keep entitlements minimal and justified by the product's needs.
Use `apple-notarization` after signing and `macos-pkg` for an installer distribution.
Verify actual signed artifacts, record public identity and source bindings, and report cleanup failures as unfinished work.
