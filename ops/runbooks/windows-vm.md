# Runbook: Windows 11 VM (KVM/libvirt)

Manual procedure for `example`: run Windows 11 as a `qemu:///system` VM in order to use Microsoft Office on Mint.
Assumes admin has applied `ansible-playbook -K site.yml --tags libvirt` and that `groups` now lists `libvirt kvm` after a re-login.

VM definitions live in `/etc/libvirt/qemu/` and virtual disks in `/var/lib/libvirt/images/` — system side, outside chezmoi.

## 0. Prepare (external, cannot be automated)

| Item | Source | Note |
|---|---|---|
| Windows 11 ISO | Microsoft (free download) | |
| virtio-win ISO | Fedora's stable `virtio-win.iso` | **Required**; without it there are no VirtIO disk/NIC drivers and performance suffers |
| Windows license key | — | Evaluation runs unactivated; daily use needs activation |
| Office license | Microsoft 365 or perpetual 2021/2024 | **Office stops working unless activated** — the one irreducible external dependency |

Put the ISOs in `/var/lib/libvirt/images/` (as admin).
The qemu process runs as `libvirt-qemu` and often cannot read anything under `/home/example` because of AppArmor and permissions; alternatively accept virt-manager's "adjust permissions?"
prompt when attaching them.

## 1. Create the VM in virt-manager

Launch `virt-manager` → connection `QEMU/KVM` (system) → new VM:

1. Install source = **local install media (ISO)** → the Windows 11 ISO.
   If the OS type is not detected, set `Microsoft Windows 11` by hand.
2. **6144 MB** RAM / **4 vCPU** (see the RAM note below).
3. **60 GiB** disk (qcow2, under `/var/lib/libvirt/images/`).
4. Tick **"Customize configuration before install"**, then Finish.

| Setting | Value | Why |
|---|---|---|
| Overview → Chipset | **Q35** | Recommended for Win11 |
| Overview → Firmware | **UEFI (OVMF, secboot)** | Required by Win11 |
| Add hardware → **TPM** | Emulated, **TPM 2.0** (swtpm) | Required by Win11 |
| Add hardware → **second CDROM** | the virtio-win ISO | Driver supply |
| Disk bus | **VirtIO** | Performance; needs the driver loaded during install |
| NIC model | **virtio** | Performance. Default **NAT (virbr0)** is fine for activation and updates |
| Display | **Spice**, video **QXL** or **virtio** | Clipboard sharing and auto-resize |

Then start the install.

## 2. During Windows setup

- On the "no disk found" screen use **Load driver** and point at `amd64\w11` (`viostor` / `vioscsi`) on the virtio-win ISO.
- If the CPU is rejected (the i7-7700K is not on Microsoft's Win11 list):
  `Shift+F10` → `regedit` → create `HKLM\SYSTEM\Setup\LabConfig` and set DWORD `BypassCPUCheck` = `1`.
  TPM 2.0 and Secure Boot are satisfied by this config,
  so no other bypass is needed.

## 3. After install

1. Run **`virtio-win-guest-tools.exe`** from the virtio-win ISO — all VirtIO drivers plus the SPICE guest agent (clipboard sharing, auto-resize).
2. Activate Windows.
3. Install Office, then activate Office.
4. Once stable, take virt-manager **snapshots** (right after a clean install, and again after Office) so a broken state can be rolled back.

## Operational notes

- **RAM (most important)**: 15 GiB physical.
  A 6 GiB VM leaves 9 GiB for Mint and Docker, which swaps if heavy dev containers run alongside.
  Stop unneeded containers before using Office (`dev-reaper` auto-stops after 2h idle;
  otherwise `docker stop`).
  Office still runs with the VM at 4 GiB.
- **File sharing**: SPICE shared folders (bundled with guest-tools) or cloud storage.
  Clipboard sharing plus drag-and-drop covers most needs.
- **Network**: default NAT is enough.
  Reaching the VM directly from the LAN would need a bridge (`bridge-utils`); out of scope here.
- **GPU**: no passthrough — IOMMU is disabled and there is a single GPU.
  Office does not need it; virtio-gpu / QXL is fine.

## Verification (as example, no sudo)

```bash
groups                                              # libvirt kvm present
virsh -c qemu:///system list --all                  # VM list
virsh -c qemu:///system capabilities | grep -i kvm  # KVM acceleration available
virsh -c qemu:///system domstats <vm-name>          # running VM uses accel=kvm
```
