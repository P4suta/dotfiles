---
status: proposed
date: 2026-10-23
---
# Resolve SSH agents at use time

## Status

Proposed.

## Context

Herdr sessions selected their SSH agent when a shell started and pinned its socket in `GIT_SSH_COMMAND`.
The dedicated agent's eight-hour key expiry left those sessions unable to authenticate.
Codex's managed app server retains the environment that started it, and domyjob's chat service runs independently of terminal shells.
Ghostty closes during normal use, and a frozen Herdr instance must never cause a launcher to leave another server beside it.
The [signing decision](sign-through-the-password-manager-agent.md) requires Git signing to use 1Password independently of the authentication agent.

## Decision

On macOS and Linux, OpenSSH reads a managed include before existing directives, after the leading OrbStack include when present.
Its `Match exec` tests call `herdr-agent route` for each connection.
The [routing rule](../../crates/rules/src/herdr_rules.rs) selects Herdr only when the dedicated agent answers with every configured public key.
An empty, missing, unreachable, or unresponsive agent selects the configured 1Password socket instead.
Remote shells retain their forwarded agent, and an explicit `DOTFILES_SSH_AUTH_SOCK` selection overrides a daemon's obsolete `SSH_AUTH_SOCK`.
The include preserves existing host configuration and the leading OrbStack include.
OpenSSH's explicit command-line options retain their normal precedence.

Shells and Git name `/usr/bin/ssh` without a session-selected `IdentityAgent` override.
Codex's existing `modify_config.toml` retains project trust, notification routing, memory settings, and other host state.
It sets the same SSH command for spawned commands and sets their default agent socket to the configured password manager socket through the [shell environment policy](https://learn.chatgpt.com/docs/config-file/config-advanced#shell-environment-policy).
This replaces a managed app server's stale inherited overrides at the command boundary after the owner reloads its configuration.
Domyjob invokes `ssh` at each connection, so it reads the same include even when its daemon inherited an obsolete agent socket.
Domyjob's job environment allowlist does not forward SSH agent or connection variables.
No new daemon-specific agent cache or replacement daemon service is needed.

Launchd on macOS and a systemd user service on Linux run `herdr-agent serve` independently of Herdr and the terminal.
The supervisor holds an exclusive lock and runs `ssh-agent` in the foreground.
It refuses to replace an answering agent, an unresponsive agent that still accepts connections, or a non-socket path.
Only a disconnected stale socket can be removed.
The existing eight-hour key limit and explicit `herdr-agent lock` remain.
`herdr-agent ensure` serializes key loading, starts the registered service when absent, and reloads the configured keys through 1Password approval.
Routing and health checks never load keys or open the vault.

The terminal launcher serializes server startup and checks the owned process list and a bounded server-status probe first.
Multiple servers, an abandoned client, or an unresponsive existing server refuse another start and name the recovery commands.
A server and its attached client are distinct roles and are not a duplicate pair.
`dotfiles herdr cleanup --pid PID` requires a terminal, displays the process and its start time, asks for the exact PID confirmation, rechecks its identity, and sends only SIGTERM.
It never selects a process automatically or escalates to SIGKILL.

`dotfiles herdr health` reports empty or stale agents, duplicate or frozen Herdr instances, and named Codex or domyjob daemon PIDs whose agent environment points at a dead socket.
It prints only the relevant process identity, never the daemon's whole environment.
The existing session start hook adds the same report as advisory context, within a bounded observation budget, without blocking work on findings or observation errors.
The [tests](../../crates/dotfiles/src/herdr.rs) inject agent and process observations.
The pure decisions have unit tests, Kani harnesses, and [rejecting probes](../../policy/obligations.toml).
The [OpenSSH configuration contract](https://man.openbsd.org/ssh_config.5) supplies per-connection `Match exec` evaluation and first-value precedence.
OpenSSH authentication, password manager approval, process observations, and service-manager behavior remain external trust boundaries.
Unit tests do not establish those services' live availability.

## Consequences

Authentication can fall back to 1Password after key expiry or a Herdr restart without changing a daemon's environment on every transition.
Git signing keeps its dedicated password manager helper and never falls back to Herdr.
A session-start socket cache was rejected because a long-lived process would retain its obsolete selection.
A custom SSH-agent protocol proxy was rejected because it would add protocol forwarding and another persistent socket boundary.
Automatic process termination was rejected because a frozen status endpoint does not prove that its sessions contain no live work.
Windows and WSL retain Windows OpenSSH and their existing signing bridge.

Adoption requires the owner to install the updated runtime and render the affected SSH, Git, shell, Codex, key-list, and service targets through the normal profile workflow.
Before registering the service on a host with a legacy standalone agent, the owner must review and retire that specific agent process after saving work.
The new supervisor deliberately refuses to displace it.
On macOS, bootstrap `~/Library/LaunchAgents/dev.dotfiles.herdr-agent.plist` in the user's GUI launchd domain.
On Linux, reload the user manager and enable `herdr-agent.service` with `systemctl --user enable --now herdr-agent.service`.
The normal services setup also registers these definitions.
Then run `herdr-agent ensure`, approve 1Password, and run `dotfiles herdr health`.
Reopen shells and reload Codex's managed app server once after saving active work, so no pre-adoption `GIT_SSH_COMMAND` override survives.
Domyjob reads the new SSH include on its next connection; a daemon reported with an obsolete environment can be reopened by the owner after saving its work.
No live process or service migration is part of this repository change.
