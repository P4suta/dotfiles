# Windows 11 virtual machine runbook

Run Windows 11 as a `qemu:///system` VM on `example` to use Microsoft Office on Mint.
First, an administrator applies `ansible-playbook -K site.yml --tags libvirt`, and after a new login `groups` lists `libvirt kvm`.

VM definitions live in `/etc/libvirt/qemu/` and disks in `/var/lib/libvirt/images/`, outside chezmoi.

## 0. Prepare the external inputs

| Item | Source | Note |
|---|---|---|
| Windows 11 installer image | Microsoft | Free download |
| `virtio-win.iso` | Fedora's stable release | **Required** for the VirtIO disk and network drivers |
| Windows license key | Microsoft | Daily use needs activation |
| Office license | Microsoft 365, or perpetual 2021 or 2024 | **Office stops working without activation** |

Put both images in `/var/lib/libvirt/images/` as the administrator.
AppArmor and file permissions often stop the `libvirt-qemu` process from reading `/home/example`.
Or accept the virt-manager prompt to adjust permissions when you attach an image.

## 1. Create the virtual machine in virt-manager

Launch `virt-manager`, open the `QEMU/KVM` connection, and create a new VM:

1. Choose local install media and select the Windows 11 image.
   If virt-manager detects no OS type, select `Microsoft Windows 11`.
2. Assign **6144 MB** of RAM and **4 vCPUs**.
3. Create a **60 GiB** `qcow2` disk under `/var/lib/libvirt/images/`.
4. Tick **Customize configuration before install**, then select **Finish**.

| Setting | Value | Reason |
|---|---|---|
| Overview → Chipset | **Q35** | Windows 11 recommends it |
| Overview → Firmware | `UEFI` with `OVMF` `secboot` | Windows 11 requires it |
| Add hardware → Trusted Platform Module (TPM) | Emulated **TPM 2.0** with `swtpm` | Windows 11 requires it |
| Add hardware → second `CDROM` | `virtio-win.iso` | Supplies the drivers |
| Disk bus | **VirtIO** | Speed, with the driver loaded during setup |
| Network device model | **virtio** | Speed, with the default Network Address Translation (NAT) network on `virbr0` |
| Display | **Spice** with **`QXL`** or **virtio** video | Clipboard sharing and automatic resizing |

Then start the installation.

## 2. During Windows setup

- On the screen that finds no disk, select **Load driver** and choose `amd64\w11` on the virtio-win image for `viostor` and `vioscsi`.
- If setup rejects the CPU, as it rejects the i7-7700K, press `Shift+F10`, run `regedit`, create `HKLM\SYSTEM\Setup\LabConfig`, and add the `DWORD` value `BypassCPUCheck` with data `1`.
  The emulated TPM and Secure Boot pass the remaining checks.

## 3. After installation

1. Run `virtio-win-guest-tools.exe` from the virtio-win image to install the VirtIO drivers and `spice-vdagent` for clipboard sharing and automatic resizing.
2. Activate Windows.
3. Install and activate Office.
4. After the system runs stably, take virt-manager snapshots after the clean install and again after Office, so you can roll back a broken state.

## Operational notes

- The host has 15 GiB of physical RAM, so memory matters most.
  A 6 GiB VM leaves 9 GiB for Mint and Docker, which swap when heavy development containers run too.
  Before using Office, stop idle containers with `docker stop`, or let `dev-reaper` stop them after two idle hours.
  Office still runs in a 4 GiB VM.
- For file sharing, use `SPICE` shared folders from the guest tools, cloud storage, or the shared clipboard and dragging files.
- The default NAT network suffices.
  Reaching the VM from the local network needs a bridge from `bridge-utils`, which this runbook omits.
- The VM gets no GPU passthrough, because the host has one GPU and `IOMMU` turned off.
  Office runs well on `virtio-gpu` or `QXL`.

## Verify without sudo

```bash
groups                                              # libvirt kvm present
virsh -c qemu:///system list --all                  # VM list
virsh -c qemu:///system capabilities | grep -i kvm  # KVM acceleration available
virsh -c qemu:///system domstats <vm-name>          # running VM uses accel=kvm
```
