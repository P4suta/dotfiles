# Apple signing and notarization

## Reuse or create the signing identity

Sign a standalone macOS CLI distributed outside the App Store with a Developer ID Application certificate.
A Developer ID Installer certificate signs installer packages, not executables.

Inspect public identity metadata:

```sh
security find-identity -v -p codesigning
```

Match the intended publisher and team, check validity, and confirm the private key.
For automated signing, pass the 40-character certificate SHA-1 hash from that output to `codesign --sign` unchanged.
Neither a Keychain label nor the certificate's common name works as that selector.
See [Apple's explanation of signing identity lookup](https://developer.apple.com/documentation/technotes/tn3161-inside-code-signing-certificates).

Reuse an existing certificate and private key.
For CI, export both as a password-protected `.p12`, and keep the file and its password in 1Password.
A `.cer` holds only the public certificate and signs nothing.
Use a macOS keychain for request creation and native signing, because `codesign` reads no identity from 1Password.
In CI, import the saved certificate and key into a temporary keychain for the job, and remove it afterward.
Never delete an existing local identity after exporting it.
Before an authorized removal, prove that the protected export restores the certificate and key.

Without the identity, check the intended team's Account Holder access, because only the Account Holder can create a Developer ID certificate.

1. Create a CSR in Keychain Access on the Mac that keeps the private key.
2. Create a Developer ID Application certificate from that CSR in the Apple Developer account.
3. Import the downloaded `.cer` on the same Mac, and confirm that its private key appears under the certificate.
4. Export the certificate and private key as a password-protected `.p12` for the repository's CI credential input.
5. Record the checked selector where the workflow expects it, and store the export and password in 1Password.

When macOS distrusts a newly imported certificate, inspect its issuer, validity dates, and local chain before changing trust settings.
Fetch a missing intermediate from [Apple's PKI directory](https://www.apple.com/certificateauthority/), check its chain up to the system Apple root, and import it normally.
Never add the intermediate as a root or force the leaf to Always Trust.
Require `security verify-cert -L -p codeSign -c /path/to/certificate.cer` to pass locally.
Online verification can fetch a missing issuer on the fly, so it proves nothing about the local chain.

Never revoke a certificate or spend a certificate slot because a local file went missing.

See [Apple's Developer ID certificate guide](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/) and [Keychain export instructions](https://support.apple.com/guide/keychain-access/kyca35961/mac).

## Check the import format

Match PKCS12 validation to the importer the release workflow uses.
The legacy `security import` command calls `SecKeychainItemImport` and encodes its `-P` passphrase as ASCII, so a memory-only `SecPKCS12Import` success proves nothing about it.
For such workflows, use a Rust native-import helper with the owner's protected input and the expected leaf certificate's SHA-256.
Require a native import, the expected leaf backed by its private key, cleanup of the keychain and temporary directory, and an unchanged search list.
Use an encrypted private temporary keychain, and leave the login keychain, default keychain, and trust settings unchanged.

When compatibility needs a derived CI copy, use the SHA-1 MAC and `PBESv1SHA1And3KeyTripleDESCBC` options, and test that exact copy with the native importer.
Keep the original export and password unchanged in the durable credential store.
Give the CI copy a generated high-entropy ASCII password that ends in neither CR nor LF, because `gh` strips both from stdin.

Import checks prove only import compatibility, the private key, and the leaf match.
Trust, signing ACLs, timestamps, and notarization need a real signed-artifact rehearsal.

See [Apple's command implementation](https://github.com/apple-oss-distributions/SecurityTool/blob/main/keychain_import.c).

## Scope temporary keychains and protected input

`codesign --keychain` restricts identity lookup, but chain construction still uses the user's keychain search list.
The named keychain supplies intermediates only while it sits on the search list.
Capture the original search-list paths and order exactly before adding a temporary keychain.
Keep spaces, literal quotes, and backslashes, and never split paths on whitespace.
Add the signing keychain, verify the identity and local chain, and keep an explicit keychain argument when signing.
Use a `finally` block or an ownership guard to restore the exact original list and delete only the owned temporary keychain on success or failure.
Attempt every cleanup step, keep the primary failure, and report cleanup failures.
See [GitHub's macOS signing example](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

For `security` operations that take a password, use a native Security API, or send one encoded command through a protected stdin pipe to `/usr/bin/security -i` and close stdin.
Never put passwords in process arguments, echo the input, turn on shell tracing, or add `security -v`, which prints arguments.
Never forward unfiltered diagnostics from those commands.
Quote every token and escape literal double quotes and backslashes for Apple's parser.
Reject NUL, CR, and LF inside tokens before serialization.
Cap the encoded command at 4094 bytes before its single terminating LF, with at most 31 tokens including the command name.
Run one process per command so a later command never masks an earlier failure.
See [Apple's command parser](https://github.com/apple-oss-distributions/SecurityTool/blob/main/security.c).

## Match the notarization key to the workflow

The notarization API key authenticates submissions to Apple and differs from the signing key.
Read the workflow's `notarytool` arguments before choosing a key.

- A team API key uses the `.p8` file, Key ID, and Issuer ID.
- An individual API key omits the Issuer ID.

A workflow that passes `--issuer` needs a team API key.
Check the authentication options with `xcrun notarytool submit --help`.

Reuse a valid stored key before generating another.
For a new team key, the Account Holder requests API access, and the Account Holder or an administrator creates the key under Team Keys in App Store Connect.
Grant a role that can notarize, such as Developer, and nothing broader.
Save the private `.p8`, Key ID, and Issuer ID in 1Password at creation, because Apple offers the download once.

See [App Store Connect API setup](https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api/).

## Resume notarization without holding a build runner

When Apple's processing outlasts a CI job, split submission from completion checks.
Run `notarytool submit ARCHIVE --no-wait --output-format json`, and save the returned submission ID durably before cleanup or runner exit.
Bind the public receipt to the source revision, build workflow and run attempt, target, version, signed binary hash, submission and distribution archive hashes, signing identity, and CDHash.
Keep the exact signed binaries and distribution archives with their build provenance, and keep private material out of the receipt.

A later short job checks the source and artifact bindings, then queries each saved ID with `notarytool info SUBMISSION_ID --output-format json`.
Resume the original submissions without rebuilding, signing, or resubmitting.
Save the ID before any bounded `notarytool wait --timeout`.

Report `In Progress` as pending, never as accepted or release-ready.
A polling job never publishes or writes an accepted completion marker while pending.
Reject malformed or mismatched IDs, unknown states, missing artifacts, and failed notarization.
After `Accepted`, fetch the notary log, check its issues, and match its ticket CDHashes to the saved signed code for every distributed architecture.

Choose a polling deadline shorter than the saved artifacts' retention, and stop automatic finalization at that deadline.
Expiry grants no new submission, so report the pending state and keep the evidence for explicit recovery.
When newer workflow code finishes the release, keep the original build provenance.
For immutable publication, check the current release ref and source binding before publishing a fully verified draft.
Retry a partial draft only after checking existing asset hashes, and add missing original assets without replacing any.

See [Apple's custom notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow), [submission status and log commands](https://developer.apple.com/documentation/technotes/tn3147-migrating-to-the-latest-notarization-tool), and [ticket CDHashes](https://developer.apple.com/forums/thread/720093).

## Verify the final artifact

For a new submission, sign the final executable with the intended Developer ID Application identity, hardened runtime, and a secure timestamp, submit it, and require `Accepted`.
For a resumed submission, require `Accepted` for the saved ID, and verify the preserved artifacts without repeating signing or upload.

On each architecture's extracted executable, run:

```sh
codesign --verify --strict --verbose=2 /path/to/binary
codesign --display --verbose=2 /path/to/binary
codesign --verify --verbose=2 -R='notarized' --check-notarization /path/to/binary
```

Check the signing authority, Team ID, hardened runtime, and timestamp, not only the exit status.
Apple staples no ticket to a standalone executable.
Staple only a distributed `.app` bundle, disk image, or installer package.
Verify archive checksums, build provenance, and the extracted binary's signature.

See [Apple's native notarization test procedure](https://developer.apple.com/forums/thread/130560).
