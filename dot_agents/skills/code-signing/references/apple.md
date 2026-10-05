# Apple signing and notarization

## Reuse or create the signing identity

Sign a standalone macOS command-line tool distributed outside the App Store with a `Developer ID Application` certificate.
Apple Development and Apple Distribution certificates never replace it.
A `Developer ID Installer` certificate signs installer packages, not executables.

Inspect public identity metadata:

```sh
security find-identity -v -p codesigning
```

Match the intended publisher and team, check validity, and confirm the private key.
Never pick the first identity only because it exists.
For automated signing, pass the 40-character Secure Hash Algorithm (SHA) 1 certificate selector from `security find-identity -v -p codesigning` to `codesign --sign` unchanged, after confirming it names the intended identity.
Neither a Keychain label nor the certificate's common name works as that selector.
The selector picks a certificate and leaves the code-signature algorithm unchanged.
See [Apple's explanation of signing identity lookup](https://developer.apple.com/documentation/technotes/tn3161-inside-code-signing-certificates).

Reuse an existing certificate and private key.
For CI, export both as a password-protected `.p12`, and keep the file and its password in 1Password.
A `.cer` holds only the public certificate and signs nothing.

Keep the protected export and its password in 1Password or another approved credential store.
Use a macOS keychain for request creation and native signing, because `codesign` reads no identity from 1Password.
In CI, import the saved certificate and key into a temporary keychain for the job, and remove it afterward.
Never delete an existing local identity after exporting it.
Before an authorized removal, prove that the protected export restores the certificate and key.
See [1Password's file storage instructions](https://support.1password.com/files/).

Without the identity, check the intended team's Account Holder access, because only the Account Holder can create a Developer ID certificate.
Membership shows neither the selected team nor the role.

1. Create a Certificate Signing Request (CSR) in Keychain Access on the Mac that keeps the private key.
2. Create a `Developer ID Application` certificate from that CSR in the Apple Developer account.
3. Import the downloaded `.cer` on the same Mac, and confirm its private key under the certificate in `My Certificates`.
4. Export the certificate and private key as a password-protected `.p12` for the repository's CI credential input.
5. Record the checked selector where the workflow expects it, and store the export and password in 1Password.

When macOS distrusts a newly imported certificate, inspect its issuer, validity dates, and local chain before changing trust settings.
Check that the certificate's public key matches the CSR and that the identity holds the matching private key.
Fetch a missing intermediate from [Apple's certificate authority directory](https://www.apple.com/certificateauthority/), check its chain up to the system Apple root, and import it normally.
Never add the intermediate as a root or force the leaf to Always Trust.
Require `security verify-cert -L -p codeSign -c /path/to/certificate.cer` to pass locally, and confirm the identity in `security find-identity -v -p codesigning`.
Online verification can fetch a missing issuer on the fly, so it proves nothing about the local chain.
See [Apple's certificate trust diagnosis](https://developer.apple.com/forums/thread/712043).

Never revoke a certificate or spend a certificate slot because a local file went missing.
Find the existing identity or saved export first.

See [Apple's Developer ID certificate guide](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/), [CSR instructions](https://developer.apple.com/help/account/certificates/create-a-certificate-signing-request/), and [Keychain export instructions](https://support.apple.com/guide/keychain-access/kyca35961/mac).

## Check the import format

Test the `.p12` with the importer the release workflow uses.
A memory-only `SecPKCS12Import` success proves nothing about the legacy `security import` command.
That command calls `SecKeychainItemImport` and encodes its `-P` passphrase as `ASCII`.
For such workflows, use a Rust native-import helper with the owner's protected input and the expected leaf certificate's SHA-256 digest.
Require a native import, the expected leaf backed by its private key, cleanup of the keychain and temporary directory, and an unchanged search list.
Use an encrypted private temporary keychain, and leave the login keychain, default keychain, and trust settings unchanged.

When compatibility needs a derived CI copy, use the documented `SHA1` message authentication and `PBESv1SHA1And3KeyTripleDESCBC` options, and test that exact copy with the native importer.
Keep the original export and password unchanged in the durable credential store.
Give the CI copy a generated high-entropy `ASCII` password that ends in neither carriage return nor line feed, because `gh` strips both from standard input.
Pass credentials through protected input, never command arguments.

Import validation proves only import compatibility, the private key, and the leaf match.
Trust, signing access controls, timestamps, and notarization still need a real signed-artifact rehearsal.

See [Apple's command implementation](https://github.com/apple-oss-distributions/SecurityTool/blob/main/keychain_import.c), [cryptography's PKCS12 compatibility documentation](https://cryptography.io/en/stable/hazmat/primitives/asymmetric/serialization/#pkcs12), and [the `gh` input handling](https://github.com/cli/cli/blob/trunk/pkg/cmd/secret/set/set.go).

## Scope temporary keychains and protected input

`codesign --keychain` restricts identity lookup, but chain construction still uses the user's keychain search list.
The named keychain supplies intermediates only while it sits on the search list.
Capture the original search-list paths and order exactly before adding a temporary keychain.
Keep spaces, literal quotes, and backslashes, and never split paths on whitespace or parse displayed paths as shell syntax.
Add the signing keychain, verify the identity and local chain, and keep an explicit keychain argument when signing.
Use a `finally` block or an ownership guard to restore the exact original list and delete only the owned temporary keychain on success or failure.
Attempt every cleanup step, keep the primary failure, and report cleanup failures, because signing with failed cleanup leaves the work unfinished.
Leave durable credential-store exports and existing keychains unchanged.
See the installed `codesign` manual's `--keychain` and `SIGNING IDENTITIES` sections and [GitHub's macOS signing example](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

For `security` operations that take a password, use a native Security API, or send one encoded command through a protected standard input pipe to `/usr/bin/security -i` and close the pipe.
Never put passwords in process arguments, echo the input, turn on shell tracing, or add `security -v`, which prints arguments.
Never forward unfiltered diagnostics from those commands.
Quote every token and escape literal double quotes and backslashes for Apple's parser, and test the round trip with public dummy values.
Reject `NUL`, carriage return, and line feed inside tokens before serialization.
Apple's parser reads 4096 bytes, so cap the encoded command at 4094 bytes before its single terminating line feed, with at most 31 tokens including the command name.
Count encoded bytes and escaping overhead, not characters.
Before using credentials, probe the installed command with harmless passing and failing commands to confirm parsing and nonzero exit status.
Run one process per command so a later command never masks an earlier failure.
See [Apple's command parser, verbose output, and status handling](https://github.com/apple-oss-distributions/SecurityTool/blob/main/security.c) and [its input reader](https://github.com/apple-oss-distributions/SecurityTool/blob/main/readline.c).

## Match the notarization key to the workflow

The notarization API key authenticates submissions to Apple and differs from the signing key.
Read the workflow's `notarytool` arguments before choosing a key.

- A team API key uses the `.p8` file, Key ID, and Issuer ID.
- An individual API key omits the Issuer ID.

A workflow that passes `--issuer` needs a team API key.
An individual Apple Developer Program membership has nothing to do with an individual API key.
Check the installed tool's authentication options with `xcrun notarytool submit --help`.

Reuse a valid stored key before generating another.
For a new team key, the Account Holder requests API access, and the Account Holder or an `Admin` creates the key under Team Keys in App Store Connect.
Grant a role that can notarize, such as Developer, and nothing broader.
Save the private `.p8`, Key ID, and Issuer ID in 1Password at creation, because Apple offers the download once.
Never rotate a key only because its local download went missing.

See [App Store Connect API setup](https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api/) and [Apple's API key documentation](https://developer.apple.com/documentation/appstoreconnectapi/creating-api-keys-for-app-store-connect-api).

## Resume notarization without holding a build runner

When Apple's processing outlasts a CI job, split submission from completion checks.
Run `notarytool submit ARCHIVE --no-wait --output-format json`, and save the returned submission ID durably before cleanup or runner exit.
Bind the public receipt to the source revision, build workflow and run attempt, target, version, signed binary hash, submission and distribution archive hashes, signing identity, and CDHash.
Keep the exact signed binaries and distribution archives with their build provenance, because a receipt keeps no bytes.
Keep private signing and authentication material out of the receipt.

A later short job checks the source and artifact bindings, then queries each saved ID with `notarytool info SUBMISSION_ID --output-format json`.
Use the project's protected authentication input, and sign nothing to check status.
Resume the original submissions without rebuilding, signing, or resubmitting.
Save the ID before any bounded `notarytool wait --timeout`, because Apple keeps processing after the client times out.

Report `In Progress` as pending, never as accepted or release-ready.
A polling job may end while pending, but it never publishes or writes an accepted completion marker.
Reject malformed or mismatched IDs, unknown states, missing artifacts, and failed notarization instead of inferring success.
After `Accepted`, fetch the notary log, check its issues, and match its ticket CDHashes to the saved signed code for every distributed architecture.
Finish the artifact checks below before publishing.

Choose a polling deadline shorter than the saved artifacts' retention, and stop automatic finalization at that deadline.
Expiry neither cancels Apple's work nor grants another submission, so report the pending state and keep the evidence for explicit recovery.
When newer workflow code finishes the release, keep the original build provenance instead of attributing old binaries to the newer revision.
For immutable publication, check the current release ref and source binding before publishing a fully verified draft, then keep its assets and tag.
Retry a partial draft only after checking existing asset hashes, and add missing original assets without replacing any.

See [Apple's custom notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow), [submission status and log commands](https://developer.apple.com/documentation/technotes/tn3147-migrating-to-the-latest-notarization-tool), and [Apple's explanation of ticket CDHashes](https://developer.apple.com/forums/thread/720093).
Check the installed `notarytool submit`, `info`, and `wait` help for flags and timeout behavior.

## Verify the final artifact

For a new submission, sign the final executable with the intended `Developer ID Application` identity, hardened runtime, and a secure timestamp.
Submit that signed artifact, and require an `Accepted` result.
For a resumed submission, require `Accepted` for the saved ID, and verify the preserved artifacts without repeating signing or upload.
Fix a failed submission from the cause in its notarization log.

On each architecture's extracted executable, run:

```sh
codesign --verify --strict --verbose=2 /path/to/binary
codesign --display --verbose=2 /path/to/binary
codesign --verify --verbose=2 -R='notarized' --check-notarization /path/to/binary
```

Check the signing authority, Team ID, hardened runtime, and timestamp, not only the exit status.
The explicit notarization check forces an online lookup for a standalone tool.
Apple staples no ticket to a standalone executable.
Staple only a distributed `.app` bundle, disk image, or installer package.
Verify archive checksums, build provenance, and the extracted binary's signature.

See [Apple's notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow), [Apple's native notarization test procedure](https://developer.apple.com/forums/thread/130560), and [Apple's explanation of supported stapling formats](https://developer.apple.com/videos/play/wwdc2019/703/?time=1948).
