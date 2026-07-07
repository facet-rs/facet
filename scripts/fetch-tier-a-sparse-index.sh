#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${TIER_A_SPARSE_OUT:-/tmp/tier-a-scale-measurement/sparse-index}"
BASE_URL="${TIER_A_SPARSE_BASE_URL:-https://index.crates.io}"

mkdir -p "$OUT/index"

metadata="${1:-$OUT/../metadata.json}"
if [[ ! -f "$metadata" ]]; then
  cargo metadata --locked --format-version 1 > "$metadata"
fi

sparse_path() {
  local name="$1"
  local len="${#name}"
  if [[ "$len" -eq 1 ]]; then
    printf '1/%s\n' "$name"
  elif [[ "$len" -eq 2 ]]; then
    printf '2/%s\n' "$name"
  elif [[ "$len" -eq 3 ]]; then
    printf '3/%s/%s\n' "${name:0:1}" "$name"
  else
    printf '%s/%s/%s\n' "${name:0:2}" "${name:2:2}" "$name"
  fi
}

manifest="$OUT/snapshot-manifest.tsv"
: > "$manifest"

config="$OUT/config.json"
curl -fsSL --retry 3 "$BASE_URL/config.json" -o "$config"
config_sha="$(shasum -a 256 "$config" | awk '{print $1}')"
config_bytes="$(wc -c < "$config" | tr -d ' ')"
printf '%s\t%s\t%s\t%s\n' "$BASE_URL/config.json" "config.json" "$config_bytes" "$config_sha" >> "$manifest"

jq -r '.packages[] | select(.source != null and (.source | startswith("registry+"))) | .name' "$metadata" \
  | sort -u \
  | while IFS= read -r name; do
      path="$(sparse_path "$name")"
      dest="$OUT/index/$path"
      mkdir -p "$(dirname "$dest")"
      curl -fsSL --retry 3 "$BASE_URL/$path" -o "$dest"
      sha="$(shasum -a 256 "$dest" | awk '{print $1}')"
      bytes="$(wc -c < "$dest" | tr -d ' ')"
      printf '%s\t%s\t%s\t%s\n' "$BASE_URL/$path" "$path" "$bytes" "$sha" >> "$manifest"
    done

sort -o "$manifest" "$manifest"
printf 'sparse snapshot: %s\n' "$OUT"
printf 'manifest: %s\n' "$manifest"
