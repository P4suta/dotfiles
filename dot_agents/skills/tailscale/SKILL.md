---
name: tailscale
description: >-
  Configure and diagnose Tailscale connectivity and least-privilege tailnet access for development machines.
  Use for tailnet reachability, MagicDNS, access rules, SSH transport, and machine enrollment.
---

# Tailscale Development Connectivity

Read the existing dotfiles, SSH aliases, machine inventory, and tailnet policy before changing networking.
Use `multi-machine` for remote development and `domyjob` for commands on another machine.
Keep hostnames and machine-specific configuration in the existing inventory rather than copying current IP addresses into reusable instructions.
Check the installed `tailscale version` and relevant `--help`; use [Tailscale's official documentation](https://tailscale.com/docs/how-to/quickstart) for current platform behavior.

Diagnose layers in order: local client and service state, tailnet membership, peer reachability, name resolution, access rules, then the destination service and SSH authentication.
Use `tailscale status`, `tailscale ping HOST`, and `tailscale netcheck` as relevant.
Inspect only the metadata needed for the failure and keep credentials and private network reports out of public artifacts.
An offline peer, a denied connection, and a locked SSH agent are different failures; fix the responsible layer instead of broadening every rule.

Distinguish ordinary OpenSSH carried over Tailscale from [Tailscale SSH](https://tailscale.com/docs/features/tailscale-ssh).
Preserve the existing OpenSSH and owner-managed SSH-agent contract unless the user authorizes a migration.
Do not enable `tailscale up --ssh`, reset login state, change advertised routes, or switch an exit node as a generic connectivity fix.
Verify which SSH server and operating systems support the intended mode before changing it.
The owner handles 1Password approval and private keys directly; never inspect the vault or relax its approval settings to obtain unattended access.

For a policy change, preserve unrelated rules and use the narrowest identity, destination, protocol, and port that satisfies the intended connection.
Read the current [access-control guidance](https://tailscale.com/docs/features/access-control/acls) and grants model before editing the policy.
Add positive and negative policy tests for the actual identities and services and validate the policy before applying it.
Do not use a wildcard allow rule, shared administrator credentials, public service exposure, or disabled checks to resolve a denied connection.
Treat routes, exit nodes, Funnel, device tags, key expiry, and machine enrollment as explicit capabilities with their own authorized scope.

Manage reusable installation and settings through the existing dotfiles and platform service conventions.
Keep authentication keys and runtime state machine-local and out of version control.
After an authorized change, verify the intended connection and a representative forbidden connection, then complete the original development task.
