# Linux system provisioning

Ansible manages the administrator-owned system tier; chezmoi manages the workspace user's home.
The playbook targets native Linux and refuses WSL.
Keep host names, addresses, workspace identity, and credentials in a private inventory and extra-vars file outside this checkout.
The required `workspace_user` has no public default.

From an administrator-accessible copy of the reviewed checkout, install the pinned tools with mise and run `just ops-check`.
The Rust checker installs the pinned `community.general` collection into an owned temporary directory and performs syntax validation without applying the playbook.
The collection supplies the section-aware [INI module](https://docs.ansible.com/projects/ansible/latest/collections/community/general/ini_file_module.html) used for XRDP configuration.

Before native provisioning, install `ops/requirements.yml` into the administrator's Ansible collection directory.
Run the following commands from `ops/`, with absolute paths to the private inventory and variable file:

```text
mise x -- ansible-galaxy collection install --no-deps --requirements-file requirements.yml
mise x -- ansible-playbook --inventory PRIVATE_INVENTORY --extra-vars @PRIVATE_VARIABLES --list-tags site.yml
mise x -- ansible-playbook --inventory PRIVATE_INVENTORY --extra-vars @PRIVATE_VARIABLES --check --diff -K site.yml
mise x -- ansible-playbook --inventory PRIVATE_INVENTORY --extra-vars @PRIVATE_VARIABLES -K site.yml
```

Use a fresh administrator-owned checkout or staging directory rather than replacing a shared temporary directory.
Inspect the exact tag selection before applying.
The `openssh`, `xrdp`, `verify`, and `oom-verify` tags require explicit selection.
Apply XRDP changes from SSH because the service restart disconnects an RDP session.

The `oom` tag protects NetworkManager and tailscaled without restarting either service or changing DHCP configuration, then verifies itself.
It requires a reviewed `oom_resources` mapping for the host in the private inventory, with byte values for the workspace user's soft limit, hard limit, swap, and preflight threshold, plus the aggregate domyjob slice and its two-job policy.
The preflight refuses limits that do not fit in physical memory, inconsistent orderings, running domyjob workers, and working memory at or above the preflight threshold.
CPU remains unrestricted, and the existing swap files and swappiness remain unchanged.
Cached files count toward cgroup memory usage, so deployment first applies only the soft limit and verifies actual reclaim before persisting or applying a hard limit.
If that check fails, no new hard limit is installed; the soft limit and management protection remain in place.
Finish memory-intensive work and retry the same tag rather than dropping global caches or stopping management services.
Use `oom-verify` for the read-only configuration check.
Docker, Tailscale, and libvirt tasks preserve running services instead of restarting them through notifications.

Sign in to the native 1Password app after installation and enable its SSH agent and CLI integration.
Tailscale enrollment, subnet routes, exit-node selection, and application logins remain explicit machine-local operations.
Keep their actual network values outside the public source.
The desktop tag removes the retired `orca-ide` package and preserves GNOME's `orca` screen reader.
After a native user-profile apply selects Fcitx 5, log out and back in to activate the session change.
See [the Windows VM runbook](runbooks/windows-vm.md) for the separate VM setup.
