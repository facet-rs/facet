#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail=false

check_forbidden_outside_allowlist() {
  local pattern="$1"
  local label="$2"
  shift 2
  local allow_files=("$@")

  local matches
  matches="$(rg -n "$pattern" Sources Tests || true)"
  if [[ -z "$matches" ]]; then
    return
  fi

  local disallowed=""
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    local file="${line%%:*}"
    local allowed=false
    for allow in "${allow_files[@]}"; do
      if [[ "$file" == "$allow" ]]; then
        allowed=true
        break
      fi
    done
    if [[ "$allowed" == false ]]; then
      disallowed+="$line"$'\n'
    fi
  done <<< "$matches"

  if [[ -n "$disallowed" ]]; then
    echo "hardening check failed: disallowed $label found outside allowlist"
    echo "$disallowed"
    fail=true
  fi
}

check_forbidden_outside_allowlist \
  '@preconcurrency import' \
  '@preconcurrency import' \
  'Sources/RoamRuntime/Transport.swift' \
  'Tests/RoamRuntimeTests/TransportTests.swift'

check_forbidden_outside_allowlist \
  '@unchecked Sendable' \
  '@unchecked Sendable' \
  'Sources/RoamRuntime/Binding.swift' \
  'Sources/RoamRuntime/Channel.swift' \
  'Sources/RoamRuntime/Driver.swift' \
  'Sources/RoamRuntime/ShmBipBuffer.swift' \
  'Sources/RoamRuntime/ShmGuest.swift' \
  'Sources/RoamRuntime/ShmRegion.swift' \
  'Sources/RoamRuntime/ShmTransport.swift' \
  'Sources/RoamRuntime/Transport.swift' \
  'Sources/shm-guest-client/main.swift'

check_forbidden_outside_allowlist \
  '(^|[^[:alnum:]_])(Unmanaged|Unsafe[A-Za-z0-9_]*|withUnsafe[A-Za-z0-9_]*)' \
  'unsafe APIs' \
  'Sources/RoamRuntime/Postcard.swift' \
  'Sources/RoamRuntime/ShmAtomics.swift' \
  'Sources/RoamRuntime/ShmBipBuffer.swift' \
  'Sources/RoamRuntime/ShmBootstrap.swift' \
  'Sources/RoamRuntime/ShmGuest.swift' \
  'Sources/RoamRuntime/ShmRegion.swift' \
  'Sources/RoamRuntime/ShmTransport.swift' \
  'Tests/RoamRuntimeTests/ShmBootstrapTests.swift' \
  'Tests/RoamRuntimeTests/ShmGuestRuntimeTests.swift'

if [[ "$fail" == true ]]; then
  exit 1
fi

echo "hardening check passed"
