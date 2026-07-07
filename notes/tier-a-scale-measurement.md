# Tier-A scale measurement

Base: `tier-a-scale-measurement`, originally from `d27e2478f` (`Model target cfg facts in vix`), rebased onto `origin/rodin` at `c868cd51a` for the molten sentinel fix.

Question: how close is "vixen resolves and plans the entire monorepo, verified against cargo" as a number?

Answer after the gap-closer pass: **the bridge is no longer purely absent, but full real-workspace resolve/plan scale is still not numerically close.** The composed path now exists for the first ring in vix, but it reaches a new scale/runtime frontier before producing a Cargo-diffable full-workspace solve.

- Before this pass, resolve at real scale was **0 / 863 Cargo-resolved package-version rows measured** because there was no vix entrypoint composing workspace manifests into a Rodin `Index`.
- After this pass, the largest default-gated real workspace ring is **16 / 145 workspace members -> 17 Rodin package domains + 32 root clauses** through `workspace -> Index` in `.vix`; the explicit ignored scale probe now builds the full member-only index: **145 / 145 workspace members -> 146 package domains + 290 root clauses**.
- A tiny composed solve now passes: **1 / 1 workspace member selected** through manifest tree -> Rodin `Index`/`Problem` -> `solve`.
- Real-workspace solve remains **0 / 863 Cargo-resolved package-version rows diffed against Cargo.lock**. The machine-lane `molten handle -1` failure is gone; real member-only solve rings at 1, 2, 4, 8, and 16 members now return a semantic empty solve (`selected_count = 0`, member count `-1`).
- Manifest ingestion at real scale remains **closed for the direct-dependency oracle**. Current tests assert 145 workspace members, 1,124 direct deps, 55 cfg/target deps, 760 legacy allowlist failures retired, and zero name/kind/target mismatches across 16 shards.
- Unit derivation at real scale: **0 / 881 Cargo unit-graph units measured through recursive `unit()` at real scale**, for the same missing composition plus the still-pinned `ResolvedUnit` adaptation gap.
- Largest fully wired solve-to-unit path: **4 packages / 4 units** in the `lock_graph` fixture, verified against Cargo `--unit-graph`.

## Gap-Closer Delta

New vix/Rust surfaces:

- Added pure host/lowerer support for `Doc.keys() -> [String]`, sorted for deterministic dependency-table enumeration.
- Fixed `Path.join` for an empty granted root so `Path("") .join("member")` yields relative `member` instead of absolute `/member`.
- Added `cargo_manifest.vix` workspace bridge state that derives Rodin `Index` fields from real manifests: pseudo workspace root, member package/version rows, root `selected -> in_graph` clauses, and a direct-dependency clause bridge for required workspace-known deps.
- Added member-only and member+direct ring entrypoints so the harness can measure the frontier without Rust-side composition bypasses.
- Added sparse-index fetch/pin script: `scripts/fetch-tier-a-sparse-index.sh`.
- Added Cargo.lock and unit-graph diff harness prep. `scripts/tier-a-scale-measurement.sh` now exports `TIER_A_OUT`, runs the tiny live solve package diff, runs the 4/4 derived-unit fixture diff, and includes both TSV summaries in `summary.txt`.
- After `c868cd51a`, root member clauses now emit the same two-step shape as direct deps: `in_graph` activation plus an exact `version_set` clause for the workspace member's manifest version. This doubled member-only root clauses from one to two per member. The prerelease tiny repro still solves empty and is committed as the ignored probe `tiny_workspace_prerelease_member_solve_selects_member`.

Prepared diff surfaces:

| Surface | Exercised Case | Match | Divergence | Artifact |
|---|---:|---:|---:|---|
| Solve `(package, version)` vs real `Cargo.lock` | tiny live solve: `__workspace__ 0.0.0`, `bytes 1.12.0` | 1 / 2 solve rows | 1 solve-only pseudo-root; 862 Cargo-selected lock rows not in tiny solve; 30 lock residue rows | `/tmp/tier-a-scale-measurement/tiny-solve-vs-lock-summary.tsv` |
| Derived units vs Cargo `--unit-graph` | `lock_graph` fixture | 4 / 4 units, 3 / 3 edges | 0 machine-only, 0 Cargo-only | `/tmp/tier-a-scale-measurement/lock-fixture-unit-diff-summary.tsv` |

After-run ring table:

