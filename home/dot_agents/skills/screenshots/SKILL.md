---
name: screenshots
description: >-
  Capture screenshots of user-visible changes and attach them to personal pull requests and issues.
  Use when a change alters a web page, an app screen, or terminal output that a reader would want to see.
metadata:
  tools: "tools.toml"
---

# Screenshots

Show each user-visible change you can capture as a screenshot in its issue or PR.
Use [diagram-explanations](../diagram-explanations/SKILL.md) for complementary explanations of behavior or relationships.
Capture the screen before the change and after it at the same size and state, so a reader can compare the two.
Leave a capture out only when no tool here reaches the screen, and name the screen that the document lacks.

## Capture

| Screen | Command |
| --- | --- |
| Web page | `shot-scraper URL --width 1280 --height 800 --output after.png` |
| One element of a page | `shot-scraper URL --selector CSS --output after.png` |
| Terminal output | `freeze --execute "COMMAND" --output after.png` |
| iOS Simulator | `xcrun simctl io booted screenshot after.png` |
| Android emulator | `adb exec-out screencap -p > after.png` |
| macOS window | `screencapture -x -l WINDOW_ID after.png` |

Run `shot-scraper install` once on a host before its first web capture.
A browser automation capability that saves a screenshot to disk serves as well.
Capture a local development server, a fixture, or a test build.
Never capture a screen that shows credentials, private data, or another person's account.
Crop to the part that changed when the rest of the screen distracts from it.

## Publish

In a personal non-fork repository, run `pr-workflow screenshot --repo OWNER/REPO --file after.png --alt TEXT` for each image.
It prepares a local image reference that the checked document command attaches without creating a storage branch.
Describe in the alternative text the screen and the state that the image shows.
Put the lines from before and after the change next to each other under a heading that names the screen.

## Verification

Open each prepared image and confirm that it shows the intended screen.
Read back the published body and confirm that every image renders.
