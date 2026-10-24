---
name: synology-dsm
description: >-
  Operate and diagnose the owner's Synology NAS over the tailnet through DiskStation Manager's own OpenSSH, with administration kept behind interactive privilege.
  Use for DiskStation Manager services, ports, certificates, QuickConnect retirement, Drive and file-share endpoints, and NAS-side Tailscale, not for artifact transfers, tailnet policy, or storage changes.
---

# Synology NAS administration

DiskStation Manager (DSM) is the NAS operating system.
Read the machine-local SSH host entry and inventory before touching the NAS.
Keep its hostname, account, ports, and QuickConnect ID there, and never in a project or this skill.
Use `tailscale` when the name doesn't resolve or the peer is unreachable, `nas` for archive transfers and restore checks, and `multi-machine` for the development hosts.

## Reach the shell

Use DSM's OpenSSH server because [Tailscale SSH doesn't run on Synology](https://tailscale.com/docs/integrations/synology#limitations-and-known-issues).
[DSM 6 and later require an SSH account in the `administrators` group](https://kb.synology.com/en-us/DSM/tutorial/How_to_login_to_DSM_with_root_permission_via_SSH_Telnet).
Install the authorized public key under the account's home directory for unattended access, and leave interactive credentials with the owner.
When key authentication fails, inspect ownership and permissions before changing them.
With `StrictModes` enabled, OpenSSH rejects an authorization file when its file, directory, or home is writable by other users.
The [OpenSSH file guidance](https://man.openbsd.org/sshd.8#FILES) recommends owner-only access to `.ssh` and `authorized_keys`.
Use the commands available on DSM rather than assuming the development hosts' tool profiles exist.

## Privilege boundary

The administrator shell isn't root, and DSM documents interactive escalation through `sudo -i`.
Read every fact you can without root first, and hand each privileged command to the owner as a one-liner they run with a terminal attached, such as `ssh -t ALIAS sudo COMMAND`.
Never add a `NOPASSWD` sudoers entry, change the SSH port, or widen the account's groups to avoid a prompt.
Treat storage pools, shared folders, snapshots, user accounts, and firewall rules as out of scope unless the owner authorized that exact change.

## Discover the installation

| Fact | Where |
| --- | --- |
| DSM version | `/etc/VERSION` |
| Installed packages | `/var/packages/NAME` with tools under `target/bin` and settings under `etc` |
| Listening ports | `netstat -tln` |
| DSM and file-service endpoints | the configured DSM or package interface, the listening sockets, and the machine-local inventory |
| NAS-side Tailscale | `/var/packages/Tailscale/target/bin/tailscale status --json` |
| Certificates | the presented TLS chain and DSM's certificate interface |

On DSM 7, outbound tailnet connections from other apps require the [documented network device setup](https://tailscale.com/docs/integrations/synology#enable-outbound-connections).
The root boot task must run again after a Tailscale package upgrade.
Check the current certificate domains and Domain Name System (DNS) settings without inferring a tailnet-wide configuration from an empty domain list.
If daemon management needs an operator, the owner can grant the intended account that role with [interactive `sudo tailscale set --operator=ACCOUNT`](https://tailscale.com/docs/reference/troubleshooting/linux/linux-operator-permission).

## Serve the web interface on the tailnet name

Inspect the current certificate's DNS names and expiry before choosing a migration.
[Tailscale HTTPS requires MagicDNS and HTTPS enablement](https://tailscale.com/docs/how-to/set-up-https-certificates#configure-https).
Certificate issuance publishes the full device DNS name in public certificate-transparency records.
Read the existing Serve configuration and preserve unrelated endpoints before adding the DSM proxy.
[Tailscale Serve](https://tailscale.com/docs/reference/tailscale-cli/serve) provides tailnet access with an automatically provisioned TLS certificate.
Use a valid HTTPS target when available.
For a recorded loopback target with an untrusted DSM certificate, use `tailscale serve --bg --https=FRONTEND_PORT https+insecure://127.0.0.1:DSM_PORT`.
Keep TLS validation enabled on clients.
Verify the chain and full DNS name from another tailnet device with a client that fails on verification errors.
For OpenSSL, use [`openssl s_client -connect HOST:FRONTEND_PORT -servername HOST -verify_hostname HOST -verify_return_error`](https://docs.openssl.org/3.6/man1/openssl-s_client/).
If using `tailscale cert` and a DSM certificate import instead, schedule and verify renewal before the actual certificate expiry.
For rollback, remove only the endpoint added by the task with `tailscale serve --https=FRONTEND_PORT off`, or restore its recorded prior configuration.

## Retirement

When the owner requests QuickConnect retirement, inventory every consumer before turning the service off.
Include desktop Drive clients on each machine, mobile Drive and Photos apps, and backup or sync tasks keyed to the QuickConnect ID.
Verify that each intended client can connect through Tailscale before changing its endpoint.
Check each DSM, Drive, and file share endpoint because the DSM proxy doesn't forward other services' protocols.
Use the client's supported interface to update or recreate its connection.
Keep the selected folders and synchronization direction.
Never edit a client's database.
Register the file share through the MagicDNS name on each client.
Turn off QuickConnect only after the migration's consumers pass their connection checks and the inventory records their rollback settings.
Keep the QuickConnect ID, account names, and session identifiers in the machine-local inventory.

## Verify

After each change, verify the intended endpoints from a tailnet device with certificate validation and the relevant app operation.
For retirement, also confirm the configured QuickConnect state in DSM.
Record what changed in the machine-local inventory.

Use the installed DSM help and the current [Synology SSH guidance](https://kb.synology.com/en-us/DSM/tutorial/How_to_login_to_DSM_with_root_permission_via_SSH_Telnet), [Tailscale on Synology](https://tailscale.com/docs/integrations/synology), and [Tailscale HTTPS certificates](https://tailscale.com/docs/how-to/set-up-https-certificates) for behavior this skill doesn't describe.
