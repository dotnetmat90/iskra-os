#!/usr/bin/env bash
# Iskra build and test helper for a Linux host (x86_64 server or arm64 VM).
# Usage: ./iskra.sh check | setup | build | run | test
set -euo pipefail
cd "$(dirname "$0")"

ARCH=x86_64
CONFIG=${ISKRA_CONFIG:-iskra}
BUILD=build/$ARCH/$CONFIG
# Text that must appear on the serial console for a boot to count as good.
BOOT_MARKER=${ISKRA_BOOT_MARKER:-"login:"}
BOOT_TIMEOUT=${ISKRA_BOOT_TIMEOUT:-600}

# ponytail: run/test reuse upstream `make qemu` (UEFI firmware, KVM auto-detect, devices).
# Extra flags ride in through the QEMU variable because upstream builds QEMUFLAGS itself.
qemu() {
  local extra=$1; shift
  make qemu ARCH=$ARCH CONFIG_NAME=$CONFIG QEMU="qemu-system-$ARCH $extra" "$@"
}

case "${1:-}" in
check)
  # Read-only: is this host suitable, and can it expose VT-x/SVM to Iskra (needed for the hv work)?
  lscpu | grep -E 'Architecture|Vendor ID|Model name|Virtualization' || true
  [ -e /dev/kvm ] && echo "KVM: present" || echo "KVM: MISSING (no acceleration; VPS without nested virt?)"
  for m in kvm_intel kvm_amd; do
    f=/sys/module/$m/parameters/nested
    [ -r $f ] && echo "nested ($m): $(cat $f)"
  done
  df -h . | tail -1
  ;;
setup)
  sudo apt-get update
  sudo apt-get install -y git make curl podman fuse3 uidmap qemu-system-x86 qemu-utils ovmf
  # Rootless podman needs sub-ID ranges; some images and VMs ship without them.
  if ! grep -q "^$USER:" /etc/subuid; then
    sudo usermod --add-subuids 100000-165535 --add-subgids 100000-165535 "$USER"
    podman system migrate
  fi
  ;;
build)
  printf 'ARCH?=%s\nCONFIG_NAME?=%s\nREPO_BINARY?=1\nPODMAN_BUILD?=1\nFSTOOLS_IN_PODMAN?=1\n' "$ARCH" "$CONFIG" > .config
  # orbdata copies branding/share at build time; drop its old output, and the repo tag so make
  # re-runs packaging (it otherwise only does that when the config file changes).
  rm -rf recipes/gui/orbdata/target "$BUILD/repo.tag"
  # CI=1 turns off the cookbook's full-screen progress view, which crashes without a terminal.
  make all CI=1
  ls -lh "$BUILD/harddrive.img"
  ;;
run)
  # Desktop over VNC on localhost only. From your machine:
  #   ssh -L 5900:127.0.0.1:5900 <server>   then open vnc://localhost:5900
  qemu "-display vnc=127.0.0.1:0"
  ;;
test)
  # Headless boot on a throwaway snapshot; pass when the boot marker shows on serial.
  log=$PWD/$BUILD/serial.log
  rm -f "$log"
  # Own process group, so the trap can stop make and the QEMU it starts together.
  setsid bash -c "$(declare -f qemu); ARCH=$ARCH CONFIG=$CONFIG; qemu -snapshot gpu=no qemu_serial_logfile='$log'" \
    < /dev/null > /dev/null 2>&1 &
  pid=$!
  trap 'kill -- -$pid 2>/dev/null || true' EXIT
  for _ in $(seq "$BOOT_TIMEOUT"); do
    if grep -q "$BOOT_MARKER" "$log" 2>/dev/null; then
      echo "PASS: '$BOOT_MARKER' seen on serial"
      exit 0
    fi
    kill -0 $pid 2>/dev/null || { echo "FAIL: QEMU exited early"; tail -20 "$log" 2>/dev/null; exit 1; }
    sleep 1
  done
  echo "FAIL: no '$BOOT_MARKER' within ${BOOT_TIMEOUT}s"
  tail -20 "$log" 2>/dev/null
  exit 1
  ;;
*)
  sed -n 2,3p "$0"
  exit 2
  ;;
esac
