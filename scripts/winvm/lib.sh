#!/usr/bin/env bash
# Shared config + helpers for the QEMU Windows test VM.
# Sourced by the other scripts in this dir; not meant to be run directly.
#
# Everything the VM needs (qemu, swtpm, xorriso, openssh, the OVMF firmware
# path in $OVMF_FD) comes from the nix dev shell — see flake.nix. Run the
# winvm scripts through `nix develop --command` (the Justfile does this).

set -euo pipefail

WINVM_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$WINVM_SCRIPT_DIR" rev-parse --show-toplevel)"

# Persistent, cached state. The disk image and ISO live here so the VM
# survives across runs and we never re-download / re-install unnecessarily.
CACHE="${FACET_WINVM_CACHE:-$HOME/.cache/facet-winvm}"
ISO="$CACHE/windows.iso"
DISK="$CACHE/disk.qcow2"
UNATTEND_ISO="$CACHE/unattend.iso"
OVMF_VARS="$CACHE/OVMF_VARS.fd"
SSHKEY="$CACHE/id_ed25519"
QEMU_PIDFILE="$CACHE/qemu.pid"
MONITOR_SOCK="$CACHE/monitor.sock"
SERIAL_LOG="$CACHE/serial.log"
SWTPM_DIR="$CACHE/swtpm"
SWTPM_SOCK="$SWTPM_DIR/sock"
SWTPM_PIDFILE="$CACHE/swtpm.pid"
INSTALLED_MARKER="$CACHE/.installed"

# Guest access. The unattended install (autounattend.xml.template) creates this
# admin user and installs our key into administrators_authorized_keys.
WIN_USER="runner"
SSH_PORT="${FACET_WINVM_SSH_PORT:-2222}"

# VM sizing. The facet suite is heavy; give it real cores + RAM. Overridable.
VM_CPUS="${FACET_WINVM_CPUS:-6}"
VM_RAM="${FACET_WINVM_RAM:-12G}"
DISK_SIZE="${FACET_WINVM_DISK_SIZE:-80G}"

# Direct MS Evaluation Center fwlink for "Windows 11 Enterprise Evaluation,
# English (US), 64-bit". fwlink 301-redirects straight to the ISO on
# download-cdn — no registration gate on the file itself. Override with
# FACET_WINVM_ISO_URL if this linkid rotates (they do, ~yearly).
WIN_ISO_URL="${FACET_WINVM_ISO_URL:-https://go.microsoft.com/fwlink/?linkid=2334167&clcid=0x409&culture=en-us&country=us}"

SSH_OPTS=(
  -o StrictHostKeyChecking=no
  -o UserKnownHostsFile=/dev/null
  -o LogLevel=ERROR
  -o ConnectTimeout=5
  -i "$SSHKEY"
)

ssh_win() {
  ssh "${SSH_OPTS[@]}" -p "$SSH_PORT" "$WIN_USER@localhost" "$@"
}

scp_to_win() {
  # scp_to_win <local> <remote>
  scp "${SSH_OPTS[@]}" -P "$SSH_PORT" "$1" "$WIN_USER@localhost:$2"
}

scp_from_win() {
  # scp_from_win <remote> <local>
  scp "${SSH_OPTS[@]}" -P "$SSH_PORT" "$WIN_USER@localhost:$1" "$2"
}

vm_running() {
  [[ -f "$QEMU_PIDFILE" ]] && kill -0 "$(cat "$QEMU_PIDFILE")" 2>/dev/null
}

# Block until the guest answers SSH, or fail after ~timeout seconds.
wait_for_ssh() {
  local timeout="${1:-1800}" waited=0
  echo "⏳ waiting for SSH on port $SSH_PORT (up to ${timeout}s)..." >&2
  while (( waited < timeout )); do
    if ssh_win -o ConnectTimeout=4 -o BatchMode=yes 'exit 0' 2>/dev/null; then
      echo "✅ guest is reachable over SSH" >&2
      return 0
    fi
    sleep 5
    waited=$((waited + 5))
  done
  echo "❌ timed out waiting for SSH after ${timeout}s" >&2
  return 1
}

require_ovmf() {
  if [[ -z "${OVMF_FD:-}" || ! -f "$OVMF_FD/OVMF_CODE.fd" ]]; then
    echo "❌ OVMF firmware not found (OVMF_FD='$OVMF_FD'). Run via 'nix develop'." >&2
    exit 1
  fi
}

# Start the emulated TPM 2.0 that Win11 setup insists on, as a background
# socket daemon. Idempotent.
start_swtpm() {
  if [[ -f "$SWTPM_PIDFILE" ]] && kill -0 "$(cat "$SWTPM_PIDFILE")" 2>/dev/null; then
    return 0
  fi
  mkdir -p "$SWTPM_DIR"
  swtpm socket \
    --tpmstate "dir=$SWTPM_DIR" \
    --ctrl "type=unixio,path=$SWTPM_SOCK" \
    --tpm2 \
    --daemon \
    --pid "file=$SWTPM_PIDFILE"
}

stop_swtpm() {
  [[ -f "$SWTPM_PIDFILE" ]] || return 0
  kill "$(cat "$SWTPM_PIDFILE")" 2>/dev/null || true
  rm -f "$SWTPM_PIDFILE"
}

# Populate the QEMU_ARGS global with everything common to install + normal
# boot. Devices are deliberately the ones stock Windows has drivers for:
# AHCI/SATA disk + CD (not virtio), e1000 NIC (not virtio-net). The user-mode
# NIC forwards host :$SSH_PORT to guest :22.
build_qemu_args() {
  require_ovmf
  QEMU_ARGS=(
    -machine "q35,smm=on"
    -accel kvm
    -cpu host
    -smp "$VM_CPUS"
    -m "$VM_RAM"

    # UEFI: read-only firmware code + a per-VM writable vars store.
    -drive "if=pflash,format=raw,unit=0,readonly=on,file=$OVMF_FD/OVMF_CODE.fd"
    -drive "if=pflash,format=raw,unit=1,file=$OVMF_VARS"

    # TPM 2.0 over the swtpm socket.
    -chardev "socket,id=chrtpm,path=$SWTPM_SOCK"
    -tpmdev "emulator,id=tpm0,chardev=chrtpm"
    -device "tpm-crb,tpmdev=tpm0"

    # System disk on AHCI.
    -device "ahci,id=ahci"
    -drive "file=$DISK,if=none,id=hd0,format=qcow2,cache=writeback"
    -device "ide-hd,drive=hd0,bus=ahci.0"

    # NIC with SSH port-forward. e1000 driver ships in Windows.
    -netdev "user,id=net0,hostfwd=tcp:127.0.0.1:$SSH_PORT-:22"
    -device "e1000,netdev=net0"

    -device qemu-xhci
    -device usb-tablet

    -pidfile "$QEMU_PIDFILE"
    -monitor "unix:$MONITOR_SOCK,server,nowait"
    -serial "file:$SERIAL_LOG"
    -display none
  )
}

# Send a key to the guest via the QEMU HMP monitor socket. Used to answer the
# "Press any key to boot from CD" stub during the very first install boot.
monitor_sendkey() {
  echo "sendkey $1" | socat - "unix-connect:$MONITOR_SOCK" >/dev/null 2>&1 || true
}
