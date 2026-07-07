#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${TIER_A_OUT:-/tmp/tier-a-scale-measurement}"
mkdir -p "$OUT"

cd "$ROOT"

metadata="$OUT/metadata.json"
unit_graph="$OUT/unit-graph.json"

cargo metadata --locked --format-version 1 > "$metadata"
cargo +nightly build --unit-graph -Z unstable-options --workspace --locked > "$unit_graph"

jq '{packages: (.packages|length), workspace_members: (.workspace_members|length), resolve_nodes: (.resolve.nodes|length), resolve_edges: ([.resolve.nodes[].deps[]?]|length), cfg_edges: ([.resolve.nodes[].deps[]?.dep_kinds[]? | select(.target != null)]|length), registry_packages: ([.packages[] | select(.source != null)]|length), path_packages: ([.packages[] | select(.source == null)]|length)}' "$metadata" \
  > "$OUT/metadata-stats.json"

jq '{units: (.units|length), roots: (.roots|length), build: ([.units[]|select(.mode=="build")]|length), run_custom_build: ([.units[]|select(.mode=="run-custom-build")]|length), custom_build: ([.units[]|select(.target.kind|index("custom-build"))]|length), proc_macro: ([.units[]|select(.target.kind|index("proc-macro"))]|length), lib: ([.units[]|select(.target.kind|index("lib"))]|length), bin: ([.units[]|select(.target.kind|index("bin"))]|length), edges: ([.units[].dependencies[]?]|length)}' "$unit_graph" \
  > "$OUT/unit-graph-stats.json"

jq -r '.packages[] | [.name,.version] | @tsv' "$metadata" | sort -u > "$OUT/metadata-pkgs.tsv"
jq -r '.packages[] | select(.source != null) | [.name,.version] | @tsv' "$metadata" | sort -u > "$OUT/metadata-registry-pkgs.tsv"

awk 'BEGIN{name="";version=""}
  /^name = /{name=$3; gsub(/"/,"",name)}
  /^version = /{version=$3; gsub(/"/,"",version)}
  /^\[\[package\]\]/{if(name!="") print name "\t" version; name=""; version=""}
  END{if(name!="") print name "\t" version}' Cargo.lock | sort -u > "$OUT/lock-pkgs.tsv"

awk 'BEGIN{name="";version="";source=""}
  /^name = /{name=$3; gsub(/"/,"",name)}
  /^version = /{version=$3; gsub(/"/,"",version)}
  /^source = /{source=$3; gsub(/"/,"",source)}
  /^\[\[package\]\]/{if(name!="" && source!="") print name "\t" version; name=""; version=""; source=""}
  END{if(name!="" && source!="") print name "\t" version}' Cargo.lock | sort -u > "$OUT/lock-registry-pkgs.tsv"

comm -23 "$OUT/lock-pkgs.tsv" "$OUT/metadata-pkgs.tsv" > "$OUT/lock-only.tsv"
comm -13 "$OUT/lock-pkgs.tsv" "$OUT/metadata-pkgs.tsv" > "$OUT/metadata-only.tsv"
comm -23 "$OUT/lock-registry-pkgs.tsv" "$OUT/metadata-registry-pkgs.tsv" > "$OUT/lock-only-registry.tsv"
comm -13 "$OUT/lock-registry-pkgs.tsv" "$OUT/metadata-registry-pkgs.tsv" > "$OUT/metadata-only-registry.tsv"

cargo nextest list -p vix -E 'test(=real_workspace_metadata_baseline_is_counted) | test(=real_workspace_dependency_probe_shard_0) | test(=direct_resolved_unit_adapter_gap_is_pinned)' \
  > "$OUT/vix-frontier-list.txt" 2>&1
cargo nextest run -p vix -E 'test(=real_workspace_metadata_baseline_is_counted) | test(=real_workspace_dependency_probe_shard_0) | test(=direct_resolved_unit_adapter_gap_is_pinned)' \
  > "$OUT/vix-frontier-run.txt" 2>&1

cargo nextest list -p vix --features real-process -E 'test(=solution_walk_derives_units_from_rodin_and_matches_cargo_oracle)' \
  > "$OUT/vix-derived-unit-list.txt" 2>&1
cargo nextest run -p vix --features real-process -E 'test(=solution_walk_derives_units_from_rodin_and_matches_cargo_oracle)' \
  > "$OUT/vix-derived-unit-run.txt" 2>&1

{
  echo "Artifacts: $OUT"
  echo
  echo "metadata stats:"
  cat "$OUT/metadata-stats.json"
  echo
  echo "unit graph stats:"
  cat "$OUT/unit-graph-stats.json"
  echo
  echo "set counts:"
  wc -l "$OUT"/metadata-pkgs.tsv "$OUT"/lock-pkgs.tsv "$OUT"/metadata-registry-pkgs.tsv "$OUT"/lock-registry-pkgs.tsv "$OUT"/lock-only.tsv "$OUT"/metadata-only.tsv
} > "$OUT/summary.txt"

cat "$OUT/summary.txt"
