# Tier-A scale measurement

Base: `tier-a-scale-measurement`, originally from `d27e2478f` (`Model target cfg facts in vix`), rebased onto `origin/rodin` at `c3156ed5c` for the nextest memory watchdog.

Question: how close is "vixen resolves and plans the entire monorepo, verified against cargo" as a number?

Answer after the gap-closer pass: **the bridge is no longer purely absent, but full real-workspace resolve/plan scale is still not numerically close.** The composed path now exists for the first ring in vix, but it reaches a new scale/runtime frontier before producing a Cargo-diffable full-workspace solve.

- Before this pass, resolve at real scale was **0 / 863 Cargo-resolved package-version rows measured** because there was no vix entrypoint composing workspace manifests into a Rodin `Index`.
- After this pass, the largest default-gated real workspace ring is **16 / 145 workspace members -> 17 Rodin package domains + 16 root clauses** through `workspace -> Index` in `.vix`; the explicit ignored scale probe now builds the full member-only index: **145 / 145 workspace members -> 146 package domains + 145 root clauses**.
- A tiny composed solve now passes: **1 / 1 workspace member selected** through manifest tree -> Rodin `Index`/`Problem` -> `solve`.
- Real-workspace solve remains **0 / 863 Cargo-resolved package-version rows diffed against Cargo.lock**. Real member-only solve rings at 1, 2, 4, 8, and 16 members all hit `molten handle -1`; full 145-member solve remains beyond the current runtime frontier.
- Manifest ingestion at real scale remains **closed for the direct-dependency oracle**. Current tests assert 145 workspace members, 1,122 direct deps, 55 cfg/target deps, 760 legacy allowlist failures retired, and zero name/kind/target mismatches across 16 shards.
- Unit derivation at real scale: **0 / 880 Cargo unit-graph units measured through recursive `unit()` at real scale**, for the same missing composition plus the still-pinned `ResolvedUnit` adaptation gap.
- Largest fully wired solve-to-unit path: **4 packages / 4 units** in the `lock_graph` fixture, verified against Cargo `--unit-graph`.

## Gap-Closer Delta

New vix/Rust surfaces:

- Added pure host/lowerer support for `Doc.keys() -> [String]`, sorted for deterministic dependency-table enumeration.
- Fixed `Path.join` for an empty granted root so `Path("") .join("member")` yields relative `member` instead of absolute `/member`.
- Added `cargo_manifest.vix` workspace bridge state that derives Rodin `Index` fields from real manifests: pseudo workspace root, member package/version rows, root `selected -> in_graph` clauses, and a direct-dependency clause bridge for required workspace-known deps.
- Added member-only and member+direct ring entrypoints so the harness can measure the frontier without Rust-side composition bypasses.
- Added sparse-index fetch/pin script: `scripts/fetch-tier-a-sparse-index.sh`.

After-run ring table:

| Ring | Cargo / input scale | Vix/Rodin result | Status |
|---|---:|---:|---|
| Tiny composed solve | 1 workspace member | 1 / 1 selected | passes |
| Real workspace member-only index, bounded | 16 workspace members | 17 package domains, 16 root clauses | passes |
| Real workspace member-only index, full | 145 workspace members | 146 package domains, 145 root clauses | passes as explicit ignored probe; 84.413s |
| Real workspace member-only solve rings | 1, 2, 4, 8, 16 workspace members | 0 selected diffable rows | blocked: `molten handle -1` at every ring |
| Real workspace member+direct deps | 145 members, 1,122 direct deps | required workspace-known direct clauses implemented but not measured full-scale | blocked: direct-clause construction timeout / bounded direct solve `molten handle -1` |
| Crates.io sparse closure | 643 crate sparse files + `config.json` for 718 registry package-version rows | fetched and pinned; not yet composed into the workspace `Index` | next ring |

Sparse snapshot fetched from `https://index.crates.io`:

| Measure | Value |
|---|---:|
| snapshot manifest rows | 644 |
| crate sparse files fetched | 643 |
| config rows fetched | 1 |
| registry package-version rows in Cargo metadata | 718 |
| snapshot size | 32M |
| manifest | `/tmp/tier-a-scale-measurement/sparse-index/snapshot-manifest.tsv` |
| manifest sha256 | `fdc863bfc1e7bb1bf043090d770b2f13cc070ca1ad316c5363c8daa29240bd80` |

The snapshot manifest records each fetched URL, sparse path, byte count, and sha256.

