---
name: apple-platform-guidelines
description: >-
  Ground every iOS, iPadOS, macOS, watchOS, and visionOS app decision in Apple's primary guidance, cite it, and record deliberate deviations.
  Use for the design, copy, networking, background work, logging, privacy, and distribution of an Apple-platform app; use apple-signing and apple-notarization for release signing.
---

# Apple platform guidelines

Before a decision about an iOS, iPadOS, macOS, watchOS, or visionOS app, read Apple's primary source for that topic.
Cite its exact URL in the issue, PR, ADR, or code comment that carries the decision.
Follow the source unless the owner overrides it, and record a deliberate deviation beside the decision with its reason.
Mark what the source leaves open as unknown and settle it on a device.
Apply `monochrome-design` and `source-bound-interface-design` inside these rules, such as Dark Mode that inverts black and white.

## Read the sources

The Human Interface Guidelines and the documentation pages render with JavaScript, so a plain fetch returns little more than the title.
Read the DocC JSON behind a page, and cite the page's own address.

| Page | DocC JSON |
| --- | --- |
| `https://developer.apple.com/design/human-interface-guidelines/PAGE` | `https://developer.apple.com/tutorials/data/design/human-interface-guidelines/PAGE.json` |
| `https://developer.apple.com/documentation/PATH` | `https://developer.apple.com/tutorials/data/documentation/PATH.json` |

`xh GET JSON_URL | jaq -r '.. | objects | select(.type? == "text") | .text'` prints the text of a page.

## Ground rules

People expect an action to succeed and need to know when it doesn't.
Show a message when the person waits and progress has stopped, when something they did failed, or when they can fix the cause.
Stay silent about a failure that the app recovers from without changing what people see.
Show status that needs no action passively in its context, never in a modal dialog.
Say what happened and what the person can do, in plain words without blame or jargon, and never with a bare "Error" title or a code.
Label the cancel button of a dialog with the platform's own word for Cancel in the app's language.

Respect Dark Mode, scale text and the sizes that should follow it with Dynamic Type and `@ScaledMetric`, and declare a launch screen.

Keep App Transport Security's local-networking exception in development configurations.
Share a URL session for related tasks instead of one session per request.
Set `waitsForConnectivity` for a transfer the person started, let an automatic transfer fail fast, and keep automatic transfers off expensive and constrained paths.

Wrap work that must finish in `beginBackgroundTask`, and release file and database locks before the handler that ends the background time returns.
Schedule a periodic check with `BGAppRefreshTask`.

Log with `Logger` at Apple's levels, and keep interpolated values private in shipped builds.
Declare each required-reason API in the privacy manifest, or avoid the API.
Mark intervals worth measuring in Instruments with signposts.
Decide per app whether it needs a String Catalog, and record the reason when it skips one.

## Sources

| Topic | Source |
| --- | --- |
| Feedback | <https://developer.apple.com/design/human-interface-guidelines/feedback> |
| Alerts | <https://developer.apple.com/design/human-interface-guidelines/alerts> |
| Writing | <https://developer.apple.com/design/human-interface-guidelines/writing> |
| Dark Mode | <https://developer.apple.com/design/human-interface-guidelines/dark-mode> |
| Dynamic Type | <https://developer.apple.com/design/human-interface-guidelines/typography> and <https://developer.apple.com/documentation/swiftui/scaledmetric> |
| Launch screen | <https://developer.apple.com/documentation/xcode/specifying-your-apps-launch-screen> |
| Local networking | <https://developer.apple.com/documentation/bundleresources/information-property-list/nsapptransportsecurity/nsallowslocalnetworking> |
| URL sessions | <https://developer.apple.com/documentation/foundation/urlsession> |
| Waiting for connectivity | <https://developer.apple.com/documentation/foundation/urlsessionconfiguration/waitsforconnectivity> |
| Expensive paths | <https://developer.apple.com/documentation/foundation/urlrequest/allowsexpensivenetworkaccess> |
| Background time | <https://developer.apple.com/documentation/uikit/extending-your-app-s-background-execution-time> and <https://developer.apple.com/documentation/xcode/sigkill> |
| Background tasks | <https://developer.apple.com/documentation/backgroundtasks/choosing-background-strategies-for-your-app> |
| Logging | <https://developer.apple.com/documentation/os/generating-log-messages-from-your-code> and <https://developer.apple.com/documentation/os/logger> |
| Required-reason APIs | <https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api> |
| Signposts | <https://developer.apple.com/documentation/os/recording-performance-data> |
| String Catalogs | <https://developer.apple.com/documentation/xcode/localizing-and-varying-text-with-a-string-catalog> |
