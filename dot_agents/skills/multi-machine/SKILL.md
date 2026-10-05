---
name: multi-machine
description: >-
  Validate and develop across the owner's Mac, Linux, and Windows machines through domyjob and protected bare-repository hubs.
  Use for cross-platform execution, machine synchronization, and remote development.
---

# Multi-machine Development

`dotfiles-xtask hosts sync` generates the machine-local SSH aliases and pinned host keys for one host per OS family from the tailnet.
`dotfiles-xtask hosts doctor` checks each host and prints the next action for any that is not ready.
Read the owner's machine-local repository roots and hub location before operating another machine.
Bare repositories under `~/git/` on the Mac are the synchronization source of truth.
Read the current `domyjob` project skill and installed command help before driving another machine.
Keep machine-specific paths and service configuration in dotfiles and existing inventories rather than copying current runtime state into this skill.
Use `dotfiles` for configuration changes that must be rendered, applied, and verified separately on the three hosts.

Run builds, tests, and remote commands through domyjob from the intended local checkout.
Use `run` when the command needs the current files, including uncommitted work, and `on` for an explicit command in the remote home directory.
Each invocation selects one actual host and returns a durable machine/job identity.
Pass a real executable and arguments after `--`; use an explicit host shell only when the requested expression needs it.
Follow the current snapshot size, filename, link, and environment rules rather than assuming the client sends an arbitrary home directory.

Before a push, run `dotfiles-xtask hosts check` on the clean commit; pre-push and `pr-workflow ready` refuse a GitHub branch until every OS family in its CI matrix has a passing note.
Exercise actual native paths and failure behavior; cross-compilation alone does not establish operating-system behavior.
Keep CI as confirmation of already exercised behavior and of hosted capabilities the local machines cannot reproduce.
Do not run signing, notarization, publication, privileged installation, or destructive cleanup merely to verify ordinary code.

For a long run, retain its job identity and inspect its status or logs without resubmitting it.
Preserve exit status and the primary failure, and distinguish a transport interruption from the job's actual result.
Use the submission identity to resolve an uncertain start instead of launching a duplicate operation.
The owner handles 1Password approval and private keys directly; do not open the vault, relax approval settings, or disable the agent's protections.

Preserve the hub workflow, machine branches, signatures, hooks, line endings, and current local work when synchronization is needed.
Use the project's ordinary branch and review flow; integration on the Mac does not authorize a release.
Never use `--no-verify`, skip variables, unsigned fallback commits, direct protected-branch writes, or ruleset bypass to overcome a failed gate.
Repair the failing gate or run it on its supported host before completing the authorized push.

Remote agent dispatch and messaging are separate from running a build.
Use domyjob chat, fleet, or herdr only when that agent work is explicitly authorized, and verify their current command contracts before using them.
Do not auto-spawn agents, accept a folder-trust prompt, or grant a writable remote agent merely because another host is available.
Complete independent local work while resolving an actual missing external prerequisite.
