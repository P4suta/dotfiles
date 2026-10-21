---
name: synology-dsm
description: >-
  Operate and diagnose the owner's Synology NAS over the tailnet through DiskStation Manager's own OpenSSH, with administration kept behind interactive privilege.
  Use for DiskStation Manager services, ports, certificates, QuickConnect retirement, Drive and file-share endpoints, and NAS-side Tailscale, not for artifact transfers, tailnet policy, or storage changes.
---

# Synology NAS administration

Read the machine-local SSH host entry and inventory before touching the NAS.
Keep its hostname, account, ports, and QuickConnect ID there, and never in a project or this skill.
Use `tailscale` when the name doesn't resolve or the peer is unreachable, `nas` for archive transfers and restore checks, and `multi-machine` for the development hosts.

## Reach the shell

DiskStation Manager (DSM) exposes ordinary OpenSSH, and Tailscale SSH doesn't run on Synology, so a key in the account's `authorized_keys` is the only unattended path.
DSM accepts SSH only from an account in the `administrators` group, and only when the owner turned on the user home service.
The owner installs the key once with `ssh-copy-id` and a password.
Never type or store the password yourself.
DSM creates home directories world-writable, so when `sshd` still refuses the key, tighten the home to 755, `.ssh` to 700, and `authorized_keys` to 600 before suspecting the key.
The shell is a minimal environment without the Mac's modern tools, so use `grep`, `cat`, `ls`, and `netstat` there.

## Privilege boundary

The administrator shell isn't root, and `sudo` needs an interactive password.
Read every fact you can without root first, and hand each privileged command to the owner as a one-liner they run with a terminal attached, such as `ssh -t ALIAS sudo COMMAND`.
Never add a `NOPASSWD` sudoers entry, change the SSH port, or widen the account's groups to avoid a prompt.
Treat storage pools, shared folders, snapshots, user accounts, and firewall rules as out of scope unless the owner authorized that exact change.

## Discover the installation

| Fact | Where |
| --- | --- |
| DSM version | `/etc/VERSION` |
| Installed packages | `/var/packages/NAME` with tools under `target/bin` and settings under `etc` |
| Listening ports | `netstat -tln` |
| DSM web ports | the redirect page on 80 and 443 names the custom HTTP and HTTPS ports |
| NAS-side Tailscale | `/var/packages/Tailscale/target/bin/tailscale status --json` |
| Certificates | `/usr/syno/etc/certificate`, root only |
| Drive server port | `cstn_port` in the SynologyDrive package settings |

On DSM 7 the Tailscale package runs as its own user.
Outbound connections or a kernel network interface need the documented root boot task, which re-runs after every package upgrade.
An empty `CertDomains` in the status JSON means HTTPS certificates are off for the whole tailnet, which blocks every certificate step below.
`tailscale serve` refuses a non-root caller until the owner names the SSH account as the operator once with `tailscale set --operator=ACCOUNT` under `sudo`.

## Serve the web interface on the tailnet name

Expect the DSM certificate to name the QuickConnect domain until the migration finishes, so a browser warns on the MagicDNS name even though the port is open.
Turning on HTTPS certificates is a change the owner makes in the tailnet administration console.
It publishes the tailnet name and each certified node name in public certificate-transparency logs.
Prefer `tailscale serve --bg https+insecure://127.0.0.1:<port>` from the NAS to front the secure DSM web port, because it renews automatically, stays tailnet-only, and needs no DSM import.
Fall back to `tailscale cert` plus a DSM certificate import and a scheduled renewal task, because those certificates expire after 90 days.
Verify the served name with an `openssl s_client` subject check from another tailnet device before changing any client.
`tailscale serve --https=443 off` removes the proxy and restores the previous behavior on that port.

## Move clients off the relay

QuickConnect is the vendor relay, and every consumer keyed to its ID stops working when it goes away.
List every consumer before turning anything off: desktop Drive clients on each machine, the mobile Drive and Photos apps, and any backup or sync task keyed to the QuickConnect ID.
A consumer re-pointed to the MagicDNS name reaches the NAS only while Tailscale runs on that device, so confirm that trade with the owner.
Repoint the Drive desktop client in its own interface after the certificate step, and never edit the client's database.
Register the file share through the MagicDNS name on each client.
Turn off QuickConnect last, after the owner confirms, and keep the QuickConnect ID, account names, and session identifiers out of every artifact.

## Verify

After each change, confirm from a tailnet device that the intended endpoint answers on the MagicDNS name with the expected certificate, and that the path you retired no longer answers.
Record what changed in the machine-local inventory.

Use the installed DSM help and the current [Synology SSH guidance](https://kb.synology.com/en-us/DSM/tutorial/How_to_login_to_DSM_with_root_permission_via_SSH_Telnet), [Tailscale on Synology](https://tailscale.com/docs/integrations/synology), and [Tailscale HTTPS certificates](https://tailscale.com/docs/how-to/set-up-https-certificates) for behavior this skill doesn't describe.
