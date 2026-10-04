---
name: apple-notarization
description: >-
  Submit, resume, verify, and staple Apple notarization for the exact preserved signed distribution.
  Use for notarization or stapling, not certificate issuance or permission to release.
---

# Apple Notarization and Stapling

Use `apple-signing`, `release-workflow`, and [the shared native procedure](../code-signing/references/apple.md).
Verify the installed `notarytool` and `stapler` contract against [Apple's current workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).
Match a team or individual API key to the actual command's issuer requirements.
The owner manages private keys; the agent receives only protected-command results and public receipts.

Submit the verified signed distribution and persist the submission ID before waiting or ending the runner.
Bind the receipt to the source revision, candidate, build run, signing identity, submitted archive digest, and signed code hashes.
Keep the exact submitted and distributable bytes with their original provenance.
Resume an existing submission instead of rebuilding, signing, or resubmitting merely to poll it.
Represent pending, accepted, rejected, expired, and malformed results distinctly; pending is never release-ready.

After acceptance, inspect the notary log and match its code hashes and issues to every intended architecture.
Staple only a supported distribution container, such as the `.app`, `.dmg`, or `.pkg`; a standalone Mach-O executable or ZIP is not a staple target.
Verify the staple, signature, and appropriate Gatekeeper assessment on the actual deliverable.
If standalone CLI distribution needs offline verification, use a supported notarized container and test the user's installation path.

Keep polling bounded below artifact-retention expiry and preserve diagnostic receipts without credentials.
Use typed Rust state transitions and ownership-based cleanup for orchestration.
Finalize only after the supported native checks pass and publication is separately authorized.
