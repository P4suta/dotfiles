---
name: dotfiles
description: >-
  Maintain and apply personal dotfiles with matching behavior on Mac, Linux, and Windows using each machine's native chezmoi profile.
  Use for dotfiles changes, tool setup, and per-machine rollout, not runtime-state synchronization or unrelated system administration.
---

# Dotfiles across machines

A shared dotfiles change targets the owner's Mac, Linux, and Windows hosts unless the user narrows it.
Keep the intended behavior on each host, with its own paths, executables, shell syntax, and installation methods.
For a change specific to one OS, state why the other hosts need nothing.

Read `multi-machine` and the current `domyjob` manual before inspecting or operating another host.
Use `mise` for tool versions and `portable-skills` for shared agent skills and client discovery.
Keep one reviewed source change in the dotfiles repository, never separate configuration copies.

## Inspect and prepare

On each affected host, identify the OS and architecture, chezmoi source path, configured profile, and relevant installed tools.
Read the source and destination changes before applying them, including local edits, diverged branches, template data, and `.chezmoiignore` behavior.
Keep unrelated local work, and resolve a stale source or conflicting profile without resetting the checkout or replacing the host's whole configuration.
Express shared intent in templates and data, with platform conditions only where the behavior needs them.
Verify that each host renders its own paths and role.

## Apply and verify

Use `domyjob run` from the intended checkout for the current source snapshot, and `domyjob on` to inspect the host's existing installation.
Preview the affected targets in the host's native chezmoi context, and apply only the intended paths and setup actions.
Install shared skills through the scoped installer, and verify the canonical files and native client discovery entries on every affected host.
Keep temporary snapshot paths out of permanent aliases, service configuration, and command entry points.
Never turn a focused change into a full profile apply, bootstrap, system provisioning, or remote agent dispatch.

Keep credentials, authentication, shell history, caches, review ledgers, and other runtime state on their own machine, and never copy them between hosts.
Report a needed owner login as pending.

Run the affected checks on the Mac, Linux, and Windows hosts with the existing project gates before an authorized push.
After applying, inspect the installed files and exercise the native command or discovery behavior without spending service allowance.
Distinguish a rendered preview, a passing build, an applied configuration, and a verified installed runtime in the report.
Report a failed or unreachable host as pending, and keep durable job identities while resolving it.
Follow `coderabbit-review` for the final review.
