# Tier-A scale measurement

Base: `tier-a-scale-measurement`, originally from `d27e2478f` (`Model target cfg facts in vix`), rebased onto `origin/rodin` at `c868cd51a` for the molten sentinel fix.

Question: how close is "vixen resolves and plans the entire monorepo, verified against cargo" as a number?

Answer after the prerelease/root-clause fix: **the composed path now produces a Cargo.lock-diffable real-workspace solve ring.** Full real-workspace resolve/plan scale is still partial, but the previous semantic empty-solve frontier is crossed for workspace-member roots.

- Before this pass, resolve at real scale was **0 / 863 Cargo-resolved package-version rows measured** because there was no vix entrypoint composing workspace manifests into a Rodin `Index`.
- After this pass, the largest measured solve ring is **16 / 145 workspace members -> 17 Rodin package domains + 32 root clauses -> 17 selected rows**, diffed against the real `Cargo.lock`: **16 / 17 solve rows match Cargo.lock**, with the remaining solve-only row being the pseudo workspace root.
- The explicit ignored scale probe still builds the full member-only index: **145 / 145 workspace members -> 146 package domains + 290 root clauses**.
- Tiny composed solves now pass for both stable and prerelease workspace members: **1 / 1 workspace member selected** through manifest tree -> Rodin `Index`/`Problem` -> `solve`, including `facet 0.50.0-rc.5`.
- Real-workspace full solve remains **16 / 863 Cargo-resolved package-version rows diffed against Cargo.lock** at the largest measured ring. The machine-lane `molten handle -1` failure is gone, and the root prerelease empty-solve is fixed; the next frontier is expanding beyond member-only ring 16 into the larger member/direct/transitive rings.
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
- Added Cargo.lock and unit-graph diff harness prep. `scripts/tier-a-scale-measurement.sh` now exports `TIER_A_OUT`, runs the tiny live solve package diff, runs the ring-16 live solve package diff, runs the 4/4 derived-unit fixture diff, and includes the TSV summaries in `summary.txt`.
- After `c868cd51a`, root member clauses now emit the same two-step shape as direct deps: `in_graph` activation plus an exact `version_set` clause for the workspace member's manifest version. This doubled member-only root clauses from one to two per member.
- Fixed Rodin's prerelease root semantics in `rodin.vix`: a default domain now acts as an unconstrained sentinel on the first real `version_set` narrow, and `in_graph` activation marks reachability without re-filtering through Cargo's plain `*` range. The semver differential tests still pin Cargo's rule that plain ranges exclude prereleases, while the unignored `tiny_workspace_prerelease_member_solve_selects_member` now proves exact workspace-root pins admit prereleases.
- Added `workspace_member_only_solve_selected_versions_text_limit` and the ignored `real_workspace_member_only_solve_ring_16_lock_diff` measurement probe so ring solves produce the same `(package, version)` Cargo.lock diff table as the tiny case.

Prepared diff surfaces:

| Surface | Exercised Case | Match | Divergence | Artifact |
|---|---:|---:|---:|---|
| Solve `(package, version)` vs real `Cargo.lock` | tiny live solve: `__workspace__ 0.0.0`, `bytes 1.12.0` | 1 / 2 solve rows | 1 solve-only pseudo-root; 862 Cargo-selected lock rows not in tiny solve; 30 lock residue rows | `/tmp/tier-a-scale-measurement/tiny-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member-only ring 16 | 16 / 17 solve rows | 1 solve-only pseudo-root; 847 Cargo-selected lock rows not in ring 16; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-ring-16-solve-vs-lock-summary.tsv` |
| Derived units vs Cargo `--unit-graph` | `lock_graph` fixture | 4 / 4 units, 3 / 3 edges | 0 machine-only, 0 Cargo-only | `/tmp/tier-a-scale-measurement/lock-fixture-unit-diff-summary.tsv` |

After-run ring table:

| Ring | Cargo / input scale | Vix/Rodin result | Status |
|---|---:|---:|---|
| Tiny composed solve | 1 workspace member | 1 / 1 selected | passes |
| Tiny prerelease composed solve | 1 workspace member, version `0.50.0-rc.5` | 1 / 1 selected | passes; exact root pin admits prerelease |
| Real workspace member-only index, bounded | 16 workspace members | 17 package domains, 32 root clauses | passes |
| Real workspace member-only index, full | 145 workspace members | 146 package domains, 290 root clauses | passes as explicit ignored probe; 79.198s |
| Real workspace member-only solve rings | 1, 2, 4, 8, 16 workspace members | selected member counts equal ring size; ring 16 writes 17 solve rows | passes; ring run 16.613s |
| Real workspace member+direct deps | 145 members, 1,124 direct deps | required workspace-known direct clauses implemented but not measured full-scale after the prerelease fix | next frontier |
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
| Selected package-version rows vs Cargo.lock | 863 selected / 893 locked | 17 ring-16 solve rows | 16 | 847 selected rows not in ring 16; 30 lock residue rows |
| Registry selected rows | 718 selected / 748 locked | 0 registry rows in member-only ring | 0 | 718 unmeasured selected registry rows |
| Lock residue relative to Cargo metadata | 30 | n/a | n/a | 30 lock-only rows |
| Workspace member-only solve ring | 145 members | 16 bounded members solved and diffed; full 145-member index measured explicitly; tiny stable/prerelease solves pass | 16 | 129 workspace members not yet solved in a measured ring |

