#!/usr/bin/env bash
# Cross-build the nextest archive on this Linux host, ship it + the workspace
# source to the running Windows VM, and run the tests there. Extra args are
# forwarded to `nextest run` on the guest.
#
# Assumes the VM is already installed + running (the Justfile's `test-windows`
# recipe guarantees that via win-vm-up).
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

ARCHIVE="$REPO_ROOT/target/facet-win.tar.zst"
TARGET="x86_64-pc-windows-msvc"

if ! vm_running; then
  echo "❌ VM is not running. Run 'just win-vm-up' first." >&2
  exit 1
fi

# --- 1. cross-build the archive on the host ----------------------------------

echo "🔨 cross-building nextest archive for $TARGET..."
cd "$REPO_ROOT" || exit 1
eval "$(RUST_LOG='' XWIN_ACCEPT_LICENSE=1 cargo xwin env --target "$TARGET" 2>/dev/null)"
cargo nextest archive \
  --target "$TARGET" \
  --features ci \
  --workspace \
  --archive-file "$ARCHIVE"

# --- 2. ship archive + source ------------------------------------------------

echo "📦 packaging workspace source..."
src_tgz="$(mktemp --suffix=.tgz)"
trap 'rm -f "$src_tgz"' EXIT
# Ship the source so nextest can remap CARGO_MANIFEST_DIR-relative fixtures on
# the guest. Skip the heavy, host-only trees.
tar czf "$src_tgz" \
  --exclude='./target' \
  --exclude='*/target' \
  --exclude='./.git' \
  --exclude='node_modules' \
  --exclude='*/node_modules' \
  --exclude='./.paseo' \
  -C "$REPO_ROOT" .

echo "🚚 shipping archive + source to guest..."
ssh_win 'New-Item -ItemType Directory -Force C:\facet | Out-Null'
scp_to_win "$ARCHIVE" 'C:/facet/facet-win.tar.zst'
scp_to_win "$src_tgz" 'C:/facet/source.tgz'
scp_to_win "$WINVM_SCRIPT_DIR/run-tests-guest.ps1" 'C:/facet/run-tests-guest.ps1'

echo "📂 extracting source on guest..."
ssh_win 'Remove-Item -Recurse -Force C:\facet\src -ErrorAction Ignore; New-Item -ItemType Directory -Force C:\facet\src | Out-Null; tar -xzf C:\facet\source.tgz -C C:\facet\src'

# --- 3. run tests on the guest -----------------------------------------------

echo "🧪 running tests on Windows..."
ssh_win "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\facet\\run-tests-guest.ps1 $*"
