---
name: dotfiles
description: >-
  Maintain and apply personal dotfiles with equivalent behavior on Mac, Linux, and Windows using each machine's native chezmoi profile.
  Use for dotfiles changes, tool setup, and per-machine rollout, not runtime-state synchronization or unrelated system administration.
---

# Dotfiles Across Machines

Treat a shared dotfiles change as work for the owner's Mac, Linux, and Windows unless the user narrows its scope.
Preserve the intended behavior on each machine while allowing OS-specific paths, executables, shell syntax, and installation methods.
For an OS-specific change, explain why the other machines are unaffected rather than applying an inappropriate configuration there.

Read `multi-machine` and the current `domyjob` manual before inspecting or operating another host.
Use `mise` for tool versions and `portable-skills` for shared agent skills and client discovery.
Keep one reviewed source change in the dotfiles repository; do not create separate configuration copies that can drift.

## Inspect and prepare

On each affected host, identify the native OS and architecture, actual home directory, chezmoi source path, configured profile, and relevant installed tools.
Read the source and destination changes before applying them, including local edits, ahead or behind branches, template data, and `.chezmoiignore` behavior.
Preserve unrelated local work and resolve a stale source or conflicting profile without resetting the checkout or replacing the host's full configuration.
Express shared intent in templates and data, with platform-specific conditions or artifacts only where the behavior requires them.
Verify that each host renders its own paths and role instead of inheriting the Mac's rendered files.

## Apply and verify

Use `domyjob run` from the intended checkout for the current source snapshot and `domyjob on` to inspect the host's existing installation.
Preview the affected targets with that host's native chezmoi context and apply only the intended paths and necessary setup actions.
Apply shared skills through the maintained scoped installer and verify the canonical files and native client discovery entries on every affected host.
Keep temporary snapshot paths out of permanent aliases, service configuration, and command entry points.
Do not turn a focused change into a full profile apply, bootstrap, system provisioning, or remote agent dispatch.

Keep credentials, authentication, shell history, caches, review ledgers, and other runtime state local to each machine.
Do not copy these between hosts or reset them to make installation pass.
Use each host's existing authenticated session when relevant; report a required owner login as pending rather than transferring another machine's credentials.

Run the affected checks on the actual Mac, Linux, and Windows before an authorized push, using the existing project gates.
After applying, inspect the installed files and exercise the native command or discovery behavior without spending service allowance merely to test setup.
Distinguish a rendered preview, a passing build, an applied configuration, and a verified installed runtime in the completion report.
Report a failed or unavailable required host as pending and retain durable job identities while resolving it.
Preserve the user's commit, push, PR, and merge authorization and follow `coderabbit-review` for the final PR review.
