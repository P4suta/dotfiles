---
name: apple-signing
description: >-
  Configure and verify macOS Developer ID Application signing, native identities, temporary keychains, and hardened-runtime timestamps.
  Use for executable signing; use apple-notarization and macos-pkg for later stages.
---

# Signing with Developer ID

Use `code-signing` and [the shared Apple procedure](../code-signing/references/apple.md).
Check current [Apple Developer ID requirements](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/) and the intended publisher and team.
Reuse an existing valid certificate and private key.
Sign executables with a Developer ID Application certificate and installer packages with a Developer ID Installer certificate.

Select the identity by its exact public certificate selector instead of its position or friendly name.
A public `.cer` can't sign, so the owner's protected export must contain the matching private key.
Validate the export with the native importer, certificate chain, and signing ACLs before claiming readiness.
Keep durable exports in the owner's credential store.

Use a private temporary keychain, and restore the original keychain search list and order exactly.
Pass passwords through native APIs or checked stdin instead of process arguments or verbose output.
Build final resources before signing, use hardened runtime and a trusted timestamp, and verify every distributed architecture.
Grant only the entitlements the product needs.
Use `apple-notarization` after signing and `macos-pkg` for an installer.
Report cleanup failures as unfinished work.
