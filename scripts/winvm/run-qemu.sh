#!/usr/bin/env bash
# Boot the already-installed Windows VM (daemonized) and leave it running.
# Idempotent: if the VM is already up, just reports that.
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

if vm_running; then
  echo "✅ Windows VM already running (pid $(cat "$QEMU_PIDFILE"), ssh port $SSH_PORT)"
  exit 0
fi

if [[ ! -f "$INSTALLED_MARKER" ]]; then
  echo "❌ no installed VM found. Run 'just win-vm-build' first." >&2
  exit 1
fi

start_swtpm
build_qemu_args

echo "🚀 booting Windows VM (daemonized)..."
qemu-system-x86_64 "${QEMU_ARGS[@]}" -daemonize

wait_for_ssh 300
echo "✅ Windows VM up — ssh -p $SSH_PORT $WIN_USER@localhost (key: $SSHKEY)"
