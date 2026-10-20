---
date: 2026-10-21
status: proposed
---
# Apply from the configured source and replace tools atomically

## Context

Permanent launchers render `.chezmoi.workingTree` as their runtime root.
Overriding `sourceDir` with another checkout during an apply gives those launchers the lifetime of that checkout.
Moving installed executables aside before building their replacements also leaves every agent hook without its runtime.

## Decision

The [profile apply](../../crates/dotfiles/src/profiles.rs) reads chezmoi's effective configuration without a source override before rendering or changing targets.
It accepts only the canonical configured checkout or its `home/` source state, which `.chezmoiroot` names.
Another checkout refuses with the configured source and the checkout to run from.
Previews retain their explicit checkout so a proposed change can still be inspected.
Refusing preserves the existing [checkout-bound validation](source-bound-push-validation.md) rather than silently applying a different revision from the one being inspected.

Cargo installations build in an owned staging root and validate every requested binary before publishing any replacement.
The [shared installer](../../crates/platform/src/aside.rs) copies each complete executable into a synced temporary file beside its destination, closes it, and renames it over the old executable.
Existing copy installers use the same replacement boundary.
Before the Rust runtime exists, the mise bootstrap stages on the destination filesystem and renames the verified download without removing the installed executable.
Old aside files remain eligible for cleanup, but replacement never moves the public destination away.

The filesystem's atomic rename and the pinned Rust standard library are external assumptions.
Rust 1.99.1 [uses POSIX-compatible replacement on supported Windows filesystems](https://doc.rust-lang.org/std/fs/fn.rename.html).
A filesystem that refuses replacement returns an error with the previous executable still installed.

## Consequences

Applying a worktree now requires explicitly configuring it as the source or running from the existing configured checkout.
Successful renders from that source retain their existing entry points.
Build and staging failures preserve installed executables, and concurrent readers observe a complete old or new file.
Each binary is replaced independently; a package installation is not a transaction across multiple binaries.
The [source regression](../../crates/dotfiles/src/profiles.rs), [installation regressions](../../crates/dotfiles/src/setup.rs), and [native replacement tests](../../crates/platform/tests/aside.rs) cover these boundaries.
The source admission rule has exhaustive unit tests, a Kani harness, and a rejecting probe in the proof inventory.
