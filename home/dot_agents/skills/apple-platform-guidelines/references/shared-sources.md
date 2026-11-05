# Shared platform sources

## Feedback and accessibility

Use [feedback](https://developer.apple.com/design/human-interface-guidelines/feedback), [alerts](https://developer.apple.com/design/human-interface-guidelines/alerts), and [writing](https://developer.apple.com/design/human-interface-guidelines/writing) to choose passive status, recoverable failures, and action labels.
Explain an interrupted task when a person must act, and keep recovered failures silent when visible behavior stays unchanged.
Use the platform's standard Cancel label.

Read [accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility), [typography](https://developer.apple.com/design/human-interface-guidelines/typography), and [Dark Mode](https://developer.apple.com/design/human-interface-guidelines/dark-mode).
Use each platform's text styles and accessibility mechanisms.
Test actual controls, focus, large text, and contrast rather than inferring accessibility from a palette or a screenshot.
Don't transfer iOS control sizes or Android density-independent pixel measurements to macOS.

## Privacy and diagnostics

[Privacy manifest files](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files) require data-collection information for apps and SDKs on all platforms.
The same page names iOS, iPadOS, tvOS, visionOS, and watchOS for required-reason API declarations.
Don't describe that platform list as a macOS required-reason mandate.
Read the current [SDK requirements](https://developer.apple.com/support/third-party-SDK-requirements/) and [required-reason API documentation](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api) for the actual dependency and distribution route.
Match declarations to shipped code and dependencies, and add the manifest to bundle resources.
A project's voluntary macOS manifest remains a project choice.

Use [Logger](https://developer.apple.com/documentation/os/logger) and Apple's [logging guidance](https://developer.apple.com/documentation/os/generating-log-messages-from-your-code).
Prefer fixed event names, processing states, and non-sensitive measurements.
Avoid logging sensitive payloads or arbitrary framework error descriptions.
Use [signposts](https://developer.apple.com/documentation/os/recording-performance-data) with an operation identity to measure intervals, and keep their metadata subject to the same privacy boundary.

Use a [String Catalog](https://developer.apple.com/documentation/xcode/localizing-and-varying-text-with-a-string-catalog) for user-facing text and plural variations when the app needs centralized copy.
Record a reason when a project omits one.

## Networking

Read [URLSession](https://developer.apple.com/documentation/foundation/urlsession), [waitsForConnectivity](https://developer.apple.com/documentation/foundation/urlsessionconfiguration/waitsforconnectivity), and [expensive network access](https://developer.apple.com/documentation/foundation/urlrequest/allowsexpensivenetworkaccess) for the actual transfer.
Reuse sessions for related tasks and choose connectivity waiting according to whether a person initiated the transfer.
Keep development-only [local networking exceptions](https://developer.apple.com/documentation/bundleresources/information-property-list/nsapptransportsecurity/nsallowslocalnetworking) out of distribution configurations.
An app without networking needs no speculative networking capability.
