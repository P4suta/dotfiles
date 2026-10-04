---
name: macos-pkg
description: >-
  Build and verify signed, notarized macOS pkg installers with correct payloads, upgrades, receipts, and stapling.
  Use for pkg distribution, not generic app packaging or authorization to publish.
---

# macOS pkg Distribution

Read the product's intended installation locations, privilege needs, supported macOS versions, architectures, and upgrade behavior.
Use `apple-signing`, `apple-notarization`, and `release-evidence` for the surrounding guarantees.
Sign the final executable with Developer ID Application before packaging and the installer with Developer ID Installer.
Keep the two identities and their protected credential inputs distinct.

Use supported `pkgbuild` and `productbuild` tooling with stable package identifiers and a version that matches the product and release candidate.
Verify payload paths, ownership, permissions, symlinks, licenses, and bundled architecture contents before signing.
Prefer declarative installation with no installer scripts; use a tested Rust helper when unavoidable product behavior needs procedural handling.
Do not overwrite unrelated files or require elevated installation without a product reason.

Notarize the final signed package, staple the accepted ticket to the supported package, and compute its final checksum afterward.
Use `pkgutil --check-signature`, `xcrun stapler validate`, and the appropriate native installer assessment on the actual package.
Record the exact package digest and candidate source, preserving original build provenance through packaging and notarization.

Test clean installation, the installed binary's signature and useful invocation, version reporting, upgrades from a supported earlier version, and the declared cleanup or uninstall path.
Inspect installer receipts and ensure file ownership and paths match the intended payload.
Use a controlled native machine through the project's remote-execution workflow when another host is required.
A generated or signed package is not an install-tested deliverable, and package creation does not authorize publication.
