# Windows code signing

## Check the existing certificate order

Use the user's existing SSL.com code signing certificate and eSigner subscription.
Verify the intended publisher, certificate issuance, validity, and active eSigner enrollment.
Finish any outstanding validation on that order instead of buying a replacement.
The enrollment guide accepts individual, organization, and extended validation certificates, so never assume an upgrade.
The private key stays in SSL.com's cloud hardware security module, so this workflow needs no Windows `.p12` export.

See [SSL.com's enrollment guide](https://www.ssl.com/how-to/enroll-esigner-remote-document-ev-code-signing/) and the [eSigner overview](https://www.ssl.com/esigner/).

## Locate the signing credentials

The account username and password authenticate to eSigner.
The credential ID selects the signing certificate.
The certificate's persistent Time-Based One-Time Password (TOTP) secret lets automation generate each one-time code.
Neither the six-digit code nor the enrollment code serves as the persistent secret.
Account login may use a different second factor.

In the SSL.com account, open the certificate order's download page and its `SIGNING CREDENTIALS` section to find the credential ID.
An installed CodeSignTool also lists it through `get_credential_ids` and `credential_info`.
Never install a tool only to read an ID that the account already shows.
Keep account passwords out of example command arguments.

For an enrolled certificate, reuse the TOTP secret saved in 1Password.
Without a saved secret, the owner opens the order's download page, enters the enrollment code in the protected interface, and chooses **Show QR Code** to view the existing enrollment.
The owner saves the persistent secret in 1Password through a protected input.
Keep the QR code and its secret out of screenshots, logs, and any browser tooling that returns screenshots or page snapshots.
Receive only the owner's confirmation of the save, never the QR code, vault contents, or secret value.
Never choose **Reset** to inspect the existing code.
A reset changes the signing secret and breaks other automation, so it needs explicit authorization.

Map the four values to the repository's designated environment secrets.
Look for saved credentials from an existing signing setup before starting a new enrollment.

See the [signing credential guide](https://www.ssl.com/guide/esigner-signing-credential-guide/), [viewing or resetting an eSigner QR code](https://www.ssl.com/how-to/view-reset-esigner-qr-code-reset-pin/), [automated signing guide](https://www.ssl.com/how-to/automate-esigner-ev-code-signing/), and [SSL.com's GitHub signing action](https://github.com/SSLcom/esigner-codesign).

## Diagnose provider handshake failures before changing credentials

For Transport Layer Security (TLS) handshake or trust-chain failures, identify the Java executable and trust store of the failing request.
Inspect the provider's wrapper and runtime version, because `JAVA_HOME` leaves a wrapper that launches its own bundled Java unchanged.
Use a provider-supported entry point with an explicitly selected, maintained Java runtime, and check whether arguments or configuration override its trust store.
For SSL.com's GitHub action, inspect the pinned action's `signing_method` modes and Java selection before changing them.
Probe the failing endpoint over HTTPS without credentials before retrying authentication or signing.
A TLS failure before authentication says nothing about the credentials, so never rotate passwords or TOTP secrets because of it.
Keep certificate and hostname verification, and never import the server's leaf certificate as a trust anchor.
A passing TLS probe proves nothing about authentication or signing, so finish the authorized signing rehearsal and artifact checks.

See [SSL.com's signing action and runtime selection](https://github.com/SSLcom/esigner-codesign) and [Oracle's Java Secure Socket Extension trust-store guidance](https://docs.oracle.com/en/java/javase/11/security/java-secure-socket-extension-jsse-reference-guide.html).

## Verify the signed Windows executable

Use the production signing service and require a timestamp.
Embed icons and other native resources before signing.
On native Windows, inspect the final extracted executable:

```powershell
$signature = Get-AuthenticodeSignature -FilePath .\binary.exe
$signature | Format-List Status, SignerCertificate, TimeStamperCertificate
if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate) {
    throw 'The distribution needs a valid Authenticode signature and signer certificate.'
}
```

Compare the signer certificate with the intended publisher.
For the Windows SDK path, resolve `$signtool` to the installed SDK's `signtool.exe` and require exit code zero:

```powershell
& $signtool verify /all /pa /tw /v .\binary.exe
$verificationExitCode = $LASTEXITCODE
if ($verificationExitCode -ne 0) {
    throw "Signature or timestamp verification failed: $verificationExitCode"
}
```

`/pa` applies the Authenticode policy, `/all` verifies every signature, and `/tw` warns when a signature lacks a timestamp.
Exit code 2 signals warnings and fails this release check.
The check requires a timestamp but no particular timestamp format.
When PowerShell omits `TimeStamperCertificate`, treat the metadata as unavailable and rely on the SDK verification.
Without the required verifier, report the check as incomplete instead of accepting the artifact.
Check the distributed archive's checksum and source provenance on their own.
An embedded icon and a successful build prove nothing about the signature.
Signing leaves elevation prompts in place and some SmartScreen warnings possible.

See [Microsoft's Authenticode inspection command](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.security/get-authenticodesignature), [SignTool options and exit codes](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool), and [User Account Control behavior](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/how-it-works).
