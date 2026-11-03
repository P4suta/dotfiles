---
date: 2026-11-03
status: accepted
---
# Give each generated SSH host its own login

## Context

[ADR 0020](0020-os-family-push-evidence.md) has `hosts sync` write one `Host` line for every selected peer and one shared `User` line.
The owner's Windows account is `livec`, while the Linux and Mac accounts are `yasunobu`, so no single user serves every peer.
The same record states that `~/.ssh/config` includes the generated file, but the managed template only inserted the agent include.
`hosts doctor` therefore reached `laptop` as the local account, and the Windows server refused it as an unknown user.

## Decision

This amends the user and include rules of [ADR 0020](0020-os-family-push-evidence.md).
`hosts sync` writes one `Host` block for each peer with that peer's own `User`.
`--user HOST=USER` sets one peer's user, and a bare `--user USER` sets every other peer's user.
Without a request, a peer keeps the user of the earlier generated file, and a combined block from an earlier release gives its user to each of its aliases.
A hub remote on that peer supplies the user last.

On Mac and Linux, the managed `~/.ssh/config` includes `~/.local/state/hosts/ssh_config` directly before the agent include.
The agent file ends inside `Host *`, so the host include must come first to stay at the top level.
The apply guard in `crates/rules/src/herdr_rules.rs` admits exactly these two includes in this order and refuses any other change to the owner's lines.

Windows keeps its unmanaged `~/.ssh/config`, which the owner already points at the generated file.

## Alternatives

Using the owner's hand-written aliases, such as `win`, in `hosts doctor` would fix one host but leave the pinned keys and users of the generated file unused.
Asking for the user of every peer on each sync would repeat a fact that the generated file already records.

## Consequences

`ssh laptop` and `domyjob doctor laptop` use the generated user and pinned key on Mac and Linux.
A sync while a selected peer is offline still fails, because the sync pins each key over the tailnet.