| Ring | Cargo / input scale | Vix/Rodin result | Status |
|---|---:|---:|---|
| Tiny composed solve | 1 workspace member | 1 / 1 selected | passes |
| Tiny prerelease composed solve | 1 workspace member, version `0.50.0-rc.5` | 0 selected rows; member count `-1` | semantic empty-solve repro |
| Real workspace member-only index, bounded | 16 workspace members | 17 package domains, 32 root clauses | passes |
| Real workspace member-only index, full | 145 workspace members | 146 package domains, 290 root clauses | passes as explicit ignored probe; 79.198s |
| Real workspace member-only solve rings | 1, 2, 4, 8, 16 workspace members | 0 selected diffable rows; member count `-1` | semantic empty solve at every ring |
| Real workspace member+direct deps | 145 members, 1,124 direct deps | required workspace-known direct clauses implemented but not measured full-scale | blocked behind member-only semantic empty solve |
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
| resolve deps | 2,795 |
| cfg-gated resolve dep-kinds | 299 |
| registry package-version rows | 718 |
| path package-version rows | 145 |

Cargo unit graph stats:

| Measure | Count |
|---|---:|
| units | 881 |
| roots | 156 |
| dependency edges | 2,478 |
| build-mode units | 805 |
| run-custom-build units | 76 |
| custom-build target-kind units | 152 |
| proc-macro target-kind units | 48 |
| lib target-kind units | 654 |
| bin target-kind units | 20 |

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
| Workspace manifest-to-Index composition, full index | 145 / 145 workspace members index-built explicitly | `real_workspace_member_only_index_builds_all_members`: 146 package domains, 290 root clauses, 79.198s |
| Workspace manifest-to-Index composition, full solve | 863 Cargo-resolved package-version rows still undiffed | real solve rings 1, 2, 4, 8, and 16 return semantic empty solve after the machine fix |
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
| Vix shard 0 after `c868cd51a`, in script | 13.084s test time | not captured |
| Vix composed member ring after root-clause correction, in script | 3.994s for 16-member index probe | not captured |
| Vix full member-only index ignored probe | 79.198s for 145-member index probe | not captured |
| Vix real member-only solve rings | ring 1: 4.367s; 2: 4.283s; 4: 4.288s; 8: 4.242s; 16: 4.949s; alias 16: 4.923s | all returned semantic empty solve (`-1` member count) |
| Vix tiny composed solve sentinel | included in `projected_member_manifests_are_read_from_granted_root`, 3.199s | not captured |

The vix performance number is not a solve-at-scale number. The bridge now exists for the member-only ring, and the measured frontier moved to interpreted-vix scale/runtime behavior before full Cargo-diffable solve output.

## Unit Derivation At Scale vs --unit-graph

| Projection | Cargo | Vix measured | Match | Divergence |
|---|---:|---:|---:|---:|
| Units | 881 | 0 full-workspace units | 0 | 881 unmeasured units |
| Unit dependency edges | 2,478 | 0 full-workspace edges | 0 | 2,478 unmeasured edges |
| `(package, target-kind, features)` shapes | 801 unique | 0 full-workspace shapes | 0 | 801 unmeasured shapes |
| Fixture solve-to-unit path | 4 packages / 4 units | 4 packages / 4 units | 4 | 0 on fixture |

Known Cargo unit categories that vix must eventually account for at scale:

| Category | Cargo Count | Status |
|---|---:|---|
| build-script companion units (`custom-build` build + run) | 152 custom-build target-kind units; 76 run-custom-build units | counted as a gap category; do not chase profile payload here |
| proc-macro host units | 48 | counted as a gap category |
| profile fields | 881 units carry profile payloads | known absent category per mission brief |
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

