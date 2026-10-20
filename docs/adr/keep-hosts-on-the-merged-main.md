---
date: 2026-10-21
status: accepted
---
# Keep hosts on the merged main

## Context

Scheduled tool upgrades leave the applied dotfiles source behind merged changes.
SSH fetches require an agent that unattended sessions cannot use.
The applied source can be a worktree apart from the checkout that launches maintenance.

## Decision

GitHub HTTPS fetches use the `gh` credential helper, including owner repositories and setup clones.
`pushInsteadOf` keeps GitHub pushes on SSH.
The upgrade on every OS resolves the applied source through `chezmoi source-path` and its Git top-level directory.
The [pure admission rule](../../crates/rules/src/source_sync_rules.rs) permits movement only on main with a clean working tree and index after a successful fetch whose main descends from HEAD.
Dirty, off-main, diverged, and failed-fetch states skip the update and enter the session-start hook failure report.
An upgrade lock serializes this host's source updates, and the adapter rechecks the checkout after fetching.

The [runtime](../../crates/dotfiles/src/source_sync.rs) fetches origin's main over HTTPS, including when an older installed configuration still rewrites owner fetches to SSH.
It fast-forwards to the fetched revision and runs the existing verified profile apply with a fresh backup under `~/.local/state/dotfiles/backups`, then completes tool maintenance from the updated source.
It never resets, stashes, switches branches, or forces a ref update.
Failures enter the same hook report.

## Consequences

Managed files retain the existing apply rollback and changed-skill recording guarantees.
After an apply failure, the source remains fast-forwarded and a later upgrade retries application, including when HEAD already equals origin/main.
Local bare-remote integration tests exercise the Git and worktree boundaries with a stub application port.
Kani proves the finite admission states and rejects the registered counterexamples.
Git, the credential helper, and the existing verified apply remain external trust boundaries; concurrent manual checkout edits are not a cross-process transaction.

Switching a host's branch or discarding local work would interrupt development.
Resetting after a failed apply would discard source history and conflict with preservation of local work.
Leaving updates manual permits installed hosts to drift indefinitely.
