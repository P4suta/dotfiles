---
date: 2026-10-20
status: accepted
---
# Leave the Cargo target to each workspace

## Context

[Keep parallel worktrees independent](keep-parallel-worktrees-independent.md) gave each checkout's hook gates a build directory outside the worktree and exported it as `CARGO_TARGET_DIR` to every command a hook ran.
A child that copies the tree and builds the copy inherits that variable.
A cargo-mutants campaign in a repository's push check built every mutant into the checkout's hook directory.
Cargo names a workspace's artifacts by workspace-relative paths and checks freshness by file times, so the next hook run tested a mutated library and failed, and the same mechanism could pass a broken change.

## Decision

Hook gates leave `CARGO_TARGET_DIR` as the caller set it.
Each build runs in its own workspace's `target` directory, so a copied tree builds in its own and a removed worktree takes its build along.
A hook run removes the directory where earlier hook gates of its checkout built, once the owner marker shows that checkout claimed it.

## Consequences

A checkout's hook gates and its own Cargo commands share one `target` directory.
On Windows, hook builds of a checkout outside the development drive no longer use that drive.
A command that copies the tree no longer needs to clear `CARGO_TARGET_DIR` to stay isolated.
