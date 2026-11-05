# Desktop interaction

## Windows and menu bar apps

Use [windows](https://developer.apple.com/design/human-interface-guidelines/windows) and [panels](https://developer.apple.com/design/human-interface-guidelines/panels) to choose primary and auxiliary surfaces.
Keep system-defined window states and native accessibility behavior.
The owner's default is a standard title bar with unchanged system window buttons and their default metrics and placement.
AppKit's [utilityWindow](https://developer.apple.com/documentation/appkit/nswindow/stylemask-swift.struct/utilitywindow) style uses a utility title bar.
Use a normal titled window or panel for the main task.
Floating behavior alone doesn't justify changing the window chrome.
Compare window buttons with a standard system window, including after text enlargement.
Verify resizing, displays, Spaces, focus, and reopening for the actual app.
An auxiliary panel that must disappear before capture can justify AppKit ownership without custom settings-window management.

Read [menu bar extras](https://developer.apple.com/design/human-interface-guidelines/the-menu-bar#Menu-bar-extras) and [MenuBarExtra](https://developer.apple.com/documentation/swiftui/menubarextra).
Apple recommends a menu for concise commands and warns that the system can hide extras.
Provide another way to reach the primary task when the app depends on an extra.
Document a deliberate menu-bar-only choice and its reopening or keyboard fallback.

## Settings and keyboard input

[Settings](https://developer.apple.com/documentation/swiftui/settings) lets SwiftUI manage settings and the standard menu command on macOS 11 or later.
[SettingsLink](https://developer.apple.com/documentation/swiftui/settingslink) opens that scene or brings its existing window forward on macOS 14 or later.
Prefer those APIs for a SwiftUI app over recreating settings-window ownership and Command-Comma handling.
Apple's [settings guidance](https://developer.apple.com/design/human-interface-guidelines/settings) places infrequent preferences in settings and task-specific actions in their task context.
Use the app name in a single-pane settings title and avoid redundant system appearance settings.

Follow [keyboard guidance](https://developer.apple.com/design/human-interface-guidelines/keyboards) for standard shortcuts and navigation.
Test Return, Escape, Tab, Shift-Tab, Command-C, and focused-button activation where relevant.
A global shortcut needs a contract for registration failure, conflicts, recording cancellation, and cleanup.
Display a shortcut as active only after registration succeeds.
Don't add Accessibility or Input Monitoring permission by analogy with another input API.
Read and test the selected API's own authorization contract.

## Typography and accessibility

Use [typography](https://developer.apple.com/design/human-interface-guidelines/typography) and [accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility) for desktop text styles, control sizing, and enlargement.
Apple recommends text enlargement of at least 200 percent and provides macOS-specific control measurements.
Use system colors and native controls that adapt to appearance and contrast preferences.
Verify narrow layouts and enlarged text without clipped labels or inaccessible actions.
Test VoiceOver labels and reading order, keyboard focus, selection, Increase Contrast, Reduce Transparency, and both appearances on the shipped controls.
Automated geometry and accessibility assertions complement human interaction checks.
