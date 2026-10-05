# Code signing on Windows

## Check the existing certificate order

Use the user's existing SSL.com code signing certificate and eSigner subscription.
Verify the intended publisher, certificate issuance, validity, and active eSigner enrollment.
Finish any outstanding validation on that order instead of buying a replacement.
The private key stays in SSL.com's cloud hardware security module, so this workflow needs no Windows `.p12` export.

See [SSL.com's enrollment guide](https://www.ssl.com/how-to/enroll-esigner-remote-document-ev-code-signing/) and the [eSigner overview](https://www.ssl.com/esigner/).

## Locate the signing credentials

The account username and password authenticate to eSigner.
The Credential ID selects the signing certificate.
The certificate's persistent TOTP secret lets automation generate each OTP.
The persistent secret differs from the six-digit OTP and the enrollment PIN.

In the SSL.com account, open the certificate order's download page and find the Credential ID in its signing credentials section.
An installed CodeSignTool also lists it through `get_credential_ids` and `credential_info`.
Keep account passwords out of example command arguments.

For an enrolled certificate, reuse the secret saved in 1Password.
Without a saved secret, the owner opens the order's download page, enters the enrollment PIN in the protected UI, and chooses **Show QR Code**.
The owner saves the persistent secret in 1Password through a protected input.
Keep the QR code and its secret out of screenshots, logs, and any browser tooling that returns screenshots or DOM snapshots.
Receive only the owner's confirmation of the save.
Never choose **Reset** to inspect the existing code.
A reset breaks other automation and needs explicit authorization.

Map the four values to the repository's designated environment secrets.

See the [signing credential guide](https://www.ssl.com/guide/esigner-signing-credential-guide/), [viewing or resetting an eSigner QR code](https://www.ssl.com/how-to/view-reset-esigner-qr-code-reset-pin/), [automated signing guide](https://www.ssl.com/how-to/automate-esigner-ev-code-signing/), and [SSL.com's GitHub signing action](https://github.com/SSLcom/esigner-codesign).

## Diagnose provider TLS failures before changing credentials

For TLS handshake or PKIX trust-chain failures, identify the Java executable and trust store of the failing request.
`JAVA_HOME` leaves a wrapper that launches its own bundled Java unchanged, so inspect the wrapper and runtime version.
Use a provider-supported entry point with an explicitly selected, maintained Java runtime, and check whether arguments or configuration override its trust store.
For SSL.com's GitHub action, inspect the pinned action's `signing_method` modes and Java selection before changing them.
Probe the failing endpoint over HTTPS without credentials before retrying authentication or signing.
Never rotate passwords or TOTP secrets because of a failure before authentication.
Keep certificate and hostname verification, and never import the server's leaf certificate as a trust anchor.

See [SSL.com's signing action and runtime selection](https://github.com/SSLcom/esigner-codesign) and [Oracle's JSSE trust-store guidance](https://docs.oracle.com/en/java/javase/11/security/java-secure-socket-extension-jsse-reference-guide.html).

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

Exit code 2 signals warnings and fails this release check.
When PowerShell omits `TimeStamperCertificate`, rely on the SDK verification.
Without the required verifier, report the check as incomplete.
Check the distributed archive's checksum and source provenance.

See [Microsoft's Authenticode inspection command](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.security/get-authenticodesignature) and [SignTool options and exit codes](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool).
