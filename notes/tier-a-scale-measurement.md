# Tier-A scale measurement

Base: `tier-a-scale-measurement` at `d27e2478f` (`Model target cfg facts in vix`).

Question: how close is "vixen resolves and plans the entire monorepo, verified against cargo" as a number?

Answer: **not numerically close yet at full resolve/plan scale, because the full real-workspace resolve-to-unit bridge is not present.** The current measurable vix frontier is:

- Manifest ingestion at real scale: **closed for the direct-dependency oracle**. Current tests assert 145 workspace members, 1,122 direct deps, 55 cfg/target deps, 760 legacy allowlist failures retired, and zero name/kind/target mismatches across 16 shards.
- Resolve at real scale: **0 / 863 Cargo-resolved package-version rows measured through a real workspace vix solve**, because there is no vix entrypoint that composes real workspace manifests + sparse rows into one `Index`/`Problem`.
- Unit derivation at real scale: **0 / 880 Cargo unit-graph units measured through recursive `unit()` at real scale**, for the same missing composition plus the still-pinned `ResolvedUnit` adaptation gap.
- Largest fully wired solve-to-unit path: **4 packages / 4 units** in the `lock_graph` fixture, verified against Cargo `--unit-graph`.

## Oracles Captured

Commands:

```sh
cargo metadata --locked --format-version 1
cargo +nightly build --unit-graph -Z unstable-options --workspace --locked
```

Artifacts from the first oracle capture:

- `/tmp/tier-a-metadata.json`
- `/tmp/tier-a-unit-graph.json`

The reusable harness writes the same captures under `${TIER_A_OUT:-/tmp/tier-a-scale-measurement}`. I did not fetch a new sparse-index snapshot in this pass because the vix-side full-workspace `Index` composition is the first blocker; the Cargo oracles used the local locked workspace and Cargo cache.

Cargo metadata stats:

| Measure | Count |
|---|---:|
| packages | 863 |
| workspace members | 145 |
| resolve nodes | 863 |
| resolve deps | 2,793 |
| cfg-gated resolve dep-kinds | 299 |
| registry package-version rows | 718 |
| path package-version rows | 145 |

Cargo unit graph stats:

| Measure | Count |
|---|---:|
| units | 880 |
| roots | 155 |
| dependency edges | 2,460 |
| build-mode units | 804 |
| run-custom-build units | 76 |
| custom-build target-kind units | 152 |
| proc-macro target-kind units | 48 |
| lib target-kind units | 654 |
| bin target-kind units | 19 |

Cargo.lock projection:

| Set | Count |
|---|---:|
| Cargo.lock package-version rows | 893 |
| Cargo metadata selected package-version rows | 863 |
| Matched package-version rows | 863 |
| Lock-only package-version rows | 30 |
| Metadata-only package-version rows | 0 |
| Cargo.lock registry package-version rows | 748 |
| Cargo metadata registry package-version rows | 718 |
| Registry lock-only rows | 30 |

The 30 lock-only rows are Cargo.lock residue not selected by `cargo metadata --locked` on this host/feature set:

```text
aws-lc-rs 1.17.0
aws-lc-sys 0.41.0
bincode 1.3.3
caseless 0.2.2
chardetng 0.1.17
cmake 0.1.58
darling 0.21.3
darling_core 0.21.3
darling_macro 0.21.3
dunce 1.0.5
encoding_rs 0.8.35
fs_extra 1.3.0
hashbrown 0.13.2
jobserver 0.1.34
language-tags 0.3.2
lexical-parse-float 1.0.6
lexical-parse-integer 1.0.6
lexical-util 1.0.7
oem_cp 2.1.2
oval 2.0.0
ownable 0.6.2
ownable-macro 0.6.3
positioned-io 0.3.5
rc-zip 5.4.1
rc-zip-sync 4.4.2
ruint 1.18.0
ruint-macro 1.2.1
rustls-pemfile 2.2.0
valuable 0.1.1
wordfreq 0.2.3
```

## Resolve At Scale vs Cargo.lock

| Projection | Cargo | Vix/Rodin measured | Match | Divergence |
|---|---:|---:|---:|---:|
| Selected package-version rows vs Cargo.lock | 863 selected / 893 locked | 0 full-workspace rows | 0 | 863 unmeasured selected rows |
| Registry selected rows | 718 selected / 748 locked | 0 full-workspace rows | 0 | 718 unmeasured selected rows |
| Lock residue relative to Cargo metadata | 30 | n/a | n/a | 30 lock-only rows |

No divergence categories can be assigned to rodin-selected rows yet, because there are no rodin-selected full-workspace rows to diff. The first blocker is not a bad solver answer; it is the absent vix composition from ingested manifests and sparse-index rows into one workspace `Index`/`Problem`.

Current source frontier:

- `cargo_manifest.vix` can derive member counts, dependency declarations, cfg data, target shapes one at a time, and `problem_of_member`.
- It still exposes `resolved_unit_adaptation_gap()` as: "Path construction is join-only from a granted root; generic ResolvedUnit emission remains blocked by dependency-table key enumeration and the demanded resolve-to-unit graph bridge."
- `rodin/index.vix` can parse sparse rows and bridge them to an `Index`, but `sparse_index_path` is still a demo hardcoded path table for a small crate set, and the bridge skips optional/dev deps.
- `crate.vix` has `crate_solution_bin[_check]`, but it requires a pre-built `Index`, `Problem`, and `UnitTargetTable`.

Categorization for the current resolve frontier:

