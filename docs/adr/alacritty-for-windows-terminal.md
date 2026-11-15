---
status: accepted
date: 2026-11-15
---
# Use Alacritty for the Windows terminal

## Context

The user selected Alacritty because herdr manages terminal sessions.
The Windows profile installs WezTerm nightly through a dedicated setup step and removes Alacritty configuration as a retired target.

## Decision

Install Alacritty through the existing Scoop extras bucket and restore its Windows configuration target.
Start herdr through PowerShell using Alacritty's [shell configuration](https://alacritty.org/config-alacritty.html#terminal).
Keep the terminal font and navigation bindings, and apply the shared appearance policy for black and white.
Remove the WezTerm setup step and retire its configuration, command shim, shortcut, and portable installation.
Keep other profiles' terminal choices unchanged.

## Alternatives

Keeping WezTerm would preserve its integrated session management but would contradict the requested terminal choice.
Installing Alacritty manually would require separate update, command shim, and shortcut management that the existing Scoop setup already provides.

## Consequences

Scoop verifies the release checksum and manages Alacritty updates and Windows launchers.
Session management remains in herdr.
Reconsider the terminal choice if Alacritty cannot support the required herdr interaction on Windows.

## Confirmation

The user explicitly selected Alacritty with herdr.
The installed Alacritty loads its managed configuration and starts herdr.
The unused WezTerm installation and configuration were backed up before removal.
