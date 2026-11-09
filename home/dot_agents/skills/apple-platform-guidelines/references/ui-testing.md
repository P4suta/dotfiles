# Interface tests on a simulator

These observations come from Xcode 27.2 and an iOS 27.0 simulator runtime on 2026-11-09.
Check each one again before a decision depends on it.

## Environment

`simctl ui DEVICE appearance` and `simctl ui DEVICE increase_contrast` switch the appearance and Increase Contrast.
`XCUIDevice` didn't change the appearance on that simulator, so the test command switches both around each pass.
The command restores the defaults after a failed pass as well.
`simctl ui` has no option for Reduce Transparency or Reduce Motion, so report both as unverified.
Launch arguments choose the language, the region, and the text size of one test.
Xcode 27.2 holds no Simulator app, so `open -a Simulator` fails there.
A simulator runs the release of its runtime.
The deployment target's own release stays unverified until a runtime or a device of that release runs the app.

## Elements

Drive the screens by accessibility identifier, so one scenario runs in every language.
An identifier on a container replaces the identifiers of its children.
Give the label and each action of a `ContentUnavailableView` an identifier of its own.
A `LabeledContent` as the label of a button in a list gave the button an empty accessibility label.
A confirmation that opens from its button shows no button with the cancel role on an iPhone.
A list makes its rows on demand, so a row under the first screenful doesn't exist until the list scrolls to it.
At the largest text size, require an element near the top of a screen and scroll before checking the rest.

## System screens

The system's file picker belongs to another process.
The picker opens a file from a tap on the file's cell, and a tap on the name alone left the picker open.
The picker asks before it replaces a file, and SpringBoard owns that question.
Find its buttons through `XCUIApplication(bundleIdentifier: "com.apple.springboard")`.

## Accessibility audits

`performAccessibilityAudit` calls its handler for each report, and the handler returns `true` to accept one.
Fail on every report by default, and state the reason beside each accepted kind.
A report without an element names no view, and neither the test nor Xcode's report can find one.
Run the same audit on one of Apple's apps with the same components to learn which reports the system causes.
Read the audit notes of [liquid-glass-design](../../liquid-glass-design/references/audit-behavior.md) for the reports that standard components get.

## String Catalog in a package

A package target takes a String Catalog as a processed resource, with a default localization in the manifest.
`swift test` on a Mac builds the catalog, so package tests read every language through `Bundle.module`.
A test that parses the catalog file can require each key in each language.
