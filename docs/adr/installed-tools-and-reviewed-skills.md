---
status: proposed
date: 2026-10-08
---
# Bind installed tools to reviewed skills

## Context

A hand-written list of tools can miss an installation added to a profile.
A skill can keep a current content hash while describing an older tool version.
Issue 140 requires the inventory and the review binding to follow actual installations.

## Decision

`xtask::inventory` derives tools and versions from the typed profile data, the rendered modern tool pins, the development mise configuration, Brew, Nix, and setup steps.
`policy/tools.toml` names each setup step's installations and gives explicit decisions for tools that need no judgment-bearing skill.
The gate rejects missing step declarations, missing coverage, stale version bindings, and incompatible profile placement.

A skill's `tools.toml` records the versions its reviewed content covers.
The catalog hashes that resource with the skill, so updating a binding requires a new decision.
The prose continues to point at installed help and authoritative documentation, without duplicating version pins.
Every version in the inventory must be covered, including a floating installation alongside an exact development pin.
OrbStack's skill installs only on Mac.

## Verification

`just inventory` exposes and checks the computed inventory.
`xtask/tests/tool_inventory.rs` exercises missing coverage, changed pins, and profile placement.
`crates/rules/src/tool_rules.rs` owns the pure coverage and placement decisions, with unit tests, Kani harnesses, and rejecting probes in `policy/obligations.toml`.
