# Linux system provisioning

Ansible manages the administrator-owned system tier, and chezmoi manages the workspace user's home.
The playbook targets native Linux and refuses Windows Subsystem for Linux (WSL).
Keep host names, addresses, workspace identity, and credentials in a private inventory and extra-vars file outside this checkout.
The required `workspace_user` has no public default.

From an administrator-owned copy of the reviewed checkout, install the pinned tools with mise and run `just ops-check`, which validates the playbook without applying it.

To provision, run these commands from `ops/` with absolute paths to the private inventory and variables file:

```text
mise x -- ansible-galaxy collection install --no-deps --requirements-file requirements.yml
mise x -- ansible-playbook --inventory PRIVATE_INVENTORY --extra-vars @PRIVATE_VARIABLES --list-tags site.yml
mise x -- ansible-playbook --inventory PRIVATE_INVENTORY --extra-vars @PRIVATE_VARIABLES --check --diff -K site.yml
mise x -- ansible-playbook --inventory PRIVATE_INVENTORY --extra-vars @PRIVATE_VARIABLES -K site.yml
```

Run from a fresh administrator-owned checkout or staging directory, not a shared temporary directory.
Inspect the tag selection before applying.
The `openssh`, `xrdp`, `verify`, and `oom-verify` tags require explicit selection.
Apply `xrdp` changes over SSH, because the service restart ends a remote desktop session.

The `oom` tag protects NetworkManager and tailscaled without restarting either service, then verifies itself.
It requires a reviewed `oom_resources` mapping for the host in the private inventory.
The mapping holds byte values for the workspace user's soft limit, hard limit, swap, and preflight threshold, and the domyjob slice total with its two-job policy.
The preflight refuses limits larger than physical memory, inconsistent orderings, running domyjob workers, and working memory that reaches the preflight threshold.
The tag leaves CPU, swap files, and swappiness unchanged.
It applies the soft limit first and installs a hard limit only after it verifies reclaim.
If reclaim fails, the tag keeps the soft limit and management protection and installs no hard limit.
Finish memory-intensive work and rerun the tag instead of dropping caches or stopping management services.
Use `oom-verify` for the read-only check.
The Docker, Tailscale, and libvirt tasks never restart running services.

After installation, sign in to the native 1Password app and turn on its SSH and `op` integrations in the Developer settings.
Tailscale enrollment, subnet routes, exit-node selection, and app logins stay manual steps on each machine.
Keep their network values outside the public source.
The desktop tag removes the retired `orca-ide` package and keeps the `orca` screen reader.
After a native user-profile apply selects Fcitx 5, log out and back in.
See [the Windows VM runbook](runbooks/windows-vm.md) for the VM setup.
