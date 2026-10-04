---
name: multi-machine
description: >-
  Validate and develop across the owner's Mac, Linux, and Windows machines through domyjob and protected bare-repository hubs.
  Use for cross-platform execution, machine synchronization, and remote development.
---

# Multi-machine Development

Read the owner's machine-local SSH aliases, host identities, repository roots, and hub location before operating another machine.
Bare repositories under `~/git/` on the Mac are the synchronization source of truth.
Read the current `domyjob` project skill and installed command help before driving another machine.
Keep machine-specific paths and service configuration in dotfiles and existing inventories rather than copying current runtime state into this skill.
Use `dotfiles` for configuration changes that must be rendered, applied, and verified separately on the three hosts.

Run builds, tests, and remote commands through domyjob from the intended local checkout.
Use `run` when the command needs the current files, including uncommitted work, and `on` for an explicit command in the remote home directory.
Check host readiness with `domyjob doctor MACHINE` before relying on it.
Each invocation selects one actual host and returns a durable machine/job identity.
Pass a real executable and arguments after `--`; use an explicit host shell only when the requested expression needs it.
Follow the current snapshot size, filename, link, and environment rules rather than assuming the client sends an arbitrary home directory.

Before push, run the checks affected by the change on the local Mac and the owner's Linux and Windows hosts when the project supports them.
Exercise actual native paths and failure behavior; cross-compilation alone does not establish operating-system behavior.
Reuse the project's maintained tasks and persistent build-cache conventions.
Keep CI as confirmation of already exercised behavior while retaining its required checks and any hosted capabilities the local machines cannot reproduce.
An unavailable host is a prerequisite to restore when its behavior is required, not a reason to submit a known unverified platform change.
Do not run signing, notarization, publication, privileged installation, or destructive cleanup merely to verify ordinary code.

For a long run, retain its job identity and inspect its status or logs without resubmitting it.
Preserve exit status and the primary failure, and distinguish a transport interruption from the job's actual result.
Use the submission identity to resolve an uncertain start instead of launching a duplicate operation.
Diagnose network reachability through `tailscale` and authentication through the configured SSH-agent boundary.
The owner handles 1Password approval and private keys directly; do not open the vault, relax approval settings, or disable the agent's protections.

Preserve the hub workflow, machine branches, signatures, hooks, line endings, and current local work when synchronization is needed.
Use the project's ordinary branch and review flow; integration on the Mac does not authorize a release.
Never use `--no-verify`, skip variables, unsigned fallback commits, direct protected-branch writes, or ruleset bypass to overcome a failed gate.
Repair the failing gate or run it on its supported host before completing the authorized push.

Remote agent dispatch and messaging are separate from running a build.
Use domyjob chat, fleet, or herdr only when that agent work is explicitly authorized, and verify their current command contracts before using them.
Do not auto-spawn agents, accept a folder-trust prompt, or grant a writable remote agent merely because another host is available.
Complete independent local work while resolving an actual missing external prerequisite.
