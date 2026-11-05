# Behavior on other Apple platforms

## Mobile lifecycle

Read [launch-screen guidance](https://developer.apple.com/documentation/xcode/specifying-your-apps-launch-screen) for an app with a launch screen.
Use platform text styles, Dynamic Type, and [ScaledMetric](https://developer.apple.com/documentation/swiftui/scaledmetric) where dimensions follow text.
These mechanisms don't establish macOS text enlargement behavior.

For bounded continuation of foreground work, read [extending background execution](https://developer.apple.com/documentation/uikit/extending-your-app-s-background-execution-time).
Release resources and end the task under the API's `expirationHandler` contract.
For scheduled work, read [background strategies](https://developer.apple.com/documentation/backgroundtasks/choosing-background-strategies-for-your-app).
The [earliestBeginDate](https://developer.apple.com/documentation/backgroundtasks/bgtaskrequest/earliestbegindate) sets a lower bound, not an exact execution time.
Don't apply UIKit background-task APIs to a native macOS app.

## Other platforms

Read the platform considerations of the relevant Human Interface Guidelines topic before selecting focus, input, layout, background work, or lifecycle behavior.
Verify API availability for the exact target rather than borrowing an iOS contract.
The [device-testing guidance](https://developer.apple.com/documentation/xcode/running-your-app-on-simulated-or-physical-devices) distinguishes simulator capabilities from physical-device behavior.
Record missing device checks as pending acceptance.
