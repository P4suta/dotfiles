---
name: macos-pkg
description: >-
  Build and verify signed, notarized macOS pkg installers with correct payloads, upgrades, receipts, and stapling.
  Use for pkg distribution, not generic app packaging or authorization to publish.
---

# Package distribution for macOS

Read the product's installation locations, privilege needs, supported macOS versions, architectures, and upgrade behavior.
Use `apple-signing`, `apple-notarization`, and `release-evidence` for the surrounding stages.
Sign the final executable with `Developer ID Application` before packaging, and sign the installer with `Developer ID Installer`.
Keep the two identities and their protected credential inputs apart.

Build with `pkgbuild` and `productbuild`, stable package identifiers, and a version that matches the product and release candidate.
Verify payload paths, ownership, permissions, symlinks, licenses, and bundled architectures before signing.
Prefer declarative installation without installer scripts, and use a tested Rust helper when the product needs procedural steps.
Never overwrite unrelated files or require elevated installation without a product reason.

Notarize the final signed package, staple the accepted ticket, and compute the final checksum afterward.
Run `pkgutil --check-signature`, `xcrun stapler validate`, and the native installer assessment on the actual package.
Record the exact package digest and candidate source, and carry the original build provenance through packaging and notarization.

Test clean installation, the installed binary's signature and invocation, version reporting, upgrades from a supported earlier version, and the declared uninstall path.
Inspect installer receipts, and confirm that file ownership and paths match the payload.
Use a controlled native machine through the project's remote-execution workflow when another host must run the test.
Only an install-tested package counts as a deliverable, and creating a package grants no permission to publish.
