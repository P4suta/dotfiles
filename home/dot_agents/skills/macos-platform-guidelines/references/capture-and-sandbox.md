# Capture and sandboxing

## Permission and screenshot scope

Read [ScreenCaptureKit guidance](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos) and the selected API's availability.
Request Screen Recording in the context of the task that needs it.
Distinguish initial explanation, denial, revocation, usable permission, and a grant that requires relaunch.
Check current access when returning from System Settings and before publishing a captured result.
Don't assume every denial requires relaunch or repeat a prompt as a retry strategy.
Treat restart failure as a recoverable operation with the original app still usable.

[captureScreenshot](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager/capturescreenshot(contentfilter:configuration:completionhandler:)) and [SCScreenshotConfiguration](https://developer.apple.com/documentation/screencapturekit/scscreenshotconfiguration) require macOS 26 or later.
The configuration defines pixel dimensions, cursor visibility, dynamic range, and an optional file destination.
Choose those properties from the task rather than copying streaming defaults.
For standard dynamic range processing, request `.sdr` and check [sdrImage](https://developer.apple.com/documentation/screencapturekit/scscreenshotoutput/sdrimage) before decoding.
Leave `fileURL` unset for in-memory processing.
Test display coverage, Retina scaling, app exclusion, cancellation between displays, and the point at which result UI returns.
Protected or unavailable content remains a framework boundary.

## Least privilege and migration

Apple requires [App Sandbox](https://developer.apple.com/documentation/security/app-sandbox) for Mac App Store distribution.
Adopting it for a Developer ID app is a project choice that limits resource access.
Read [sandbox configuration](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox) before adding entitlements.
Screen Recording consent and sandbox entitlements serve different purposes.
Add only capabilities needed by the selected APIs and actual operations.
Opening a browser through the system doesn't itself justify networking or broad filesystem access.

Use Apple's [container migration manifest](https://developer.apple.com/documentation/security/migrating-your-app-s-files-to-its-app-sandbox-container) to move existing support files when adopting App Sandbox.
Validate imported preference types, preserve current preferences, and make app-level migration idempotent.
Test first launch, no prior data, malformed data, and repeated launch without removing a person's real container.
Inspect [sandbox violations](https://developer.apple.com/documentation/security/discovering-and-diagnosing-app-sandbox-violations) and the built app's signed entitlements.
Exercise capture, shortcuts, browser opening, clipboard access, and relaunch from the sandboxed app in a login session.
