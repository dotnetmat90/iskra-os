# Building and running Iskra on Proxmox VE

Proxmox runs x86 VMs with KVM, so Iskra runs at native speed (an M1 Mac can only emulate it),
the web console is the screen, and nested virtualization makes it the place for hypervisor work.

**Status:** not yet tried on Proxmox. The commands are standard Proxmox; booting Redox under
Proxmox's OVMF firmware is the untested part.

Two VMs: one to build, one per Iskra image you want to use.

## 0. Check the host (read-only)

On the Proxmox host:

```bash
lscpu | grep -E 'Vendor ID|Model name|Virtualization'
cat /sys/module/kvm_intel/parameters/nested 2>/dev/null || cat /sys/module/kvm_amd/parameters/nested
pvesm status
```

Nested virtualization should print `Y` or `1`. If it doesn't, enabling it means setting
`options kvm_intel nested=1` (or `kvm_amd`) in `/etc/modprobe.d/kvm.conf` and reloading the module
with no VMs running. `pvesm status` lists storage names (`local-lvm`, `local-zfs`, ...); use yours
wherever this guide says `local-lvm`.

## 1. Build VM

Ubuntu 24.04 VM: 8 cores, 16 GB RAM, 80 GB disk, **CPU type `host`** (passes virtualization
through, so `./iskra.sh test` boots Iskra under KVM inside this VM).

Inside it:

```bash
git clone -b iskra https://github.com/dotnetmat90/iskra-os ~/iskra-os
cd ~/iskra-os
./iskra.sh check     # CPU, KVM, nested virtualization, disk
./iskra.sh setup     # podman, QEMU, OVMF, sub-ID ranges for rootless podman
./iskra.sh build     # build/x86_64/iskra/harddrive.img
./iskra.sh test      # headless boot, passes when the login prompt appears
./iskra.sh run       # desktop on VNC at 127.0.0.1:5900 (ssh -L 5900:127.0.0.1:5900 <vm>)
```

## 2. Iskra as a Proxmox VM

Copy `build/x86_64/iskra/harddrive.img` to the Proxmox host, then:

```bash
qm create 9100 --name iskra --memory 4096 --cores 4 --machine q35 --bios ovmf \
  --efidisk0 local-lvm:1,efitype=4m,pre-enrolled-keys=0 \
  --net0 e1000,bridge=vmbr0 --vga std
qm importdisk 9100 harddrive.img local-lvm
qm config 9100 | grep unused          # name of the imported disk, e.g. local-lvm:vm-9100-disk-1
qm set 9100 --sata0 local-lvm:vm-9100-disk-1 --boot order=sata0
qm start 9100
```

Open the VM's console in the web UI. Login: `user`, empty password.

- `pre-enrolled-keys=0` keeps Secure Boot off; Redox's bootloader is not signed.
- SATA disk and e1000 network have Redox drivers. The network uses DHCP.
- If the mouse misbehaves, `qm set 9100 --tablet 0` (PS/2 input; UTM needed the same).
- To update to a new build: `qm stop 9100`, `qm disk import` the new image, swap `sata0`,
  delete the old disk.
