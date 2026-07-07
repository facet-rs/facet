#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${TIER_A_OUT:-/tmp/tier-a-scale-measurement}"
mkdir -p "$OUT"

cd "$ROOT"

nextest_timeout=(
  --config 'profile.default.slow-timeout.period = "600s"'
  --config 'profile.default.slow-timeout.terminate-after = 2'
)

metadata="$OUT/metadata.json"
unit_graph="$OUT/unit-graph.json"
sparse_out="$OUT/sparse-index"

cargo metadata --locked --format-version 1 > "$metadata"
cargo +nightly build --unit-graph -Z unstable-options --workspace --locked > "$unit_graph"

if [[ "${TIER_A_FETCH_SPARSE:-1}" != "0" ]]; then
  TIER_A_SPARSE_OUT="$sparse_out" scripts/fetch-tier-a-sparse-index.sh "$metadata" \
    > "$OUT/sparse-fetch.txt" 2>&1
fi

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

cargo nextest list "${nextest_timeout[@]}" -p vix -E 'test(=real_workspace_metadata_baseline_is_counted) | test(=real_workspace_dependency_probe_shard_0) | test(=direct_resolved_unit_adapter_gap_is_pinned)' \
  > "$OUT/vix-frontier-list.txt" 2>&1
cargo nextest run "${nextest_timeout[@]}" -p vix -E 'test(=real_workspace_metadata_baseline_is_counted) | test(=real_workspace_dependency_probe_shard_0) | test(=direct_resolved_unit_adapter_gap_is_pinned)' \
  > "$OUT/vix-frontier-run.txt" 2>&1

cargo nextest list "${nextest_timeout[@]}" -p vix -E 'test(=projected_member_manifests_are_read_from_granted_root) | test(=dependency_declarations_extract_workspace_and_detailed_forms) | test(=real_workspace_member_only_index_builds_bounded_ring)' \
  > "$OUT/vix-composed-member-ring-list.txt" 2>&1
cargo nextest run "${nextest_timeout[@]}" -p vix -E 'test(=projected_member_manifests_are_read_from_granted_root) | test(=dependency_declarations_extract_workspace_and_detailed_forms) | test(=real_workspace_member_only_index_builds_bounded_ring)' \
  > "$OUT/vix-composed-member-ring-run.txt" 2>&1

cargo nextest list "${nextest_timeout[@]}" -p vix --features real-process -E 'test(=solution_walk_derives_units_from_rodin_and_matches_cargo_oracle)' \
  > "$OUT/vix-derived-unit-list.txt" 2>&1
cargo nextest run "${nextest_timeout[@]}" -p vix --features real-process -E 'test(=solution_walk_derives_units_from_rodin_and_matches_cargo_oracle)' \
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
  if [[ -f "$sparse_out/snapshot-manifest.tsv" ]]; then
    echo
    echo "sparse snapshot:"
    wc -l "$sparse_out/snapshot-manifest.tsv"
    du -sh "$sparse_out"
    shasum -a 256 "$sparse_out/snapshot-manifest.tsv"
  fi
} > "$OUT/summary.txt"

cat "$OUT/summary.txt"
