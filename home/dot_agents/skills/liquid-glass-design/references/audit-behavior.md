# Audit behavior on standard components

These observations come from Xcode 27.2 and an iOS 27.0 simulator runtime on 2026-11-09.
Repeat the probe in the last section before relying on one of them.

## Passes

Run the screen tour in the light appearance, in the dark appearance, and in both with Increase Contrast.
Run the tour in each language, and at the largest text size in the languages that the app's people read.
`apple-platform-guidelines` describes how a test command switches the settings of a simulator.
Report Reduce Transparency and Reduce Motion as unverified when no switch reaches them.

## Contrast

At the default contrast, the audit reports secondary text, section headers, and footers on grouped backgrounds.
The system's secondary label color reaches about 3.5:1 on a white row.
Apple's Settings app got 11 contrast reports on its About screen at the default contrast and none with Increase Contrast.
The [accessibility guidance](https://developer.apple.com/design/human-interface-guidelines/accessibility) asks an app that misses its contrast ratios by default to reach them with Increase Contrast.
The same page asks for the check in both appearances.
Leave the contrast audit out of the passes at the default contrast, and run it in a light and a dark pass with Increase Contrast.

A hierarchical style such as `.secondary` derives from the foreground style around it.
With Increase Contrast it stayed at 4.0:1 on a white row, where `Color.secondary` reached 6.0:1.
Inside a button, `.primary` took the tint.
Use `Color.primary` and `Color.secondary` for text, and refuse the hierarchical styles in the source check.

## Reports that the system causes

Fail on every report by default, and accept one only for a reason that the handler states.

Some contrast reports carry no element, and neither the test nor Xcode's report finds the view.
Settings got such reports on its first screen in every pass.
In one app their number stayed the same when a trial build exposed every text that the app merges or hides.
Run that trial before accepting such reports, because merged text of the app itself also arrives without an element.

The audit reports partial Dynamic Type support for the calendar of a date picker and for some text in system styles inside a list.
Settings got that report for its own title.
Keep a report of no support as a failure.

Text under a floating bar fails the contrast audit through the bar's material.
The first header of a form in a sheet at the medium height failed the same way, and a screenshot measured 5.5:1.
Measure the pixels of a screenshot before accepting such a report.

At the default text size, the audit of clipped text predicts clipping for headers and footers that the largest size shows whole.
Run that audit in the passes at the largest text size, and refuse `lineLimit`, `minimumScaleFactor`, and `truncationMode` in the source check.

Text without an element belongs to the system where a screen shows a chart axis, a calendar, or the screen under a sheet.
Accept such text on those screens by name, and give each chart mark a spoken label and value.

## Probe of a system app

Before accepting a report, run the same audit on one of Apple's apps that uses the same components.
A UI test launches Settings through `XCUIApplication(bundleIdentifier: "com.apple.Preferences")`, and the handler lists every report.
Wait for a pushed screen to settle before the audit.
An audit during the transition reported 13 contrast failures that the settled screen lacked.
A report that Apple's own screens get comes from the component.
Keep the probe out of the project's gate, because its result changes with the system.
