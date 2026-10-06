---
name: multi-machine
description: >-
  Validate and develop across the owner's Mac, Linux, and Windows machines through domyjob and protected bare-repository hubs.
  Use for cross-platform execution, machine synchronization, and remote development.
---

# Multi-machine development

`dotfiles-xtask hosts sync` generates the machine-local SSH aliases and pinned host keys for one host per OS family from the tailnet.
`dotfiles-xtask hosts doctor` checks each host and prints the next action for any host that fails.
Read the owner's machine-local SSH aliases and hub location before operating another machine.
Bare repositories under `~/git/` on the Mac hold the synchronization source of truth.
Read the current `domyjob` project skill before driving another machine.
Keep machine-specific paths in dotfiles and existing inventories, never in this skill.
Use `dotfiles` for configuration changes that each of the three hosts must render, apply, and verify.

Run builds, tests, and remote commands through domyjob from the intended local checkout.
Use `run` when the command needs the current files, including uncommitted work, and `on` for an explicit command on the remote host.
Check host readiness with `domyjob doctor MACHINE` before relying on it.
Pass a real executable and arguments after `--`, and use an explicit host shell only when the expression needs it.
Follow the current snapshot size, filename, link, and environment rules.

Before push, run `dotfiles-xtask hosts check` on the clean commit.
Pre-push and `pr-workflow ready` refuse a GitHub branch until every OS family in its CI matrix has a passing note.
Exercise native paths and failure behavior, because cross-compilation proves nothing about operating-system behavior.
Keep CI as confirmation, with its required checks.
Restore an unavailable host when the change needs its platform.
Never run signing, notarization, publication, privileged installation, or destructive cleanup to verify ordinary code.

For a long run, keep its job identity and inspect its status or logs without resubmitting it.
Preserve exit status and the primary failure, and tell a transport interruption apart from the job's result.
Resolve an uncertain start through the submission identity instead of launching a duplicate.
Diagnose network reachability through `tailscale` and authentication through the configured SSH-agent boundary.
Never open the 1Password vault, relax approval settings, or turn off its key protections.

Preserve the hub workflow, machine branches, signatures, hooks, line endings, and local work during synchronization.
Integration on the Mac grants no release permission.
Never use `--no-verify`, skip variables, unsigned fallback commits, direct protected-branch writes, or ruleset bypass to pass a failed gate.
Repair the failing gate or run it on its supported host before the authorized push.

Use domyjob chat, fleet, or herdr only for explicitly authorized session work.
Never start sessions automatically, accept a folder-trust prompt, or grant a remote session write access just because another host exists.