| Category | Count / Blast Radius | Evidence |
|---|---:|---|
| Missing workspace manifest-to-Index composition | 863 Cargo-resolved package-version rows unreachable | no `workspace -> Index` entrypoint; `problem_of_member` only accepts already-known ids |
| Sparse-index live path not generic | 718 registry rows need lookup/snapshot plumbing | `sparse_index_path` hardcodes demo crates |
| Optional/dev/features in sparse bridge incomplete | 61 workspace feature sections, 299 cfg-gated dep-kinds in metadata; registry feature closure unmeasured | `bridge_dep` skips optional and dev; feature maps are present but not populated by sparse rows |
| Cargo.lock residue | 30 lock-only rows | lock-vs-metadata diff above |
| Index snapshot skew | not measured | no new live snapshot was fetched; used local Cargo cache/oracles only |
| Solver behavior divergence | not measured | no full-workspace rodin answer exists to classify |

Performance:

| Command | Wall | Max RSS |
|---|---:|---:|
| Cargo metadata oracle | 2.83s | 188,416,000 bytes |
| Cargo unit-graph oracle | 1.12s | 169,164,800 bytes |
| Vix real-workspace dependency shard 0/16, warm artifacts | 20.207s test time / 23.22s wall | 215,793,664 bytes |
| Vix same shard in the 3-probe run | 89.810s test time | not captured |

The vix performance number is not a solve-at-scale number. It is the largest current real-workspace manifest probe. Full interpreted-vix solve over 863 packages is unreachable until the bridge exists.

## Unit Derivation At Scale vs --unit-graph

| Projection | Cargo | Vix measured | Match | Divergence |
|---|---:|---:|---:|---:|
| Units | 880 | 0 full-workspace units | 0 | 880 unmeasured units |
| Unit dependency edges | 2,460 | 0 full-workspace edges | 0 | 2,460 unmeasured edges |
| `(package, target-kind, features)` shapes | 801 unique | 0 full-workspace shapes | 0 | 801 unmeasured shapes |
| Fixture solve-to-unit path | 4 packages / 4 units | 4 packages / 4 units | 4 | 0 on fixture |

Known Cargo unit categories that vix must eventually account for at scale:

| Category | Cargo Count | Status |
|---|---:|---|
| build-script companion units (`custom-build` build + run) | 152 custom-build target-kind units; 76 run-custom-build units | counted as a gap category; do not chase profile payload here |
| proc-macro host units | 48 | counted as a gap category |
| profile fields | 880 units carry profile payloads | known absent category per mission brief |
| non-lib/bin crate types | 9 cdylib, 4 rlib, 1 staticlib crate-type entries | unmeasured at vix scale |

`crate.vix` recursive unit derivation is real but fixture-scoped: `solution_unit` recursively computes deps from `SolveResult`, and `crate_solution_bin[_check]` calls `solve(index, problem, target_name)`. The current fixture proves the shape over `mini_app -> alpha_lib -> core_lib` plus `formatting_lib`, not over the monorepo.

## Scoped Verification

```sh
cargo nextest list -p vix -E 'test(=real_workspace_metadata_baseline_is_counted) | test(=real_workspace_dependency_probe_shard_0) | test(=direct_resolved_unit_adapter_gap_is_pinned)'
cargo nextest run -p vix -E 'test(=real_workspace_metadata_baseline_is_counted) | test(=real_workspace_dependency_probe_shard_0) | test(=direct_resolved_unit_adapter_gap_is_pinned)'
cargo nextest list -p vix --features real-process -E 'test(=solution_walk_derives_units_from_rodin_and_matches_cargo_oracle)'
cargo nextest run -p vix --features real-process -E 'test(=solution_walk_derives_units_from_rodin_and_matches_cargo_oracle)'
```

Results:

- `real_workspace_metadata_baseline_is_counted`, `real_workspace_dependency_probe_shard_0`, `direct_resolved_unit_adapter_gap_is_pinned`: 3 passed, 189 skipped; shard 0 took 89.810s in that run.
- `solution_walk_derives_units_from_rodin_and_matches_cargo_oracle`: 1 passed, 199 skipped; 11.165s.
- Escalated `/usr/bin/time -l cargo nextest run -p vix -E 'test(=real_workspace_dependency_probe_shard_0)'`: 1 passed, 191 skipped; 20.207s test time; 215,793,664-byte max RSS.
- `scripts/tier-a-scale-measurement.sh`: completed end to end after stderr capture was fixed; frontier set 3 passed, 189 skipped with shard 0 at 72.482s; derived-unit fixture 1 passed, 199 skipped at 32.146s.

Reusable harness:

```sh
scripts/tier-a-scale-measurement.sh
```

It writes captures and derived diffs under `${TIER_A_OUT:-/tmp/tier-a-scale-measurement}`.

## Precise Frontier

Largest reachable subgraph today:

- Cargo oracle scale: 863 package-version rows, 880 units.
- Vix real-manifest probe scale: 145 workspace members and 1,122 direct deps, sharded; this is ingestion/projection, not solve.
- Vix solve-to-unit scale: 4 package fixture, Cargo unit-graph matched.

The next ring is blocked by a vix-side composition gap, not by a measured Cargo divergence:

1. Add vix dependency-table key enumeration / package id assignment so real workspace manifests can produce the package/version/clause rows for `Index`.
2. Generalize sparse-index lookup/snapshot ingestion beyond the demo path table, pin the fetched snapshot, and record the row set.
3. Compose workspace path packages and crates.io rows into one `Index`, with features/cfg gates populated from the manifest ingestion layer.
4. Emit `UnitTargetTable` from real manifests using join-only `Path` provenance, then rerun `crate_solution_*` against the full Cargo unit graph.

Until those exist, the numeric answer remains:

```text
resolve match at real scale: 0 / 863 selected Cargo packages measured
unit-graph match at real scale: 0 / 880 Cargo units measured
largest solve-to-unit match: 4 / 4 fixture units
manifest ingestion match: 1,122 / 1,122 direct workspace deps
```
