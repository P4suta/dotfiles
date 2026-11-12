---
name: android-platform-guidelines
description: >-
  Ground Android app design, architecture, permissions, background work, privacy, testing, and distribution in current Android Developers and Google Play guidance.
  Use for Android app decisions and reviews that involve Kotlin or Jetpack Compose.
---

# Android platform guidelines

Read the current primary source for each Android decision and cite its exact URL beside the decision in an issue, design record, or review.
Distinguish platform requirements, official recommendations, and project choices.
Record a deliberate departure with its reason and verification limits.
Check the app's supported Android releases and `targetSdk`, because permission, background execution, and navigation behavior depend on both.
Use `monochrome-design` and `source-bound-interface-design` within Android's native interaction and accessibility rules.

## Architecture and interface

Use the [architecture recommendations](https://developer.android.com/topic/architecture/recommendations) to assess repository boundaries, unidirectional data flow, screen ViewModels, and lifecycle-aware state collection.
Keep contexts and lifecycle objects out of ViewModels.
Use the [offline data guidance](https://developer.android.com/topic/architecture/data-layer/offline-first) when local data must remain usable during network failure.
Check atomic publication, cancellation, process death, and recovery at the actual storage boundary.

Use [Compose accessibility defaults](https://developer.android.com/develop/ui/compose/accessibility/api-defaults) for semantic controls and tap targets of at least 48 dp.
Verify TalkBack, large text, focus order, contrast, and narrow layouts on the shipped interface.
Respect [window insets](https://developer.android.com/develop/ui/compose/system/insets), system navigation, the keyboard, and the [system splash screen](https://developer.android.com/develop/ui/views/launch/splash-screen).
Keep recoverable status in context and explain failures that require a person's action without exposing internal errors.
Use Android's own localized control labels and string resources.

## Permissions, lifecycle, and data

Follow the [runtime permission workflow](https://developer.android.com/training/permissions/requesting), request access in context, and preserve unrelated features after denial or revocation.
Treat roles and special access as distinct from runtime permissions.
Read the specific role or service contract before using it.
Read the response deadline and call eligibility rules in [CallScreeningService](https://developer.android.com/reference/android/telecom/CallScreeningService) before adding caller identification.
An extension's successful lookup must never become an unintended condition for continuing the primary system operation.

Choose persistent work using [WorkManager guidance](https://developer.android.com/develop/background-work/background-tasks/persistent) and its [constraints and retry rules](https://developer.android.com/develop/background-work/background-tasks/persistent/getting-started/define-work).
Don't promise an exact periodic execution time.
Check [background activity launch restrictions](https://developer.android.com/guide/components/activities/secure-bal) before displaying an activity from a service.
Test role revocation, lock-screen display, process termination, reboot, cancellation, and network transitions when the feature depends on them.

Apply the [security guidance](https://developer.android.com/privacy-and-security/security-tips) to private storage, exported components, transport, backups, and interprocess input.
Use [common intents](https://developer.android.com/guide/components/intents-common) for dial and map actions, and handle a missing receiving app.
Keep sensitive inputs out of logs under the [log disclosure guidance](https://developer.android.com/privacy-and-security/risks/log-info-disclosure).
Audit network payloads, third-party dependencies, and direct app code.

## Verification and distribution

Verify the [Android Gradle plugin compatibility matrix](https://developer.android.com/build/releases) and the [Compose compiler setup](https://developer.android.com/develop/ui/compose/setup-compose-dependencies-and-compiler) before pinning tools.
Run lint, compiler checks, focused local tests, and device tests appropriate to the changed contracts.
Apply `formal-assurance` to deterministic decisions and bounds, and `development-assurance` to lifecycle and device boundaries.
Distinguish a model proof, host test, emulator observation, and physical-device result.
Record the actual release, device, permissions, network state, and artifact identity for empirical checks.

Follow [app signing](https://developer.android.com/studio/publish/app-signing) for upload and app-signing key identities and recovery.
Match the [Google Play Data safety declaration](https://support.google.com/googleplay/android-developer/answer/10787469?hl=en) to the shipped dependencies, service logging, external destinations, and privacy policy.
Keep credentials and signing material outside source control.
Preparing an artifact grants no authority to publish it or create an account.

## Compare platform guidance

When changing platform guidance, compare `apple-platform-guidelines` by user outcome and failure condition.
Carry a useful question about accessibility, lifecycle, permissions, offline recovery, privacy, diagnostics, or release evidence into the other skill only after reading that platform's primary source.
Preserve each platform's APIs, units, contracts, and exceptions instead of translating API names by analogy.
Record a concrete gap and the evidence for each transferred rule.
A comparison doesn't require changing both skills when the other platform already covers the property.
