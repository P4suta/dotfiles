# Apple Developer ID signing and notarization

## Reuse or obtain the correct signing identity

For a standalone macOS CLI distributed outside the App Store, use **Developer ID Application**.
Apple Development and Apple Distribution certificates do not replace it.
Developer ID Installer signs installer packages, not the CLI executable.

Inspect public identity metadata with:

```sh
security find-identity -v -p codesigning
```

Match the intended publisher and team, check validity, and confirm the associated private key is available.
Do not select the first identity just because it exists.
Prefer the exact 40-character certificate SHA-1 selector shown by `security find-identity -v -p codesigning` when configuring automated signing.
Confirm that this hash belongs to the intended valid identity and pass it to `codesign --sign` unchanged.
A Keychain friendly label and a certificate's subject common name are not interchangeable with that selector.
This SHA-1 value selects a certificate; it does not request SHA-1 as the code-signature algorithm.
See [Apple's explanation of signing identity lookup](https://developer.apple.com/documentation/technotes/tn3161-inside-code-signing-certificates).

If a suitable certificate and private key already exist, reuse them.
For CI, export that certificate together with its private key as a password-protected `.p12`, and keep both the file and its password in 1Password.
A `.cer` contains the public certificate alone and cannot sign a binary.

Use 1Password or the user's approved credential store as the durable storage for the protected export and its password.
Use a macOS keychain as the working store for CSR creation and native signing; storing the export in 1Password does not make it a direct `codesign` provider.
For CI, import the saved certificate and private key into a temporary keychain for the job, then remove the temporary credentials after use.
Do not delete an existing local identity automatically after exporting it.
Before any separately authorized removal, verify that the protected export can restore the certificate and private key.
See [1Password's file storage instructions](https://support.1password.com/files/).

If the identity is missing, check the intended team's Account Holder access.
Apple's Developer ID certificate creation procedure requires the Account Holder.
Membership alone does not establish which team or role is currently selected.

1. Create a CSR in Keychain Access on the Mac that will retain the private key.
2. Create a Developer ID Application certificate in the Apple Developer account using that CSR.
3. Import the downloaded `.cer` on the same Mac and confirm its private key appears under the certificate in My Certificates.
4. Export the certificate and private key as a password-protected `.p12` for the repository's CI credential input.
5. Record the validated signing selector where the workflow expects its identity and store the export and password in 1Password.

If a newly imported certificate is not trusted, inspect its issuer, validity dates, and local certificate chain before changing trust settings.
Check that the issued certificate's public key matches the CSR and that the identity has its corresponding private key.
Locate any missing intermediate through [Apple's official PKI directory](https://www.apple.com/certificateauthority/), and validate its chain against the existing system Apple root before importing it normally.
Do not add the intermediate as a new root or force the leaf to Always Trust.
Require local chain verification with `security verify-cert -L -p codeSign -c /path/to/certificate.cer` and confirm the intended identity appears in `security find-identity -v -p codesigning`.
Online verification can fetch a missing issuer temporarily, so it does not by itself prove the local chain is complete.
See [Apple's certificate trust diagnosis](https://developer.apple.com/forums/thread/712043).

Do not revoke another certificate or consume a new certificate slot merely because a file is absent locally.
First locate the existing identity or saved export.

Sources: [Apple's Developer ID certificate guide](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/), [CSR instructions](https://developer.apple.com/help/account/certificates/create-a-certificate-signing-request/), and [Keychain export instructions](https://support.apple.com/guide/keychain-access/kyca35961/mac).

## Validate the CI import format

Match PKCS12 validation to the importer used by the release workflow.
A successful memory-only `SecPKCS12Import` does not prove that the legacy `security import` CLI can read the same blob.
Apple's CLI uses `SecKeychainItemImport` and converts its `-P` passphrase with ASCII encoding.
For workflows using that CLI, use a Rust native-import validation helper with owner-managed protected input and an expected leaf DER SHA-256.
Require successful native import, the expected private-key-backed leaf, successful keychain and temporary-directory cleanup, and an unchanged search list.
Use an encrypted private temporary keychain without changing the login keychain, default keychain, or trust settings.

If import compatibility requires a derived CI copy, use the documented SHA1 MAC and `PBESv1SHA1And3KeyTripleDESCBC` option and validate that exact copy with the native importer.
Use this compatibility format only when required; keep the original export and password unchanged in the durable credential store.
For a CI copy, an independently generated high-entropy ASCII password avoids the CLI's character-set constraint.
GitHub CLI also strips trailing CR and LF when reading a secret from stdin; this is a separate transport constraint, so do not generate a CI password ending in either character.
Pass credentials through protected input rather than command arguments.

Import validation establishes import compatibility, private-key presence, and leaf matching only.
It does not verify trust, signing ACLs, timestamps, or notarization.
A real signed-artifact rehearsal is still required.

Sources: [Apple's CLI implementation](https://github.com/apple-oss-distributions/SecurityTool/blob/main/keychain_import.c), [cryptography's PKCS12 compatibility documentation](https://cryptography.io/en/stable/hazmat/primitives/asymmetric/serialization/#pkcs12), and [GitHub CLI's stdin handling](https://github.com/cli/cli/blob/trunk/pkg/cmd/secret/set/set.go).

## Scope temporary keychains and protected CLI input

`codesign --keychain` restricts identity lookup, but certificate-chain construction still uses the user's standard keychain search list.
The specified keychain does not supply intermediate certificates to that lookup unless it is also on the search list.
Capture the original search-list paths and order exactly before creating or adding a temporary keychain.
Preserve spaces, literal quotes, and backslashes; do not split paths on whitespace or treat displayed paths as shell syntax.
Temporarily add the signing keychain, verify the intended valid identity and local chain, and keep an explicit keychain argument when signing.
Use a `finally` block or equivalent ownership guard to restore the exact original list and delete only the owned temporary keychain on success or failure.
Attempt every cleanup operation, preserve the primary failure, and report cleanup failures; successful signing with failed cleanup is not complete.
Keep durable credential-store exports and existing keychains unchanged.
Consult the installed `codesign` manual's `--keychain` and `SIGNING IDENTITIES` sections and [GitHub's macOS signing example](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

For password-bearing `security` operations, use a native Security API or send one encoded command through a protected stdin pipe to `/usr/bin/security -i`, then close stdin.
Never place passwords in process arguments, echo the input, enable shell tracing, or add `security -v`, which prints command arguments.
Do not forward unfiltered diagnostics from password-bearing commands.
For stdin encoding, quote every token and escape literal double quotes and backslashes for Apple's command parser; verify their round trip with public dummy values.
Reject NUL, CR, and LF within tokens before serialization.
For Apple's published 4096-byte parser, conservatively limit the encoded command to 4094 bytes before its single terminating LF and at most 31 tokens including the command name.
Count encoded bytes and escaping overhead, not characters.
Probe the installed CLI with harmless successful and failing commands to confirm parsing and nonzero failure propagation before using credentials.
One process per command prevents a later command from replacing an earlier failure status.
See [Apple's command parser, verbose output, and status handling](https://github.com/apple-oss-distributions/SecurityTool/blob/main/security.c) and [its input reader](https://github.com/apple-oss-distributions/SecurityTool/blob/main/readline.c).

## Match the notarization key to the workflow

The notarization API key authenticates submissions to Apple.
It is separate from the private key used to sign the executable.
Read the actual `notarytool` arguments before choosing an API key.

- A team API key uses the `.p8` file, Key ID, and Issuer ID.
- An individual API key omits the Issuer ID.

A workflow that always passes `--issuer` needs a team API key unless its implementation is deliberately changed and verified.
An individual Apple Developer Program membership is not the same concept as an individual API key.
Use `xcrun notarytool submit --help` to verify the installed tool's authentication requirements.

Reuse a valid stored key before generating another.
For a new team key, the Account Holder requests API access if necessary, and the Account Holder or an Admin creates the key in App Store Connect's Team Keys.
Use a role permitted to notarize, such as Developer, rather than granting broader access by default.
Save the private `.p8`, Key ID, and Issuer ID in 1Password when the key is created.
Apple permits downloading that private key only once.
Do not rotate a key simply because its local download is missing.

Sources: [App Store Connect API setup](https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api/) and [Apple's API key documentation](https://developer.apple.com/documentation/appstoreconnectapi/creating-api-keys-for-app-store-connect-api).

## Resume notarization without holding a build runner

When Apple's processing exceeds a practical CI job limit, separate submission from completion checks.
Use `notarytool submit ARCHIVE --no-wait --output-format json` and durably save its returned submission ID before cleanup or runner termination.
Bind the public receipt to the original source revision, build workflow and run attempt, target, version, signed binary hash, submission and distribution archive hashes, signing identity, and CDHash.
Preserve the exact signed binaries and distribution archives with their original build provenance; a receipt alone does not preserve the submitted bytes.
Do not include private signing or authentication material in that receipt.

A later short job must validate the original source and artifact bindings before checking each saved ID with `notarytool info SUBMISSION_ID --output-format json`.
Use the project's existing protected authentication input; checking status does not require another code signature.
Resume the original submissions without rebuilding, signing again, or submitting again merely to check progress.
If a bounded `notarytool wait --timeout` is appropriate, save the ID first; Apple's service continues processing after the client times out.

Report `In Progress` as pending, never as accepted notarization or release readiness.
A polling job may end normally while pending, but it must not publish or produce an accepted completion marker.
Reject malformed or mismatched IDs, unknown states, missing original artifacts, and failed notarization instead of inferring success.
After `Accepted`, fetch the notary log, check its issues, and match its ticket CDHashes against the saved signed code for every distributed architecture.
Complete the final artifact checks below before publishing.

Choose a project-specific polling deadline shorter than the saved artifacts' retention period, and stop automatic finalization when that deadline expires.
Expiry neither cancels Apple's work nor authorizes another submission; report the pending state and preserve the evidence for explicit recovery.
Keep original build provenance when finalizing from newer workflow code rather than attributing old binaries to that later source revision.
For immutable publication, validate the current release ref and source binding before publishing a fully verified draft, then preserve its published assets and tag.
Retry a partial draft only after checking existing asset hashes; add missing original assets without replacing them.

Sources: [Apple's custom notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow), [submission status and log commands](https://developer.apple.com/documentation/technotes/tn3147-migrating-to-the-latest-notarization-tool), and [Apple's explanation of ticket CDHashes](https://developer.apple.com/forums/thread/720093).
Check the installed `notarytool submit`, `info`, and `wait` help for supported flags and timeout semantics.

## Verify the final artifact

For a new submission, sign the final executable with the intended Developer ID Application identity, hardened runtime, and a secure timestamp.
Submit the actual signed artifact for notarization and require an `Accepted` result.
For a resumed submission, require `Accepted` for the saved ID and verify the preserved artifacts without repeating completed signing or upload steps.
For a failed submission, retrieve its notarization log and resolve the reported cause.

On each macOS architecture's extracted executable, run:

```sh
codesign --verify --strict --verbose=2 /path/to/binary
codesign --display --verbose=2 /path/to/binary
codesign --verify --verbose=2 -R='notarized' --check-notarization /path/to/binary
```

Check the signing authority, intended Team ID, hardened runtime, and timestamp in addition to command success.
The explicit notarization check forces an online check for a standalone CLI.
Apple does not support stapling a notarization ticket to a standalone executable.
Use stapling only for a supported distribution format such as an application bundle, disk image, or installer package that the project actually distributes.
Verify archive checksums and build provenance as well as the extracted binary's signature.

Sources: [Apple's notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow), [Apple's native notarization test procedure](https://developer.apple.com/forums/thread/130560), and [Apple's explanation of supported stapling formats](https://developer.apple.com/videos/play/wwdc2019/703/?time=1948).
