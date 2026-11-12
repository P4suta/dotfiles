---
name: liquid-glass-design
description: >-
  Design and build an Apple-platform interface on Apple's system design with Liquid Glass, from Apple's kit in Figma through standard SwiftUI components to source checks and accessibility audits.
  Use when the user explicitly chooses Apple's system design for an artifact; monochrome-design remains the default otherwise.
---

# Apple system design

Use this design when the user explicitly chooses Apple's system design for an artifact.
The monochrome-design skill states that explicit requirements for the current artifact take precedence, so the choice selects this design for that artifact only.
Keep monochrome-design for every other visual task.
Record the choice in the project's instructions and in an ADR, so later work keeps the system design instead of the default.
Use `apple-platform-guidelines` to read and cite Apple's pages, and `source-bound-interface-design` for structure and copy.

## Principles

Read Apple's current pages before designing, and cite each page beside the rule that applies it.
The [adoption guide](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass) says that standard components take the material when the app builds with the current SDK.
Build every screen from standard components, and let the system draw bars, sheets, menus, alerts, and the keyboard.
Remove custom backgrounds from bars and sheets, and add no glass effect to content.
The [materials guidance](https://developer.apple.com/design/human-interface-guidelines/materials) keeps Liquid Glass out of the content layer.
Use system colors and one accent color, following the [color guidance](https://developer.apple.com/design/human-interface-guidelines/color).
Use system text styles so that text follows Dynamic Type, and keep the standard metrics, radii, and margins.
A request to match Apple's own apps calls for fewer custom parts, not for more effects.
Add a custom part only when a cited page asks for one.

## Design the screens first

Design the system and every screen before writing interface code, and show both to the user.
Start from Apple's kit for the target release, and keep the project's own layer thin.
Read the [kit reference](references/figma-kit.md) for the kit's structure, measurements, and limits.
Ask the user for each decision that the pages leave open, such as the accent color, the deployment target, and a new control.
Compute the contrast of the accent as text in both appearances before proposing the color.

## Build with SwiftUI

Map each screen to standard components.
A tabbed app takes `TabView` with `Tab`, `NavigationStack`, `List` or `Form` with `Section`, toolbar items with symbols, and sheets with detents.
Empty states take `ContentUnavailableView`, confirmations take `confirmationDialog` or `alert`, and charts take Swift Charts with the tint.
Give a sheet a cancelling action at its leading edge and a confirming action at its trailing edge through button roles.
Offer each row action through both `swipeActions` and a `contextMenu`.
Define the accent as an asset catalog color that references a system color, so the accent adapts to both appearances and to Increase Contrast.
Compare each API with the deployment target in [API availability](references/api-availability.md).
Build the icon as layers in Icon Composer, and render each appearance before accepting the icon.

## Enforce in source

Add a source check to the project's gate, because the compiler accepts every departure from the design.
Refuse colors authored in code, fixed hues, custom bar and sheet backgrounds, a fixed appearance, and glass effects in content.
Refuse fixed font sizes, modifiers that cut text short, and the compatibility setting that turns the design off.
Refuse the hierarchical foreground styles such as `.secondary`, and use the system colors, for the reason in [audit behavior](references/audit-behavior.md).
Plant a violation of each rule in the check's own tests.

## Verify on a simulator or a device

Drive every screen in a UI test, and run the accessibility audit on each screen.
Run that tour in both appearances, in each language, at the largest text size, and with Increase Contrast.
Read [audit behavior](references/audit-behavior.md) before accepting a report.
Accept only a report that the system's own drawing causes, state the reason beside the exception, and keep every other report fatal.
Look at the screenshots of the largest text size, because the audit missed chart labels that overlapped and row text that took the tint.
Compare screenshots of the app with the Figma frames.
Report each release, accessibility setting, and device that the checks left out as unverified.