## Oracles Captured

Commands:

```sh
cargo metadata --locked --format-version 1
cargo +nightly build --unit-graph -Z unstable-options --workspace --locked
```

Artifacts from the first oracle capture:

- `/tmp/tier-a-metadata.json`
- `/tmp/tier-a-unit-graph.json`

The reusable harness writes the same captures under `${TIER_A_OUT:-/tmp/tier-a-scale-measurement}`. This pass fetched and pinned a sparse-index snapshot under `/tmp/tier-a-scale-measurement/sparse-index`.

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
| Selected package-version rows vs Cargo.lock | 863 selected / 893 locked | 0 full-workspace solve rows | 0 | 863 unmeasured selected rows |
| Registry selected rows | 718 selected / 748 locked | 0 full-workspace rows | 0 | 718 unmeasured selected rows |
| Lock residue relative to Cargo metadata | 30 | n/a | n/a | 30 lock-only rows |
| Workspace member-only index ring | 145 members | 16 bounded members measured by default; full 145-member index measured explicitly; 1 tiny solve | n/a | real solve rings not reached |

No divergence categories can be assigned to rodin-selected full-workspace rows yet, because there are no rodin-selected full-workspace rows to diff. The first blocker is no longer total absence of a workspace `Index` bridge; it is now the language/runtime and interpreted-scale frontier hit while carrying that bridge past the bounded member ring.

Current source frontier:

- `cargo_manifest.vix` can derive member counts, dependency declarations, cfg data, target shapes one at a time, `problem_of_member`, and a workspace-member Rodin `Index` ring.
- It now exposes `resolved_unit_adaptation_gap()` as: "Path construction is join-only from a granted root; generic ResolvedUnit emission remains blocked by sparse-index composition, UnitTargetTable derivation, and the demanded resolve-to-unit graph bridge."
- `rodin/index.vix` can parse sparse rows and bridge them to an `Index`, but `sparse_index_path` is still a demo hardcoded path table for a small crate set, and the bridge skips optional/dev deps.
- `crate.vix` has `crate_solution_bin[_check]`, but it requires a pre-built `Index`, `Problem`, and `UnitTargetTable`.

Categorization for the current resolve frontier:

| Category | Count / Blast Radius | Evidence |
|---|---:|---|
| Workspace manifest-to-Index composition, bounded | 16 / 145 workspace members index-built by default; 1 / 1 tiny solve | new `workspace_member_only_*` entrypoints |
| Workspace manifest-to-Index composition, full index | 145 / 145 workspace members index-built explicitly | `real_workspace_member_only_index_builds_all_members`: 146 package domains, 145 root clauses, 84.413s |
| Workspace manifest-to-Index composition, full solve | 863 Cargo-resolved package-version rows still undiffed | real solve rings 1, 2, 4, 8, and 16 all hit `molten handle -1` |
| Sparse-index live path not generic in vix | 718 registry rows need lookup/snapshot composition | snapshot fetched/pinned externally; `sparse_index_path` still hardcodes demo crates |
| Optional/dev/features in sparse bridge incomplete | 61 workspace feature sections, 299 cfg-gated dep-kinds in metadata; registry feature closure unmeasured | `bridge_dep` skips optional and dev; feature maps are present but not populated by sparse rows |
| Cargo.lock residue | 30 lock-only rows | lock-vs-metadata diff above |
| Index snapshot skew | not measured | no new live snapshot was fetched; used local Cargo cache/oracles only |
| Solver behavior divergence | not measured | no full-workspace rodin answer exists to classify |

Performance:

| Command | Wall | Max RSS |
|---|---:|---:|
| Cargo metadata oracle | 2.83s | 188,416,000 bytes |
| Cargo unit-graph oracle | 1.12s | 169,164,800 bytes |
| Vix real-workspace dependency shard 0/16, before bridge growth | 20.207s test time / 23.22s wall | 215,793,664 bytes |
| Vix shard 0 after rebase/watchdog, in script | 8.696s test time | not captured |
| Vix composed member ring after rebase/watchdog, in script | 3.703s for 16-member index probe | not captured |
| Vix full member-only index ignored probe | 84.413s for 145-member index probe | not captured |
| Vix real member-only solve rings | ring 1: 4.193s; 2: 4.422s; 4: 4.192s; 8: 4.650s; 16: 5.407s; alias 16: 5.196s | all failed before RSS capture with `molten handle -1` |
| Vix tiny composed solve sentinel | included in `projected_member_manifests_are_read_from_granted_root`, 2.862s | not captured |

