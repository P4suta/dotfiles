---
name: app-store-connect
description: >-
  Prepare and inspect releases through the App Store Connect CLI, including metadata, builds, creative placements, and review drafts.
  Use browser interaction for remaining web-only settings and visual previews.
---

# Prepare store releases

Prefer the [App Store Connect CLI](https://github.com/rorkai/App-Store-Connect-CLI) for supported operations before opening a browser.
Check the project's pinned version and the installed command's help before choosing an operation.
The CLI is a third-party client of Apple's API.
Its available commands and Apple's editable states change independently.

For project pins, use `mise`.
For an existing scoped credential setup, use `doppler`.
For release automation or candidate boundaries, read `release-workflow`.
Read [creative assets](references/creative-assets.md) when preparing headers, search artwork, or their review state.

## Establish the target and authority

Identify the app ID, bundle identifier, platform, version ID, localization IDs, selected build, and current review submission.
Read server state before changing an existing app.
Use the project's metadata files and release commands rather than reconstructing their values from screenshots.
Preserve review contacts and existing settings when changing unrelated fields.

Distinguish preparing metadata, uploading a build, adding items to a draft, submitting for review, and releasing an approved version.
Apple's [submission procedure](https://developer.apple.com/help/app-store-connect/manage-submissions-to-app-review/submit-an-app) separates adding an item for review from submitting the review draft.
An instruction to leave the final submission button to the user stops before submission.
For that handoff, verify a ready draft with the intended items and a null submission timestamp.
Keep the selected release option explicit, because automatic release can publish after approval.

## Authenticate without borrowing the browser session

Use the existing API key's issuer ID, key ID, and private key through the CLI's supported environment variables.
Read current authentication help before mapping stored credentials into supported environment variables or an existing key path.
The issuer ID uses `ASC_ISSUER_ID`.
The key ID uses `ASC_KEY_ID`.
The Base64-encoded private key uses `ASC_PRIVATE_KEY_B64`.

Pass credentials through the consuming process environment.
Keep credentials out of command arguments and plaintext files.
For release operations, select the existing scoped Doppler project and config.
Inject credentials with `doppler run`.
Select required secret names with `--only-secrets`.
Turn off cached fallback with `--no-fallback`.
Don't change authentication methods or broaden access to repair an unrelated validation failure.
Preserve configured telemetry opt-outs.
Supported opt-out variables include `ASC_TELEMETRY_DISABLED`.
When supported, also preserve `DO_NOT_TRACK`.

An authenticated browser and a cached CLI Apple web session are separate capabilities.
An API key doesn't create either web session.
Before relying on a web-session command, inspect `asc web auth status`.
Never read browser cookies, profile databases, or stored passwords as a fallback.

## Inspect and prepare through supported commands

Use read-only mode for discovery and inspect the relevant help.
The package input below is an iOS App Store Package (IPA):

```sh
asc --read-only status --app "$APP_ID"
asc --read-only versions view --version-id "$VERSION_ID" --include-build --include-submission
asc --read-only review status --app "$APP_ID" --version-id "$VERSION_ID"
asc --read-only validate --app "$APP_ID" --version-id "$VERSION_ID" --strict --ipa "$IPA_PATH"
```

Read localization IDs before updating selected fields.
Verify pricing, territories, age rating, content rights, encryption, URLs, review details, screenshots, and applicable declarations through the current API commands.
For public copy, combine Apple's [product page guidance](https://developer.apple.com/app-store/product-page/) with `editorial-assurance`.
Keep descriptions about the user's work and outcomes.
Use promotional text for useful current information, within Apple's current field limit.

Reuse an already processed build for metadata or artwork changes.
Don't upload another build because a convenience publishing command requires an IPA argument.
For an actual binary change, follow the project's candidate checks, archive, export, and native signature verification.
Compare the exact package's bundle identifier, version, build number, signing team, profile, entitlements, and resource exclusions with the selected candidate.
Complement the platform's signature checks with `asc ipa-info`.
Report unavailable remote checksums as a verification limit.
Local hash and signature checks establish their own properties.

Treat a mutation timeout as an unknown outcome until you inspect server state.
Inspect partial upload receipts before retrying or deleting anything.
Filter response fields before displaying them: review contact details and temporary upload URLs can appear in ordinary resource responses.
Keep operational IDs, credentials, private contacts, and project paths out of reusable skill files.

## Complete the remaining web checks

Read the validator's individual findings rather than treating its exit code as a complete store audit.
Some checks need a cached Apple web session when using `asc validate --deep`.
A missing session establishes a coverage gap, not a missing app setting.
Use an authorized existing web session when available, or batch the remaining checks in the selected browser.
Verify published App Privacy answers, applicable business agreements, and actual device previews where CLI coverage is absent.
Derive privacy answers from the shipped code and SDKs rather than assuming no collection for every app.
Check applicability before filling business agreements, monetization details, or category declarations.
Don't accept legal agreements without the required authorization.

Use the requested browser and reduce focus changes.
When the browser connection is unavailable, use an authorized native UI capability or make one concrete handoff for the unsupported work.
Don't keep retrying focus-stealing automation for an operation the CLI supports.

## Verify the requested stopping point

Read back the selected build, localized fields, placements, asset processing states, review items, and submission timestamp after mutation.
For preparation, create draft submissions and add items through the supported review commands:

```sh
asc review submissions-create --app "$APP_ID" --platform "$PLATFORM"
asc review items add --submission "$SUBMISSION_ID" --item-type appStoreVersions --item-id "$VERSION_ID"
```

Inspect existing drafts before creating another one.
Inspect command effects before calling submission or publishing wrappers.
The convenience workflow can prepare and submit a version: `asc review submit`.
An existing draft can use `asc review submissions-submit`.

Use submission commands only when submission is within the current authorization.
Report the verified result and any concrete action still required from the user without claiming that public API validation covers unperformed web checks.
