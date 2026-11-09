# API availability by iOS release

These facts come from the iOS 27.2 simulator SDK of Xcode 27.2, read on 2026-11-09.
Read the installed SDK again before a decision depends on one of them.

Documentation and samples show the newest API, and the oldest supported release lacks part of it.
Read availability from the module interfaces of the SDK before a design depends on a declaration.
SwiftUI splits its declarations between the `SwiftUI` and `SwiftUICore` modules.
An attribute names a release as `iOS 27.0` or as `anyAppleOS 27.0`, so search for both forms.

## First available in release 26.0

The glass API starts here: `glassEffect`, `GlassEffectContainer`, `glassEffectID`, `glassEffectUnion`, and the `glass` and `glassProminent` button styles.
The button roles `confirm` and `close` start here, with `navigationSubtitle`, `ToolbarSpacer`, and `DefaultToolbarItem`.
The same release adds `tabBarMinimizeBehavior`, `tabViewBottomAccessory`, `searchToolbarBehavior`, `scrollEdgeEffectStyle`, and `backgroundExtensionEffect`.

## First available in release 27.0

`TabRole.prominent`, `ToolbarItemPlacement.topBarPinnedTrailing`, and `presentationPlacement` need iOS 27.0.
The same holds for `dismissalConfirmationDialog`, `BorderedTextFieldStyle`, and the `swipeActions` form that takes `onPresentationChanged`.

## Limits of the compiler

`toolbarBackground` and `presentationBackground` carry no deprecation, so the compiler accepts a custom background on a bar or a sheet.
Only a source check refuses one.

A simulator runs the release of its runtime.
Report the deployment target's own release as unverified until a device or a runtime of that release runs the app.

## Icon rendering

The Icon Composer app inside Xcode holds an `ictool` that exports an image of each appearance from an `.icon` file.
`xcrun ictool` resolved to another tool that refused the same arguments.
XcodeGen takes an `.icon` file as a resource from release 2.45.1.
