# Windows Authenticode with SSL.com eSigner

## Check the existing certificate order

Use the user's existing SSL.com code signing certificate and eSigner subscription.
Verify the intended publisher, certificate issuance, validity, and active eSigner enrollment.
Complete any outstanding validation on that order rather than purchasing a replacement by default.
The enrollment guide supports IV, OV, and EV code signing certificates; do not assume an EV upgrade is required.
The private key stays in SSL.com's cloud hardware security module.
Do not search for a Windows `.p12` export as a prerequisite for this workflow.

Sources: [SSL.com's enrollment guide](https://www.ssl.com/how-to/enroll-esigner-remote-document-ev-code-signing/) and [eSigner overview](https://www.ssl.com/esigner/).

## Locate the certificate's signing credentials

The account username and password authenticate to eSigner.
The Credential ID selects the signing certificate.
The certificate's persistent TOTP secret allows signing automation to generate its changing OTP.
Neither the six-digit OTP nor the enrollment PIN is the persistent secret.
Account login MFA may use a different secret.

In the SSL.com account, open the certificate order's download page and its **SIGNING CREDENTIALS** section to find the Credential ID.
If CodeSignTool is already available, `get_credential_ids` and `credential_info` provide another way to locate and identify the intended certificate.
Do not install another tool solely to retrieve an ID already visible in the account.
Do not put account passwords into example command arguments.

For an enrolled certificate, reuse the TOTP secret saved in 1Password.
If it was not saved, open the order's download page, enter the enrollment PIN in the protected UI, and choose **Show QR Code** to view the existing enrollment information.
Save the persistent secret code in 1Password through a protected input.
Avoid exposing the QR code or its secret in screenshots or logs.
If browser tooling automatically returns screenshots or DOM snapshots, have the user save the secret directly to 1Password in the protected UI instead of opening that screen through the agent's tooling.
Receive only confirmation that the owner saved it in the prepared credential input, not the QR code, vault item contents, or secret value.
Do not choose **Reset** merely to inspect the existing code.
Resetting the QR code changes the signing secret and can break other automation; obtain authorization for a reset if it is actually required.

Map these four values to the actual repository's designated environment secrets.
Locate saved credentials from an existing signing setup before starting new enrollment.

Sources: [Signing credential guide](https://www.ssl.com/guide/esigner-signing-credential-guide/), [viewing or resetting an eSigner QR code](https://www.ssl.com/how-to/view-reset-esigner-qr-code-reset-pin/), [automated signing guide](https://www.ssl.com/how-to/automate-esigner-ev-code-signing/), and [SSL.com's GitHub signing action](https://github.com/SSLcom/esigner-codesign).

## Diagnose provider TLS failures before changing credentials

For TLS handshake or PKIX trust-chain failures, identify the Java executable and trust store used by the failing request.
Inspect the provider's wrapper and runtime version; setting `JAVA_HOME` does not change a wrapper that explicitly launches its own bundled Java.
Prefer a provider-supported entrypoint that uses an explicitly selected, maintained Java runtime, and check whether command arguments or application configuration override its trust store.
For SSL.com's GitHub action, inspect the pinned action's supported `signing_method` modes and Java selection before changing them.
Use a credential-free HTTPS probe of the failing provider endpoint to check TLS before retrying authentication or signing.
A TLS failure before authentication leaves credential validity unassessed; do not rotate passwords or TOTP secrets based on that failure alone.
Preserve certificate and hostname verification; do not bypass TLS checks or import the server's leaf certificate as a trust anchor.
A successful TLS probe does not prove that authentication or signing will succeed, so complete the authorized signing rehearsal and artifact checks afterward.

Sources: [SSL.com's signing action and runtime selection](https://github.com/SSLcom/esigner-codesign) and [Oracle's JSSE trust-store guidance](https://docs.oracle.com/en/java/javase/11/security/java-secure-socket-extension-jsse-reference-guide.html).

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
For the Windows SDK verification path, resolve `$signtool` to the installed SDK's `signtool.exe` and require exit code zero:

```powershell
& $signtool verify /all /pa /tw /v .\binary.exe
$verificationExitCode = $LASTEXITCODE
if ($verificationExitCode -ne 0) {
    throw "Signature or timestamp verification failed: $verificationExitCode"
}
```

`/pa` uses the Authenticode policy, `/all` verifies every signature, and `/tw` warns when a signature lacks a timestamp.
Exit code 2 means warnings and must fail this release check; do not treat every non-error-looking output as success.
This checks timestamp presence, not an exclusive requirement for the RFC 3161 format.
If PowerShell omits `TimeStamperCertificate`, distinguish unavailable metadata from a proven missing timestamp and complete the SDK verification.
If the required verifier is unavailable, report the check as incomplete instead of silently accepting the artifact.
Check the distributed archive's checksum and source provenance separately.
An embedded icon and a successful build do not establish signature validity.
Signing does not suppress UAC when elevation is required and does not guarantee that SmartScreen will never warn.

Sources: [Microsoft's Authenticode inspection command](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.security/get-authenticodesignature), [SignTool options and exit codes](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool), and [UAC behavior](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/how-it-works).