The vix performance number is not a solve-at-scale number. The bridge now exists for the member-only ring, and the measured frontier moved to interpreted-vix scale/runtime behavior before full Cargo-diffable solve output.

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

- `real_workspace_metadata_baseline_is_counted`, `real_workspace_dependency_probe_shard_0`, `direct_resolved_unit_adapter_gap_is_pinned`: 3 passed, 196 skipped; shard 0 took 8.696s in the post-rebase script run.
- `projected_member_manifests_are_read_from_granted_root`, `dependency_declarations_extract_workspace_and_detailed_forms`, `real_workspace_member_only_index_builds_bounded_ring`: 3 passed, 196 skipped; bounded member ring took 3.703s.
- `solution_walk_derives_units_from_rodin_and_matches_cargo_oracle`: 1 passed, 206 skipped; 6.205s.
- `real_workspace_member_only_index_builds_all_members`: 1 passed, 203 skipped; 84.413s for 146 package domains and 145 root clauses.
- `real_workspace_member_only_solve_ring_{1,2,4,8,16}` plus the bounded-ring alias: all failed with `Error: "molten handle -1"`; no Cargo-lock-diffable selected rows.
- Escalated `/usr/bin/time -l cargo nextest run -p vix -E 'test(=real_workspace_dependency_probe_shard_0)'`: 1 passed, 191 skipped; 20.207s test time; 215,793,664-byte max RSS.
- `scripts/tier-a-scale-measurement.sh`: completed end to end after adding the nextest measurement timeout override; sparse refetch was disabled for the final rerun because `/tmp/tier-a-scale-measurement/sparse-index` was already fetched and pinned.

Gate status from the gap-closer pass:

| Gate | Result |
|---|---|
| `git fetch origin rodin && git rebase origin/rodin` | completed; picked up `c3156ed5c` memcap wrapper |
| `cargo check --workspace --all-targets` | passed after rebase |
| `cargo nextest run -p vix --features real-process -E 'test(=tail_loop_array_accumulator_handles_100k_iterations)'` | failed loudly under watchdog: `MEMCAP EXCEEDED`, RSS 7,073,744KB > 6,144MB, exit 137 |
| `cargo nextest run -p vix --features real-process` | not rerun after the watchdog landed, per environment warning not to rerun the full vix suite yet |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed after disk was freed and after rebase |
| `git diff --check` | passed |

Reusable harness:

```sh
scripts/tier-a-scale-measurement.sh
```

It writes captures and derived diffs under `${TIER_A_OUT:-/tmp/tier-a-scale-measurement}`.

## Precise Frontier

Largest reachable subgraph today:

- Cargo oracle scale: 863 package-version rows, 880 units.
- Vix real-manifest ingestion/projection scale: 145 workspace members and 1,122 direct deps, sharded.
- Vix composed workspace-member `Index` ring: 16 / 145 members by default gate; 145 / 145 members as an explicit ignored probe; 1 / 1 tiny member solve.
- Vix solve-to-unit scale: 4 package fixture, Cargo unit-graph matched.

The next ring is blocked by vix runtime/scale gaps while exercising the new composition path, not by a measured Cargo divergence:

1. Minimize `molten handle -1` from `workspace_member_solve_selected_member_count_limit`; it reproduces from the real 1-member ring upward, while the synthetic 1-member solve passes.
2. Keep full 145-member member-only index construction in the ignored scale probe until interpreted-vix performance or the incoming carried-hasher fold makes it suitable for the default harness.
3. Compose the pinned sparse snapshot rows into the workspace `Index`, replacing the demo hardcoded `sparse_index_path` surface.
4. Emit `UnitTargetTable` from real manifests using join-only `Path` provenance, then rerun `crate_solution_*` against the full Cargo unit graph.

Until those exist, the numeric answer remains:

```text
resolve match at real scale: 0 / 863 selected Cargo packages measured
unit-graph match at real scale: 0 / 880 Cargo units measured
composed workspace-member index ring: 16 / 145 members measured by default gate
composed workspace-member full index: 145 / 145 members measured explicitly
composed tiny workspace solve: 1 / 1 member selected
largest solve-to-unit match: 4 / 4 fixture units
manifest ingestion match: 1,122 / 1,122 direct workspace deps
```
