---
name: multi-machine
description: >-
  Test and develop across the owner's Mac, Linux, and Windows machines through domyjob and protected bare-repository hubs.
  Use for cross-platform execution, machine synchronization, and remote development.
---

# Multi-machine development

Read the owner's machine-local SSH aliases, host identities, repository roots, and hub location before operating another machine.
Bare repositories under `~/git/` on the Mac hold the synchronization source of truth.
Read the current `domyjob` project skill and installed command help before driving another machine.
Keep machine-specific paths and service configuration in dotfiles and existing inventories, never in this skill.
Use `dotfiles` for configuration changes that each of the three hosts must render, apply, and verify.

Run builds, tests, and remote commands through domyjob from the intended local checkout.
Use `run` when the command needs the current files, including uncommitted work, and `on` for an explicit command in the remote `$HOME`.
Check host readiness with `domyjob doctor MACHINE` before relying on it.
Each invocation selects one host and returns a durable machine and job identity.
Pass a real executable and arguments after `--`, and use an explicit host shell only when the expression needs it.
Follow the current snapshot size, filename, link, and environment rules, because the client never sends an arbitrary `$HOME` tree.

Before push, run the affected checks on the local Mac and on the owner's Linux and Windows hosts when the project supports them.
Exercise native paths and failure behavior, because cross-compilation proves nothing about operating-system behavior.
Reuse the project's maintained tasks and persistent build-cache conventions.
Keep CI as confirmation, with its required checks and any hosted capabilities the local machines lack.
Restore an unavailable host when the change needs its platform, and never submit a known unverified platform change.
Never run signing, notarization, publication, privileged installation, or destructive cleanup to verify ordinary code.

For a long run, keep its job identity and inspect its status or logs without resubmitting it.
Preserve exit status and the primary failure, and tell a transport interruption apart from the job's result.
Resolve an uncertain start through the submission identity instead of launching a duplicate.
Diagnose network reachability through `tailscale` and authentication through the configured 1Password SSH key boundary.
The owner handles 1Password approval and private keys, so never open the vault, relax approval settings, or turn off its key protections.

Preserve the hub workflow, machine branches, signatures, hooks, line endings, and local work during synchronization.
Use the project's ordinary branch and review flow, because integration on the Mac grants no release permission.
Never use `--no-verify`, skip variables, unsigned fallback commits, direct protected-branch writes, or ruleset bypass to pass a failed gate.
Repair the failing gate or run it on its supported host before the authorized push.

Running a build differs from dispatching or messaging remote coding sessions.
Use domyjob chat, fleet, or herdr only for explicitly authorized session work, and check their current command contracts first.
Never start sessions automatically, accept a folder-trust prompt, or grant a remote session write access just because another host exists.
Complete independent local work while resolving a missing external prerequisite.
