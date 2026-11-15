# Apple's design kit in Figma

These notes describe the kit named iOS and iPadOS 27 and Figma's remote plugin interface on 2026-11-09.
Read the kit and the interface again before relying on a value.

## Kit

Apple's [design resources](https://developer.apple.com/design/resources/) link the kit as a Figma community file.
A file imports the kit's components, variables, and text styles by key.
Read the license in the kit before any use other than a mockup of software for Apple's platforms.

An iPhone screen in the kit measures 402 by 874 points.
The status bar takes 62 points.
The top toolbar sits under the status bar at 54 points, or at 125 points with a large title.
Each toolbar holds its buttons in a leading slot and a trailing slot.
The tab bar measures 402 by 95 points and starts at y 779.
A grouped section takes a 26-point corner radius and the secondary grouped background.

The kit holds a `Color Scheme` collection with a light mode and a dark mode.
It holds a `Dynamic Type` collection with twelve modes up to AX5.
It holds no mode for Increase Contrast.
Give a screen frame its mode instead of drawing a second palette.
The kit's accent resolves to system blue.
Bind each accent paint to one local variable that refers to the chosen system color.

## File structure

Keep the project's own layer thin.
Local variables refer to the kit's colors for the accent, a chart series, and the few roles that only the project has.
Local components cover only what the kit lacks, and each description names the SwiftUI type that implements the component.
Bars, sheets, alerts, keyboards, pickers, and standard rows stay instances of the kit.
Draw every screen and state in the light and the dark mode.
Draw the largest text size and each language for the screens with the most text.
Type each symbol as an SF Symbols character in SF Pro instead of drawing a vector.

## Limits

A script loads SF Pro and SF Compact before editing a kit instance.
Hiragino Sans and PingFang weren't available to the interface.
Japanese and Chinese text took Noto Sans, and the file's cover says so.
A dark copy of a screen lost its instance overrides when its mode changed.
Copy the text, the visibility, and the properties from the light frame again after the change.
The dark keyboard rendered the wrong keys through the interface, so keyboard states stayed light.
An instance of the scroll edge effect arrived at 200 by 200 points and needed the width of the screen.

Figma's [rate limits](https://developers.figma.com/docs/figma-mcp-server/rate-limits-access/) give some seats a monthly allowance of read calls.
A Collab seat on an education plan received 20 a month.
Dozens of script calls on the same seat ran in one day without a refusal.
Spend the read calls on library discovery and record the returned keys.
Inspect the file through scripts and the images that a script renders.
