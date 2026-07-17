#!/usr/bin/env bash
# One-time (cached) provisioning of the Windows 11 test VM:
#   fetch ISO -> create disk -> unattended install -> enable SSH ->
#   install cargo-nextest.exe + Node on the guest -> leave it running.
#
# Idempotent at the top level: if $INSTALLED_MARKER exists we assume the image
# is good and just boot it. Delete the marker (or `just win-vm-clean`) to redo.
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

mkdir -p "$CACHE"

if [[ -f "$INSTALLED_MARKER" ]]; then
  echo "✅ Windows image already installed. Booting it."
  exec "$WINVM_SCRIPT_DIR/run-qemu.sh"
fi

if vm_running; then
  echo "❌ a VM is already running (pid $(cat "$QEMU_PIDFILE")) but the image is" >&2
  echo "   not marked installed. Stop it with 'just win-vm-down' first." >&2
  exit 1
fi

require_ovmf
"$WINVM_SCRIPT_DIR/fetch-iso.sh"

# --- host-side artifacts -----------------------------------------------------

if [[ ! -f "$SSHKEY" ]]; then
  echo "🔑 generating SSH keypair for the VM..."
  ssh-keygen -t ed25519 -N '' -C "facet-winvm" -f "$SSHKEY" >/dev/null
fi

echo "💽 creating $DISK_SIZE system disk..."
rm -f "$DISK"
qemu-img create -f qcow2 "$DISK" "$DISK_SIZE" >/dev/null

echo "🧾 copying writable OVMF vars..."
cp -f "$OVMF_FD/OVMF_VARS.fd" "$OVMF_VARS"
chmod u+w "$OVMF_VARS"

echo "📝 rendering autounattend.xml (injecting SSH pubkey)..."
pubkey="$(cat "$SSHKEY.pub")"
unattend_build="$(mktemp -d)"
trap 'rm -rf "$unattend_build"' EXIT
template="$(cat "$WINVM_SCRIPT_DIR/autounattend.xml.template")"
printf '%s' "${template//@@SSH_PUBKEY@@/$pubkey}" > "$unattend_build/autounattend.xml"

echo "📀 building unattend CD..."
xorriso -as mkisofs -J -r -V UNATTEND -o "$UNATTEND_ISO" "$unattend_build" >/dev/null 2>&1

# --- unattended install boot -------------------------------------------------

start_swtpm
build_qemu_args
# Attach install media only for this boot. bootindex prefers the CD so OVMF
# boots setup; the disk becomes bootable once setup populates it.
QEMU_ARGS+=(
  -device "ide-cd,drive=winiso,bus=ahci.1,bootindex=0"
  -drive "file=$ISO,if=none,id=winiso,media=cdrom,readonly=on"
  -device "ide-cd,drive=unattend,bus=ahci.2"
  -drive "file=$UNATTEND_ISO,if=none,id=unattend,media=cdrom,readonly=on"
)
# The system disk defaults to a lower boot priority than the CD above.
QEMU_ARGS+=(-boot menu=off)

echo "🚀 starting unattended install (this takes 20-40 min under emulation)..."
qemu-system-x86_64 "${QEMU_ARGS[@]}" -daemonize

# Answer the "Press any key to boot from CD or DVD" stub — but ONLY for the
# first ~15s. Later setup reboots must fall through to the (now-populated)
# disk, so we deliberately stop pressing after the initial window.
# Send keys long enough to cover a slow OVMF POST/TPM init, but stop well
# before setup's first post-filecopy reboot (minutes away) so we don't bounce
# back into the installer.
echo "⌨️  nudging past the CD-boot prompt..."
for _ in $(seq 1 30); do
  monitor_sendkey spc
  sleep 1
done

echo "⏳ waiting for Windows to install and come up on SSH..."
echo "    (serial log: $SERIAL_LOG)"
wait_for_ssh 3000  # generous: full unattended install under TCG-ish emulation

# FirstLogonCommands drops this once SSH + firewall + key are all in place.
echo "⏳ waiting for guest provisioning marker (C:\\provisioned.txt)..."
for _ in $(seq 1 120); do
  if ssh_win 'if (Test-Path C:\provisioned.txt) { exit 0 } else { exit 1 }' 2>/dev/null; then
    break
  fi
  sleep 5
done

# --- guest-side toolchain (cargo-nextest.exe + Node) -------------------------

echo "🔧 provisioning guest test tooling..."
scp_to_win "$WINVM_SCRIPT_DIR/provision-guest.ps1" 'C:\provision-guest.ps1'
ssh_win 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\provision-guest.ps1'

# --- finish: clean reboot without install media ------------------------------

echo "🔁 finalizing: shutting the VM down and rebooting from disk only..."
ssh_win 'shutdown /s /t 0' 2>/dev/null || true
for _ in $(seq 1 60); do
  vm_running || break
  sleep 2
done
vm_running && { kill "$(cat "$QEMU_PIDFILE")" 2>/dev/null || true; sleep 3; }
stop_swtpm

touch "$INSTALLED_MARKER"
echo "✅ base image installed."

# Leave it running by default, as requested.
exec "$WINVM_SCRIPT_DIR/run-qemu.sh"
