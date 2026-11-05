# Verification and distribution

Use [Logger guidance](https://developer.apple.com/documentation/os/generating-log-messages-from-your-code) and [signposts](https://developer.apple.com/documentation/os/recording-performance-data) for processing phases, outcomes, and durations.
Use fixed categories and unique interval identities.
Keep images, decoded text, destinations, and arbitrary errors out of diagnostic metadata.
Private interpolation isn't a reason to collect a payload the diagnosis doesn't need.

Read [privacy manifest scope](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files) for the actual platform and dependencies.
The all-platform data-collection declaration and the named-platform required-reason API declaration are different requirements.
A macOS project can choose to bundle a no-collection manifest without claiming that macOS appears in the required-reason platform list.
Inspect the built resources and the generated privacy report where applicable.

Use deterministic native tests for permissions, operation ordering, stale completions, action failures, and preference migration.
Use real known-answer images and a blank control for recognition frameworks.
Record actual target architecture execution rather than treating cross-compilation as a native test.
Keep fast contributor checks independent of owner-only tools and move full proof and mutation campaigns to the release gate.
Issue a release receipt only after every required check succeeds for the exact source, tools, targets, and built artifact.
Test rejection of missing, incomplete, and stale evidence at the packaging boundary.

Choose Mac App Store or Developer ID distribution before defining release evidence.
Read [distribution guidance](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases) for the chosen route.
Use `apple-signing`, `apple-notarization`, and `release-evidence` for their specific stages.
Preparing or verifying an app grants no authority to publish it.

## Cross-platform comparison

Android's [permission workflow](https://developer.android.com/training/permissions/requesting) asks for access in context and preserves unrelated features after denial or revocation.
The same user outcome prompts macOS denial, recovery, and revocation tests, while Screen Recording keeps its own authorization and relaunch contract.
Android's [Compose defaults](https://developer.android.com/develop/ui/compose/accessibility/api-defaults) require targets of at least 48 dp.
macOS uses Apple's desktop control guidance and native keyboard behavior rather than that measurement.
Android's [log disclosure guidance](https://developer.android.com/privacy-and-security/risks/log-info-disclosure) and Apple's logging guidance both motivate excluding sensitive payloads.
The logging APIs and release configuration remain platform-specific.
The compared Android guidance already covers these outcomes, so the desktop split needs no Android API changes.