Full-workspace divergence categories cannot be assigned yet, because the largest rodin-selected real-workspace output is the member-only ring 16, not the full 863-row Cargo closure. At the measured ring, there is no version skew: all 16 real member rows match `Cargo.lock`; the only solve-only row is the pseudo workspace root.

Current source frontier:

- `cargo_manifest.vix` can derive member counts, dependency declarations, cfg data, target shapes one at a time, `problem_of_member`, and a workspace-member Rodin `Index` ring.
- It now exposes `resolved_unit_adaptation_gap()` as: "Path construction is join-only from a granted root; generic ResolvedUnit emission remains blocked by sparse-index composition, UnitTargetTable derivation, and the demanded resolve-to-unit graph bridge."
- `rodin/index.vix` can parse sparse rows and bridge them to an `Index`, but `sparse_index_path` is still a demo hardcoded path table for a small crate set, and the bridge skips optional/dev deps.
- `crate.vix` has `crate_solution_bin[_check]`, but it requires a pre-built `Index`, `Problem`, and `UnitTargetTable`.

Categorization for the current resolve frontier:

| Category | Count / Blast Radius | Evidence |
|---|---:|---|
| Workspace manifest-to-Index composition, bounded | 16 / 145 workspace members solved by default; stable/prerelease tiny solves pass | new `workspace_member_only_*` entrypoints |
| Workspace manifest-to-Index composition, full index | 145 / 145 workspace members index-built explicitly | `real_workspace_member_only_index_builds_all_members`: 146 package domains, 290 root clauses, 79.198s |
| Workspace manifest-to-Index composition, full solve | 16 / 863 Cargo-resolved package-version rows diffed | member-only solve rings 1, 2, 4, 8, and 16 pass; ring 16 diff table has 16 matches, 0 version skew |
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
| Vix real member-only solve rings after prerelease fix | ring 1: 13.896s; 2: 13.734s; 4: 14.152s; 8: 14.499s; 16: 16.580s; alias 16: 16.472s; ring-16 lock diff: 16.598s | all passed in the script run; max RSS not captured |
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
- `real_workspace_member_only_solve_ring_{1,2,4,8,16}`, `real_workspace_member_index_solves_bounded_ring`, and `real_workspace_member_only_solve_ring_16_lock_diff`: 7 passed, 203 skipped; 16.613s in the latest script run. Ring 16 reports 17 solve rows, 893 lock rows, 16 exact matches, 1 solve-only pseudo-root, 877 lock-only rows categorized as 847 Cargo-selected-not-in-solve and 30 lock-residue-not-selected-by-metadata.
- `tiny_workspace_prerelease_member_solve_selects_member`: 1 passed; exact workspace-member root pin selects `facet 0.50.0-rc.5`.
- `solution_walk_derives_units_from_rodin_and_matches_cargo_oracle`: 1 passed, 217 skipped; 13.341s in the latest script run; diff table reports 4 machine units, 4 Cargo units, 4 unit matches, 3 machine edges, 3 Cargo edges, 3 edge matches, and zero machine-only/Cargo-only units or edges.
- `real_workspace_member_only_index_builds_all_members`: 1 passed, 208 skipped; 79.198s for 146 package domains and 290 root clauses.
- Escalated `/usr/bin/time -l cargo nextest run -p vix -E 'test(=real_workspace_dependency_probe_shard_0)'`: 1 passed, 191 skipped; 20.207s test time; 215,793,664-byte max RSS.
- `TIER_A_FETCH_SPARSE=0 scripts/tier-a-scale-measurement.sh`: completed end to end after adding the member solve ring probes and ring-16 lock diff artifact; sparse refetch was disabled because `/tmp/tier-a-scale-measurement/sparse-index` was already fetched and pinned.

Gate status from the gap-closer pass:

| Gate | Result |
|---|---|
| `git fetch origin rodin && git rebase origin/rodin` | completed; picked up `c868cd51a` molten sentinel fix |
| `cargo check --workspace --all-targets` | passed; 11.10s check after waiting for package-cache lock |
| `cargo nextest run -p vix --features real-process` | passed; 206 passed, 12 skipped, 177.221s |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed; 2.99s |
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
- Vix composed workspace-member solve ring: 16 / 145 members solved and diffed; 145 / 145 members still measured as an explicit index-construction probe; stable and prerelease 1 / 1 tiny member solves pass.
- Vix solve-to-unit scale: 4 package fixture, Cargo unit-graph matched.

The next ring is no longer blocked by the prerelease/root-clause empty solve. The current frontier is expanding the composed path beyond member-only ring 16:

1. Measure the next member-only rings past 16 and decide whether the full 145-member solve is now runtime-feasible under the watchdog.
2. Compose workspace-known direct deps into the solved ring and record the first direct-dep divergence table.
3. Compose the pinned sparse snapshot rows into the workspace `Index`, replacing the demo hardcoded `sparse_index_path` surface.
4. Emit `UnitTargetTable` from real manifests using join-only `Path` provenance, then rerun `crate_solution_*` against the full Cargo unit graph.

Until those exist, the numeric answer remains:

```text
resolve match at real scale: 16 / 863 selected Cargo packages measured
unit-graph match at real scale: 0 / 881 Cargo units measured
composed workspace-member solve ring: 16 / 145 members solved and diffed
composed workspace-member full index: 145 / 145 members measured explicitly
composed tiny workspace solve: 1 / 1 stable member selected; 1 / 1 prerelease member selected
largest solve-to-unit match: 4 / 4 fixture units
manifest ingestion match: 1,124 / 1,124 direct workspace deps
```
