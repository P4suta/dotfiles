---
status: accepted
date: 2026-10-20
---
# Keep every managed behavior the same on every host

## Context

The owner works on a Mac, a Linux host, and a Windows host, and moves between them during a day.
A tool, hook, or gate that acts differently on one host makes the owner remember which host ran it, which defeats one shared source.
[ADR 0011](0011-one-git-policy-on-every-host.md) removed one such split, after Windows skipped the Git hooks that the Mac and Linux ran.

## Decision

Every managed behavior acts the same on the Mac, Linux, Windows, and WSL.
Hosts differ only in paths, executables, shell syntax, and installation methods, and the shared source holds those differences as data or in one adapter.
A useful feature added for one host reaches every host in the same change, unless a platform lacks what the feature needs, and the change states that reason.
Hooks and gates run the same Rust code on every host, so a fix on one host fixes the others.

## Consequences

A change that works on one host only needs a stated platform reason before it merges.
The `dotfiles` skill asks for verification on each affected host.
