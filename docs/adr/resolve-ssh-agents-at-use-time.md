---
status: proposed
date: 2026-10-23
---
# Resolve SSH agents at use time

## Context

Session-selected sockets outlive authentication keys and leave long-running clients unable to connect.
Git signing must retain its [independent password manager](sign-through-the-password-manager-agent.md).

## Decision

Resolve authentication for each connection through [OpenSSH configuration](https://man.openbsd.org/ssh_config.5).
The [routing rules](../../crates/rules/src/herdr_rules.rs) select Herdr only when every configured key is available; otherwise, select 1Password.
Preserve forwarded agents, explicit socket overrides, and existing host configuration.
A [locked supervisor](../../crates/dotguard/src/bin/herdr-agent/unix.rs) preserves eight-hour expiry and requires approval to reload keys.
[Health checks](../../crates/dotfiles/src/herdr.rs) report stale Codex environments.
The [launcher](../../crates/dotguard/src/herdr.rs) refuses duplicate servers.
Cleanup requires exact PID confirmation, an identity recheck, and SIGTERM.

## Alternatives

Session caches retain stale selections.
A protocol proxy adds another persistent boundary.
Automatic termination risks active work.

## Consequences

Mac and Linux resolve routing at use time.
Windows and WSL keep their existing bridge.
Approval, process observations, and service availability remain external boundaries.
The user reviews and retires legacy agents during scoped installation.

## Confirmation

[Tests](../../crates/dotguard/tests/herdr_agent.rs) and [proof obligations](../../policy/obligations.toml) check routing and lifecycle decisions.
Reconsider if per-connection resolution can't preserve authentication or signing isolation.
