---
name: apple-notarization
description: >-
  Submit, resume, verify, and staple Apple notarization for the exact preserved signed distribution.
  Use for notarization or stapling, not certificate issuance or permission to release.
---

# Apple notarization and stapling

Use `apple-signing`, `release-workflow`, and [the shared native procedure](../code-signing/references/apple.md).
Check the installed `notarytool` and `stapler` with [Apple's current workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).
The owner holds private keys, and the agent receives only protected-command results and public receipts.

Submit the verified signed distribution and persist the submission ID before waiting or ending the runner.
Bind the receipt to the source revision, candidate, build run, signing identity, submitted archive digest, and signed code hashes.
Resume an existing submission instead of rebuilding, signing, or resubmitting it.
Treat pending, accepted, rejected, expired, and malformed results as distinct states, and never treat pending as release-ready.

After acceptance, match the notary log's code hashes and issues to every intended architecture.
Staple only a `.app`, `.dmg`, or `.pkg`, never a standalone Mach-O executable or ZIP.
Verify the staple, signature, and Gatekeeper assessment on the deliverable.
For offline verification of a standalone CLI, ship it in a notarized container.

Bound polling below artifact-retention expiry, and keep receipts free of credentials.
Write orchestration in Rust with typed state transitions and ownership-based cleanup.
Finish only after the native checks pass and the user authorizes publication.
