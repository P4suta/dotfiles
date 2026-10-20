---
status: accepted
date: 2026-10-20
---
# Keep parallel worktrees independent

## Context

Agents work in several worktrees of this repository at once.
Hook gates in every worktree built in one Cargo target directory.
Cargo names a workspace's artifacts by workspace-relative paths and checks freshness by file times, so one worktree reused another worktree's stale build of a shared crate, and a commit failed on a field that existed only in a sibling branch.
The [stale-tool refusal](0022-refuse-tools-built-from-stale-sources.md) also refused the shared `pr-workflow` in every worktree but the one that built it, so worktrees kept reinstalling it over each other.

## Decision

Each checkout's hook gates build in their own directory outside the worktree, named by the checkout's directory and the start of its [Source root partition key](partition-cargo-artifacts-by-physical-source-root.md).
An owner marker keys the directory to its checkout, so storage-scout reaps it once the checkout disappears and never while the checkout exists.
An absolute `CARGO_TARGET_DIR` still wins.

An installed `pr-workflow` or `coderabbit` that runs inside a dotfiles working copy whose sources differ from its build builds that copy's sources and runs the build with the same arguments and exit status.
The handed run carries a marker, so a build that still lags refuses instead of handing the run on again.
A checkout named by `DOTFILES_SOURCE` and the checkout that built the tool keep the refusal, and every other repository keeps the installed binary.

## Consequences

A worktree's first hook run builds the workspace once in its own directory, and later runs reuse it.
The first command in a changed worktree waits for a debug build of the tool.
Decisions inside a dotfiles checkout always come from that checkout's sources, as before.
The earlier shared directory `~/.cache/dotfiles-target` stays on disk until the owner removes it, because nothing claims it.
