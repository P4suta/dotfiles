# Windows 11 guest runbook

Run Windows 11 as a `qemu:///system` VM on `example` to use Microsoft Office on Mint.
First apply `ansible-playbook -K site.yml --tags libvirt` as an administrator, and confirm that `groups` lists `libvirt kvm` after a new login.

## 0. Prepare the external inputs

| Item | Source | Note |
|---|---|---|
| Windows 11 ISO | Microsoft | Free download |
| `virtio-win.iso` | Fedora's stable release | **Required** for the VirtIO disk and NIC drivers |
| Windows license key | Microsoft | Daily use needs activation |
| Office license | Microsoft 365, or perpetual 2021 or 2024 | **Office stops working without activation** |

Put both ISOs in `/var/lib/libvirt/images/` as the administrator.

## 1. Create the guest in virt-manager

Launch `virt-manager`, open the `QEMU/KVM` connection, and create a new VM:

1. Choose local install media and select the Windows 11 ISO.
   If virt-manager detects no OS type, select `Microsoft Windows 11`.
2. Assign **6144 MB** of RAM and **4 vCPUs**.
3. Create a **60 GiB** qcow2 disk under `/var/lib/libvirt/images/`.
4. Tick **Customize configuration before install**, then select **Finish**.

| Setting | Value |
|---|---|
| Overview → Chipset | **Q35** |
| Overview → Firmware | **UEFI** with OVMF and Secure Boot |
| Add hardware → **TPM** | Emulated **TPM 2.0** with `swtpm` |
| Add hardware → second **CDROM** | `virtio-win.iso` |
| Disk bus | **VirtIO** |
| NIC model | **virtio**, with the default **NAT** network `virbr0` |
| Display | **SPICE** with **QXL** or **virtio** video |

Then start the installation.

## 2. During Windows setup

- On the screen that finds no disk, select **Load driver** and choose `amd64\w11` on the virtio-win ISO.
- If setup rejects the CPU, press `Shift+F10`, run `regedit`, create `HKLM\SYSTEM\Setup\LabConfig`, and add the DWORD value `BypassCPUCheck` with data `1`.

## 3. After installation

1. Run `virtio-win-guest-tools.exe` from the virtio-win ISO.
2. Activate Windows.
3. Install and activate Office.
4. Take virt-manager snapshots after the clean install and again after Office.

## Operational notes

- The host has 15 GiB of physical RAM, and a 6 GiB VM leaves 9 GiB for Mint and Docker.
  Before using Office, run `docker stop` on idle containers, or let `dev-reaper` stop them after two idle hours.
  Office still runs in a 4 GiB VM.

## Verify without sudo

```bash
groups                                              # libvirt kvm present
virsh -c qemu:///system list --all                  # VM list
virsh -c qemu:///system capabilities | grep -i kvm  # KVM acceleration available
virsh -c qemu:///system domstats <vm-name>          # running VM uses accel=kvm
```