- `tiny_workspace_solve_diff_is_categorized_against_real_cargo_lock`: 1 passed, 204 skipped; tiny solve diff table reports 2 solve rows, 893 lock rows, 1 exact match, 1 solve-only pseudo-root, 892 lock-only rows categorized as 862 Cargo-selected-not-in-solve and 30 lock-residue-not-selected-by-metadata.
- `real_workspace_metadata_baseline_is_counted`, `real_workspace_dependency_probe_shard_0`, `direct_resolved_unit_adapter_gap_is_pinned`: 3 passed, 206 skipped; shard 0 took 13.084s in the post-`c868cd51a` script run.
- `projected_member_manifests_are_read_from_granted_root`, `dependency_declarations_extract_workspace_and_detailed_forms`, `real_workspace_member_only_index_builds_bounded_ring`: 3 passed, 206 skipped; bounded member ring took 3.994s.
- `tiny_workspace_solve_diff_is_categorized_against_real_cargo_lock`: 1 passed, 208 skipped; tiny solve diff table reports 2 solve rows, 893 lock rows, 1 exact match, 1 solve-only pseudo-root, 892 lock-only rows categorized as 862 Cargo-selected-not-in-solve and 30 lock-residue-not-selected-by-metadata.
- `solution_walk_derives_units_from_rodin_and_matches_cargo_oracle`: 1 passed, 216 skipped; 6.492s in the latest script run; diff table reports 4 machine units, 4 Cargo units, 4 unit matches, 3 machine edges, 3 Cargo edges, 3 edge matches, and zero machine-only/Cargo-only units or edges.
- `real_workspace_member_only_index_builds_all_members`: 1 passed, 208 skipped; 79.198s for 146 package domains and 290 root clauses.
- `tiny_workspace_prerelease_member_solve_selects_member`: ignored repro failed with selected member count `-1` instead of `1`.
- `real_workspace_member_only_solve_ring_{1,2,4,8,16}` plus the bounded-ring alias: all failed with selected member count `-1`; no Cargo-lock-diffable selected rows.
- Escalated `/usr/bin/time -l cargo nextest run -p vix -E 'test(=real_workspace_dependency_probe_shard_0)'`: 1 passed, 191 skipped; 20.207s test time; 215,793,664-byte max RSS.
- `scripts/tier-a-scale-measurement.sh`: completed end to end after adding the nextest measurement timeout override and diff artifacts; sparse refetch was disabled for the final rerun because `/tmp/tier-a-scale-measurement/sparse-index` was already fetched and pinned.

Gate status from the gap-closer pass:

| Gate | Result |
|---|---|
| `git fetch origin rodin && git rebase origin/rodin` | completed; picked up `c868cd51a` molten sentinel fix |
| `cargo check --workspace --all-targets` | passed after rebase; 30.92s |
| `cargo nextest run -p vix --features real-process` | passed after `c868cd51a`; 205 passed, 12 skipped, 337.253s |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed after rebase; 15.08s |
| `git diff --check` | passed |

Reusable harness:

```sh
scripts/tier-a-scale-measurement.sh
```

It writes captures and derived diffs under `${TIER_A_OUT:-/tmp/tier-a-scale-measurement}`.

Rerun sequence when the machine fix lands:

```sh
git fetch origin rodin
git rebase origin/rodin
TIER_A_FETCH_SPARSE=0 scripts/tier-a-scale-measurement.sh
```

Use `TIER_A_FETCH_SPARSE=1` only when intentionally refreshing the pinned sparse snapshot; record the new `snapshot-manifest.tsv` sha256 if it changes.

Once the branch already contains the machine fix, the remeasurement command is:

```sh
TIER_A_FETCH_SPARSE=0 scripts/tier-a-scale-measurement.sh
```

## Precise Frontier

Largest reachable subgraph today:

- Cargo oracle scale: 863 package-version rows, 881 units.
- Vix real-manifest ingestion/projection scale: 145 workspace members and 1,124 direct deps, sharded.
- Vix composed workspace-member `Index` ring: 16 / 145 members by default gate; 145 / 145 members as an explicit ignored probe; 1 / 1 tiny member solve.
- Vix solve-to-unit scale: 4 package fixture, Cargo unit-graph matched.

The next ring is blocked by vix runtime/scale gaps while exercising the new composition path, not by a measured Cargo divergence:

1. Diagnose the semantic empty solve from `workspace_member_solve_selected_member_count_limit`; it reproduces from the real 1-member ring upward and from the ignored tiny prerelease repro, while the stable synthetic 1-member solve passes.
2. Keep full 145-member member-only index construction in the ignored scale probe until interpreted-vix performance or the incoming carried-hasher fold makes it suitable for the default harness.
3. Compose the pinned sparse snapshot rows into the workspace `Index`, replacing the demo hardcoded `sparse_index_path` surface.
4. Emit `UnitTargetTable` from real manifests using join-only `Path` provenance, then rerun `crate_solution_*` against the full Cargo unit graph.

Until those exist, the numeric answer remains:

```text
resolve match at real scale: 0 / 863 selected Cargo packages measured
unit-graph match at real scale: 0 / 881 Cargo units measured
composed workspace-member index ring: 16 / 145 members measured by default gate
composed workspace-member full index: 145 / 145 members measured explicitly
composed tiny workspace solve: 1 / 1 member selected
largest solve-to-unit match: 4 / 4 fixture units
manifest ingestion match: 1,124 / 1,124 direct workspace deps
```
