---
date: 2026-11-04
status: proposed
---
# Preserve active tools behind managed guards

## Status

Proposed.

## Context

Hook workers inherit the environment selected by `mise exec`.
Prepending mise shims and package manager directories hides the selected executable versions.
A Windows hook can then run an older Scoop installation or a quarantined shim despite an available pinned executable.
Managed guard and installation directories must retain precedence, including in an isolated home.

## Decision

Keep `.local/bin`, `.cargo/bin`, and `.nix-profile/bin` for the requested home first.
Preserve the inherited path entries in their original order after these installations.
Append mise shims and platform package manager directories as fallbacks.
Continue removing retired launcher directories.
Use the shared [path constructor](../../crates/platform/src/home.rs) for every runtime consumer.

[mise exec](https://mise.jdx.dev/cli/exec.html) selects the active tool environment.
The runtime preserves that selection without launching another resolver or weakening hook gates.
Keeping every managed directory first would retain the executable shadowing.
Putting the inherited environment before managed guards would allow it to hide the guards.
Removing shims would prevent fallback discovery in a shell without active mise tools.

## Consequences

The change applies to Mac, Linux, Windows, and WSL through the shared constructor.
Explicit managed installations still take priority over project tools.
The inherited environment remains a trusted input supplied by the invoking process.
The [path regression tests](../../crates/platform/src/home.rs) cover guard precedence, active tool precedence, fallback retention, and retired launcher exclusion.
Operating system executable lookup and mise environment construction remain external semantic boundaries.
Native hook execution checks the installed runtime against the actual repository pins.
These checks don't prove either external implementation.
