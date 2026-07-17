#!/usr/bin/env bash
# Download + cache the Windows 11 Enterprise Evaluation ISO.
# Idempotent: if a plausible ISO is already cached, does nothing.
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

mkdir -p "$CACHE"

# Treat anything >= 3 GiB as a real ISO; the MS fwlink returns a tiny HTML
# error page when a gate trips, and we don't want to feed that to QEMU.
iso_looks_valid() {
  [[ -f "$ISO" ]] || return 1
  local bytes
  bytes="$(stat -c %s "$ISO")"
  (( bytes > 3 * 1024 * 1024 * 1024 ))
}

if iso_looks_valid; then
  echo "✅ Windows ISO already cached ($(du -h "$ISO" | cut -f1)) at $ISO"
  exit 0
fi

echo "⬇️  fetching Windows 11 Enterprise Evaluation ISO..."
echo "    from: $WIN_ISO_URL"
echo "    (override with FACET_WINVM_ISO_URL, or drop your own ISO at $ISO)"

# aria2 follows the fwlink redirects, resumes partial downloads, and parallel
# -splits to saturate the link. Download to a temp name so an aborted run
# never leaves a half-file that passes the size check.
aria2c \
  --continue=true \
  --max-connection-per-server=8 \
  --split=8 \
  --min-split-size=16M \
  --auto-file-renaming=false \
  --allow-overwrite=true \
  --dir="$CACHE" \
  --out="windows.iso.part" \
  "$WIN_ISO_URL"

mv -f "$CACHE/windows.iso.part" "$ISO"

if ! iso_looks_valid; then
  echo "❌ downloaded file is too small to be a Windows ISO — the fwlink" >&2
  echo "   probably rotated. Set FACET_WINVM_ISO_URL to a current link from" >&2
  echo "   https://www.microsoft.com/evalcenter, or place an ISO at $ISO" >&2
  rm -f "$ISO"
  exit 1
fi

echo "✅ ISO cached ($(du -h "$ISO" | cut -f1)) at $ISO"
