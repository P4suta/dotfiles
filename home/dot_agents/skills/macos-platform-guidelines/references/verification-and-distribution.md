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

Prepare the window state a Mac UI scenario needs after every launch.
Activation alone can leave restored windows minimized.
If `app.windows` omits minimized windows, inventory existing windows through usable entries in the app's [Window menu](https://developer.apple.com/documentation/appkit/nsapplication/windowsmenu).
AppKit's `windowsMenu` property can be `nil` when the menu is absent.
If window entries are unavailable, use supported controls supplied by the app or report unsupported window preparation.
Unsupported preparation leaves required coverage incomplete.
Select each entry to [show its window](https://developer.apple.com/documentation/appkit/nswindow/makekeyandorderfront(_:)) before closing unwanted windows with [Command-W](https://support.apple.com/en-us/102650).
Use observed accessibility identifiers and indexed entries when window titles repeat.
Verify a visible window and usable controls after preparation.

On 2026-11-14, XCTest omitted minimized windows on macOS 27.0.1 with Xcode 27.0.
The [hosted regressions](https://github.com/P4suta/gymemo/actions/runs/43600500036) passed for two open windows, two minimized windows, repeated preparation, and relaunch.
Keep the query observation scoped to the tested SDK.
Recheck window queries and menu selectors after changing the OS or SDK.

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
Android's [activity lifecycle](https://developer.android.com/guide/components/activities/activity-lifecycle#onresume) ties interaction to the Resumed state.
Keep activity callbacks and Mac window restoration with their respective platforms.
The compared Android guidance already covers these outcomes, so the desktop split needs no Android API changes.
