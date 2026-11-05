---
name: macos-platform-guidelines
description: >-
  Ground native macOS app decisions in Apple guidance for desktop interaction, screen capture, sandboxing, diagnostics, and distribution.
  Use for desktop app design and implementation; use the dedicated signing skills for release credentials and notarization.
---

# macOS platform guidelines

Apply [shared Apple rules](../apple-platform-guidelines/SKILL.md), and read the primary source for each desktop decision.
Identify the earliest supported macOS release, current SDK, supported architectures, and distribution route.
Distinguish a mandatory capability or API contract from an Apple recommendation and a project choice.
An API introduced in macOS 26 can't establish support for an earlier deployment target.

Read [desktop interaction](references/desktop-interaction.md) for windows, menu bar apps, settings, keyboard actions, typography, and accessibility.
Read [capture and sandbox](references/capture-and-sandbox.md) for Screen Recording, screenshots, least privilege, and container migration.
Read [verification and distribution](references/verification-and-distribution.md) for diagnostics, native acceptance, manifests, and signed artifacts.

Use standard controls, system text styles, focus rings, selection, and window behavior when the project requests native appearance.
Keep standard title bars and window controls at their system-defined sizes and positions.
Don't shrink, redraw, rescale, or reposition system window buttons, including through a utility-window style chosen only to make a window float.
Keep app decisions in testable types and platform effects in thin adapters.
Preserve results when opening, copying, permission recovery, or relaunch fails.
Bind asynchronous completions to their operation generation, reject obsolete results, and stop work on cancellation.
Define and verify these project contracts without presenting them as universal framework behavior.

Before claiming support, test the actual deployment target in a normal login session.
Record the OS build, architecture, app identity, permission state, connected displays, accessibility settings, and observed outcome.
Proofs of a decision core can't establish screenshot fidelity, shortcut delivery, VoiceOver behavior, or sandbox compatibility.
