---
name: multi-machine
description: >-
  Use owner-side machines for optional project caches, synchronization, and remote development through domyjob.
  Use for cross-platform execution, machine synchronization, and remote development.
---

# Multi-machine development

`dotfiles hosts sync` generates the machine-local SSH aliases and pinned host keys for one host per OS family from the tailnet.
`dotfiles hosts doctor` checks each host and prints the next action for any host that fails.
Read machine-local SSH aliases and synchronization topology before operating another machine.
Read the current `domyjob` project skill before driving another machine.
Keep machine identities, paths, and synchronization conventions in the owner's configuration.
Never put them in a project or this skill.
Use `dotfiles` for authorized configuration rollout to the affected hosts.

Run optional cache preparation and remote development commands through domyjob from the intended local checkout.
Use `run` when the command needs the current files, including uncommitted work, and `on` for an explicit command on the remote host.
Check host readiness with `domyjob doctor MACHINE` before relying on it.
Pass a real executable and arguments after `--`, and use an explicit host shell only when the expression needs it.
Follow the current snapshot size, filename, link, and environment rules.

Use the project's documented hosted runners or pinned container for required platform checks.
Exercise native paths and failure behavior there, because cross-compilation proves nothing about operating-system behavior.
The owner's hosts may cache artifacts and exact-source evidence.
Their availability never defines a project's verdict.
Keep `domyjob` and `dotfiles hosts check` within the owner-side tooling boundary in `formal-assurance`.
Never run signing, notarization, publication, privileged installation, or destructive cleanup to verify ordinary code.

For a long run, keep its job identity and inspect its status or logs without resubmitting it.
Preserve exit status and the primary failure, and tell a transport interruption apart from the job's result.
Resolve an uncertain start through the submission identity instead of launching a duplicate.
Diagnose network reachability through `tailscale` and authentication through the configured SSH-agent boundary.
Before treating a failed agent or credential-store call as expired credentials, compare the executable, user, environment, and login session with the owner's working context.
Read [credential-context diagnosis](references/credential-context.md) when remote background execution can't reach credentials that work on the desktop.
Never open the 1Password vault, relax approval settings, or turn off its key protections.

Preserve the hub workflow, machine branches, signatures, hooks, line endings, and local work during synchronization.
Synchronization grants no release permission.
Never use `--no-verify`, skip variables, unsigned fallback commits, direct protected-branch writes, or ruleset bypass to pass a failed gate.
Repair the failing gate or run it through the project's documented runner before the authorized push.

Use domyjob chat, fleet, or herdr only for explicitly authorized session work.
Never start sessions automatically, accept a folder-trust prompt, or grant a remote session write access just because another host exists.
