---
date: 2026-11-05
status: accepted
---

# Route desktop decisions through macOS guidance

## Status

Accepted.

## Context

The shared Apple skill applies mobile background execution and launch-screen rules without a platform boundary.
Desktop work also needs decisions about settings scenes, screen capture, permission recovery, and sandbox migration.
Apple's [privacy manifest documentation](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files) names different platform scopes for data collection and required-reason APIs.

## Decision

Keep source selection, availability, privacy, diagnostics, and evidence boundaries in `apple-platform-guidelines`.
Route desktop work to `macos-platform-guidelines` and keep mobile lifecycle details in a separate reference.
Use topic references for desktop interaction, capture and sandboxing, and verification and distribution.
Distinguish Apple requirements, Apple recommendations, and project choices beside their sources.

Compare Android guidance by user outcome and failure condition.
Permission denial, readable controls, and private diagnostics provide relevant questions for both platforms.
Their APIs and measurements keep their own platform scope.
The macOS references record the comparison and its primary sources without changing the Android skill.

## Consequences

Desktop decisions can load the relevant sources without applying UIKit lifecycle APIs.
The common entrypoint routes to macOS guidance, and the Claude Code alias targets the same canonical skill tree that Codex and OpenCode discover.
The Mac installation applies only the selected skills and aliases.
Experimental maintenance stays off.
The earlier common guidance had no separate desktop reference.
