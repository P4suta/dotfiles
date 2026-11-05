---
name: apple-platform-guidelines
description: >-
  Ground Apple-platform app decisions in primary guidance and route platform-specific work to the relevant references.
  Use for shared interface, privacy, networking, diagnostics, and distribution decisions; use macos-platform-guidelines for desktop behavior.
---

# Apple platform guidelines

Read Apple's current primary source before choosing app behavior.
Cite the exact page beside the decision in an issue, PR, design record, or source comment.
Distinguish platform requirements, Apple recommendations, and project choices.
Check the deployment target, SDK availability, and distribution channel for each rule.
Record deliberate departures with their reason and verification limits.

For macOS, load [macOS platform guidelines](../macos-platform-guidelines/SKILL.md).
For iOS, iPadOS, tvOS, watchOS, or visionOS, read [platform behavior](references/platform-behavior.md).
Read [shared sources](references/shared-sources.md) for feedback, privacy, logging, localization, or networking decisions.
Apply `source-bound-interface-design` and the owner's visual preferences within the current artifact's explicit requirements.
Prefer native system behavior when the artifact calls for standard platform colors, selection, materials, or accessibility accommodations.

Apple documentation supports a Markdown view at the page URL with `.md` appended.
If unavailable, read the DocC JSON at `https://developer.apple.com/tutorials/data/documentation/PATH.json`, or `https://developer.apple.com/tutorials/data/design/human-interface-guidelines/PAGE.json` for a design page.
Cite the human-readable page URL and report an unread source as unavailable.

When changing platform guidance, compare user outcomes and failure conditions with `android-platform-guidelines` when available.
Transfer questions about denial, recovery, cancellation, accessibility, and sensitive diagnostics only after reading the destination platform's primary source.
Keep platform APIs, lifecycle contracts, dimensions, and distribution exceptions with their own platform.
Record a concrete gap and its supporting sources without requiring a change when existing guidance already covers the outcome.

Use `formal-assurance` for deterministic properties and `development-assurance` for framework boundaries.
Distinguish proofs, host tests, simulator observations, and physical-device results.
Bind empirical evidence to the source, built artifact, OS release, architecture, permission state, and relevant device configuration.
Missing target-platform acceptance remains missing evidence.
