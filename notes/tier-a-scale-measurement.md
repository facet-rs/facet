# Tier-A scale measurement

Base: `tier-a-scale-measurement`, originally from `d27e2478f` (`Model target cfg facts in vix`), rebased onto `origin/rodin` at `c868cd51a` for the molten sentinel fix.

Question: how close is "vixen resolves and plans the entire monorepo, verified against cargo" as a number?

Answer after the prerelease/root-clause fix: **the composed path now produces a Cargo.lock-diffable real-workspace solve ring.** Full real-workspace resolve/plan scale is still partial, but the previous semantic empty-solve frontier is crossed for workspace-member roots.

- Before this pass, resolve at real scale was **0 / 863 Cargo-resolved package-version rows measured** because there was no vix entrypoint composing workspace manifests into a Rodin `Index`.
- After the widening pass, the largest measured solve ring is **64 / 145 workspace members -> 65 selected rows**, diffed against the real `Cargo.lock`: **64 / 65 solve rows match Cargo.lock**, with the remaining solve-only row being the pseudo workspace root.
- The explicit ignored scale probe still builds the full member-only index: **145 / 145 workspace members -> 146 package domains + 290 root clauses**.
- Tiny composed solves now pass for both stable and prerelease workspace members: **1 / 1 workspace member selected** through manifest tree -> Rodin `Index`/`Problem` -> `solve`, including `facet 0.50.0-rc.5`.
- Real-workspace full solve remains **64 / 863 Cargo-resolved package-version rows diffed against Cargo.lock** at the largest measured member-only ring. The machine-lane `molten handle -1` failure is gone, and the root prerelease empty-solve is fixed; the 145-member solve hit the wall-clock frontier at 180.020s.
- The first small direct-deps sparse ring now runs: **8 workspace members + 16 direct crates.io sparse files -> 25 package domains, 22 clauses, 10 selected rows**, diffed against `Cargo.lock` with **9 exact matches, 0 version-skew packages, and 1 expected pseudo-root solve-only row**.
- Manifest ingestion at real scale remains **closed for the direct-dependency oracle**. Current tests assert 145 workspace members, 1,124 direct deps, 55 cfg/target deps, 760 legacy allowlist failures retired, and zero name/kind/target mismatches across 16 shards.
- Unit derivation at real scale: **0 / 881 Cargo unit-graph units measured through recursive `unit()` at real scale**, for the same missing composition plus the still-pinned `ResolvedUnit` adaptation gap.
- Largest fully wired solve-to-unit path: **4 packages / 4 units** in the `lock_graph` fixture, verified against Cargo `--unit-graph`.

## Gap-Closer Delta

New vix/Rust surfaces:

- Added pure host/lowerer support for `Doc.keys() -> [String]`, sorted for deterministic dependency-table enumeration.
- Fixed `Path.join` for an empty granted root so `Path("") .join("member")` yields relative `member` instead of absolute `/member`.
- Added `cargo_manifest.vix` workspace bridge state that derives Rodin `Index` fields from real manifests: pseudo workspace root, member package/version rows, root `selected -> in_graph` clauses, and a direct-dependency clause bridge for required workspace-known deps.
- Added member-only and member+direct ring entrypoints so the harness can measure the frontier without Rust-side composition bypasses.
- Added sparse JSONL row parsing in `cargo_manifest.vix` and direct sparse ring probes that compose selected real workspace members plus direct crates.io rows from the pinned sparse snapshot into one Rodin `Index`.
- Added sparse-index fetch/pin script: `scripts/fetch-tier-a-sparse-index.sh`.
- Added Cargo.lock and unit-graph diff harness prep. `scripts/tier-a-scale-measurement.sh` now exports `TIER_A_OUT`, runs the tiny live solve package diff, runs the member-ring live solve package diffs through ring 64 by default, runs the 4/4 derived-unit fixture diff, and includes the TSV summaries in `summary.txt`. The timed-out 145-member solve is opt-in via `TIER_A_FULL_MEMBER_RING=1`.
- After `c868cd51a`, root member clauses now emit the same two-step shape as direct deps: `in_graph` activation plus an exact `version_set` clause for the workspace member's manifest version. This doubled member-only root clauses from one to two per member.
- Fixed Rodin's prerelease root semantics in `rodin.vix`: a default domain now acts as an unconstrained sentinel on the first real `version_set` narrow, and `in_graph` activation marks reachability without re-filtering through Cargo's plain `*` range. The semver differential tests still pin Cargo's rule that plain ranges exclude prereleases, while the unignored `tiny_workspace_prerelease_member_solve_selects_member` now proves exact workspace-root pins admit prereleases.
- Added `workspace_member_only_solve_selected_versions_text_limit` and ignored ring lock-diff measurement probes for 16, 32, 64, and 145 so ring solves produce the same `(package, version)` Cargo.lock diff table as the tiny case.

Prepared diff surfaces:

| Surface | Exercised Case | Match | Divergence | Artifact |
|---|---:|---:|---:|---|
| Solve `(package, version)` vs real `Cargo.lock` | tiny live solve: `__workspace__ 0.0.0`, `bytes 1.12.0` | 1 / 2 solve rows | 1 solve-only pseudo-root; 862 Cargo-selected lock rows not in tiny solve; 30 lock residue rows | `/tmp/tier-a-scale-measurement/tiny-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member-only ring 16 | 16 / 17 solve rows | 1 solve-only pseudo-root; 847 Cargo-selected lock rows not in ring 16; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-ring-16-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member-only ring 32 | 32 / 33 solve rows | 1 solve-only pseudo-root; 831 Cargo-selected lock rows not in ring 32; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-ring-32-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member-only ring 64 | 64 / 65 solve rows | 1 solve-only pseudo-root; 799 Cargo-selected lock rows not in ring 64; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-ring-64-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member+direct sparse ring 8 | 9 / 10 solve rows | 1 solve-only pseudo-root; 0 version skew; 854 Cargo-selected lock rows not in ring 8; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-direct-ring-8-solve-vs-lock-summary.tsv` |
| Derived units vs Cargo `--unit-graph` | `lock_graph` fixture | 4 / 4 units, 3 / 3 edges | 0 machine-only, 0 Cargo-only | `/tmp/tier-a-scale-measurement/lock-fixture-unit-diff-summary.tsv` |

After-run ring table:

| Ring | Cargo / input scale | Vix/Rodin result | Status |
|---|---:|---:|---|
| Tiny composed solve | 1 workspace member | 1 / 1 selected | passes |
| Tiny prerelease composed solve | 1 workspace member, version `0.50.0-rc.5` | 1 / 1 selected | passes; exact root pin admits prerelease |
| Real workspace member-only index, bounded | 16 workspace members | 17 package domains, 32 root clauses | passes |
| Real workspace member-only index, full | 145 workspace members | 146 package domains, 290 root clauses | passes as explicit ignored probe; 79.198s |
| Real workspace member-only solve rings | 1, 2, 4, 8, 16, 32, 64 workspace members | selected member counts equal ring size; ring 64 writes 65 solve rows | passes through 64; 145 timed out at 180.020s |
| Real workspace member+direct deps, small sparse ring | 8 workspace members plus direct crates.io rows from 16 sparse files | 25 package domains, 22 clauses, 989 sparse rows, 10 selected rows | passes; first divergence table captured |
| Real workspace member+direct deps, full workspace | 145 members, 1,124 direct deps | required workspace-known direct clauses implemented; full direct sparse solve not attempted after the 145-member member-only wall-clock timeout | pending perf frontier |
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
| Selected package-version rows vs Cargo.lock | 863 selected / 893 locked | 65 ring-64 solve rows | 64 | 799 selected rows not in ring 64; 30 lock residue rows |
| Registry selected rows | 718 selected / 748 locked | 0 registry rows in member-only ring | 0 | 718 unmeasured selected registry rows |
| Lock residue relative to Cargo metadata | 30 | n/a | n/a | 30 lock-only rows |
| Workspace member-only solve ring | 145 members | 64 bounded members solved and diffed; full 145-member index measured explicitly; tiny stable/prerelease solves pass | 64 | 81 workspace members not yet solved in a completed ring |
| Workspace member+direct sparse solve ring | 8 members + direct sparse candidates | 10 solve rows | 9 | 1 pseudo-root row; remaining Cargo rows outside the small ring |

Full-workspace divergence categories cannot be assigned yet, because the largest rodin-selected real-workspace output is the member-only ring 64, not the full 863-row Cargo closure. At the measured ring, there is no version skew: all 64 real member rows match `Cargo.lock`; the only solve-only row is the pseudo workspace root.

First direct sparse divergence table:

| Category | Count | Evidence | Current read |
|---|---:|---|---|
| exact solve rows matching Cargo.lock | 9 | `facet-showcase`, `peer-server`, `strid`, `strid-examples`, `strid-macros`, `tokio`, `vox-phon`, `wasm-browser-tests`, `wasm-inprocess-tests` | all workspace members in the 8-member ring plus the first direct crates.io package match lock versions |
| solve-only pseudo-root | 1 | `__workspace__ 0.0.0` | harness root sentinel, expected |
| version skew | 0 | Rodin now renders/selects `tokio 1.52.3`, matching Cargo.lock | the emitted direct `version_set` req was `1`, not wildcard; narrowing was intact |
| Cargo-selected rows outside the ring | 854 | lock rows selected by metadata but not selected by this ring | expected small-ring residue |
| Cargo.lock residue not selected by metadata | 30 | same lock-vs-metadata residue as the full oracle | expected lock residue |

Tokio skew diagnosis:

| Half Checked | Result | Evidence |
|---|---|---|
| requirement narrowing | intact | `real-direct-ring-8-tokio-narrowing.tsv` records `tokio_emitted_req = 1`; the direct bridge did not emit `*` |
| candidate preference / sparse row order | fixed | sparse JSONL rows are now registered into the `Index` in file order; Rodin preserves that ascending candidate order so Vix back-pop tries the newest admissible row first. `real-direct-ring-8-tokio-candidates.txt` starts with `1.52.3`, and the selected row is `tokio 1.52.3` |

Current source frontier:

- `cargo_manifest.vix` can derive member counts, dependency declarations, cfg data, target shapes one at a time, `problem_of_member`, and a workspace-member Rodin `Index` ring.
- It now exposes `resolved_unit_adaptation_gap()` as: "Path construction is join-only from a granted root; generic ResolvedUnit emission remains blocked by sparse-index composition, UnitTargetTable derivation, and the demanded resolve-to-unit graph bridge."
- `rodin/index.vix` can parse sparse rows and bridge them to an `Index`, but `sparse_index_path` is still a demo hardcoded path table for a small crate set, and the bridge skips optional/dev deps.
- `crate.vix` has `crate_solution_bin[_check]`, but it requires a pre-built `Index`, `Problem`, and `UnitTargetTable`.

Categorization for the current resolve frontier:

| Category | Count / Blast Radius | Evidence |
|---|---:|---|
| Workspace manifest-to-Index composition, bounded | 64 / 145 workspace members solved by default; stable/prerelease tiny solves pass | new `workspace_member_only_*` entrypoints |
| Workspace manifest-to-Index composition, full index | 145 / 145 workspace members index-built explicitly | `real_workspace_member_only_index_builds_all_members`: 146 package domains, 290 root clauses, 79.198s |
| Workspace manifest-to-Index composition, full solve | 64 / 863 Cargo-resolved package-version rows diffed | member-only solve rings pass through 64; ring 64 diff table has 64 matches, 0 version skew; ring 145 timed out at 180.020s |
| Workspace member+direct sparse composition | 8-member direct sparse ring solved and diffed | 16 direct sparse files, 989 sparse rows, 25 packages, 22 clauses, 10 solve rows; 9 matches, 0 version skew |
| Sparse-index live path not generic in vix | 718 registry rows need lookup/snapshot composition | snapshot fetched/pinned externally; direct sparse ring currently feeds JSONL rows from the harness rather than vix fetching sparse paths itself |
| Optional/dev/features in sparse bridge incomplete | 61 workspace feature sections, 299 cfg-gated dep-kinds in metadata; registry feature closure unmeasured | `bridge_dep` skips optional and dev; feature maps are present but not populated by sparse rows |
| Cargo.lock residue | 30 lock-only rows | lock-vs-metadata diff above |
| Index snapshot skew | not measured | no new live snapshot was fetched; used local Cargo cache/oracles only |
| Solver behavior divergence | 0 in the measured ring-8 direct sparse table | the prior `tokio 0.0.0` row was not a wildcard requirement; after preserving sparse row order and making candidate search try the newest admissible row first, ring 8 has no version skew |

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
| Vix widened member-only lock diffs | isolated: ring 32 10.636s, ring 64 27.594s; default script: ring 32 8.304s, ring 64 21.724s; ring 145 timed out at 180.020s | 32 and 64 passed with 0 version skew; 145 stopped before direct-dep expansion |
| Vix direct sparse lock diff ring 8 | 51.797s before fix; latest fixed rerun 57.496s (129.839s on the first post-clean rerun) | 989 sparse rows from 16 direct crates.io sparse files; now 9 matches, 0 version skew |
| Vix member-only ring 32, interpreter lane | 6.539s | default `Machine::load`/measurement lane is `Lane::Interp` |
| Vix member-only ring 32, JIT lane | 6.577s | near parity with interpreter; host-call/memo/load trunk dominates this workload |
| Vix tiny composed solve sentinel | included in `projected_member_manifests_are_read_from_granted_root`, 3.199s | not captured |

The vix performance number is not a solve-at-scale number. The bridge now exists for the member-only ring, and the measured frontier moved to interpreted-vix scale/runtime behavior before full Cargo-diffable solve output.

Ring 32 solve lane and stax profile:

| Measurement | Result |
|---|---|
| default execution lane | interpreter; `Machine::load` lowers through `Lane::Interp` |
| one-line JIT switch | `Machine::load_with_lane(..., Lane::Jit)` in the ignored ring-32 probe |
| interpreter ring 32 | 6.539s |
| JIT ring 32 | 6.577s |
| stax run | run 25, ring-32 interpreter probe, 5,820 kperf samples / 4,474 intervals, target finished in 7.36s |
| stax time split | 1.007s active CPU, 28.251s off-CPU across sampled threads |

The flame does not show a solve-specific `version_set` or propagation loop trunk. The visible trunk is machine/module/host infrastructure:

| Stack / leaf | Active time | Read |
|---|---:|---|
| `Machine::demand_i64` / `Driver::demand` | 536.15ms, 53.2% | demand execution trunk |
| `Driver::burst` -> `LaneTask::advance` -> `weavy::task::Task::run_hosted` | 318.09ms, 31.6% | hosted task/driver burst path |
| `Driver::projection_memo_hit` -> `verify_projection_read_set` -> observation hashing | 69.04ms, 6.9% | memo/projection read-set verification and value observation hashing |
| `intern_molten_word` / `ValueStore::alloc_map` / canonical map pairs | 31.02ms plus 24.29ms under burst | value interning and map churn |
| `manifest_machine_with_lane` / `Machine::load_with_lane` / `compile_module_set` | 449.66ms, 44.6% | module load/compile cost inside the measured test |
| `canon_fn_hash` / `phon::api::encode` | 210.41ms | module identity hashing |
| flat `top --sort self` leaves | `_platform_memmove`, `Vec::clone`, iterator stepping, `BTreeMap` lookup, `RawTable::find`, `blake3`, `SipHash` leaves | supports the queued SchemaId/keyed-map/hash and module-load amortization levers more than a Rodin-specific propagation rewrite |

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
- `real_workspace_member_only_solve_ring_lock_diff_32`: 1 passed, 215 skipped; 10.636s. Ring 32 reports 33 solve rows, 893 lock rows, 32 exact matches, 1 solve-only pseudo-root, 861 lock-only rows categorized as 831 Cargo-selected-not-in-solve and 30 lock-residue-not-selected-by-metadata.
- `real_workspace_member_only_solve_ring_lock_diff_64`: 1 passed, 215 skipped; 27.594s. Ring 64 reports 65 solve rows, 893 lock rows, 64 exact matches, 1 solve-only pseudo-root, 829 lock-only rows categorized as 799 Cargo-selected-not-in-solve and 30 lock-residue-not-selected-by-metadata.
- `real_workspace_member_only_solve_ring_lock_diff_145`: timed out at 180.020s under the bounded nextest run, before producing a lock diff table. Direct-dep expansion was not run because the full member-only solve did not complete cleanly.
- `real_workspace_member_only_solve_ring_lock_diff_32_interp_lane`: 1 passed; paired lane measurement 6.539s; latest scoped rerun 8.294s; wrote `real-ring-32-interp-*` lock-diff artifacts.
- `real_workspace_member_only_solve_ring_lock_diff_32_jit_lane`: 1 passed; 6.577s; wrote `real-ring-32-jit-*` lock-diff artifacts.
- `pinned_sparse_row_parses_in_cargo_manifest_module`: 1 passed; proves the `cargo_manifest.vix` sparse JSONL row parser over a real pinned `blake3` sparse row.
- `real_workspace_member_direct_sparse_solve_ring_lock_diff_8`: 1 passed; latest scoped rerun 57.496s; ring 8 direct sparse table reports 989 sparse rows, 25 packages, 22 clauses, 10 solve rows, 9 matches, 0 version skew, 1 pseudo-root; `tokio_emitted_req = 1`.
- `tiny_workspace_prerelease_member_solve_selects_member`: 1 passed; exact workspace-member root pin selects `facet 0.50.0-rc.5`.
- `solution_walk_derives_units_from_rodin_and_matches_cargo_oracle`: 1 passed, 217 skipped; 13.341s in the latest script run; diff table reports 4 machine units, 4 Cargo units, 4 unit matches, 3 machine edges, 3 Cargo edges, 3 edge matches, and zero machine-only/Cargo-only units or edges.
- `real_workspace_member_only_index_builds_all_members`: 1 passed, 208 skipped; 79.198s for 146 package domains and 290 root clauses.
- Escalated `/usr/bin/time -l cargo nextest run -p vix -E 'test(=real_workspace_dependency_probe_shard_0)'`: 1 passed, 191 skipped; 20.207s test time; 215,793,664-byte max RSS.
- `TIER_A_FETCH_SPARSE=0 scripts/tier-a-scale-measurement.sh`: completed end to end through the ring-64 member solve frontier; sparse refetch was disabled because `/tmp/tier-a-scale-measurement/sparse-index` was already fetched and pinned.

Gate status from the gap-closer pass:

| Gate | Result |
|---|---|
| `git fetch origin rodin && git rebase origin/rodin` | completed; picked up `c868cd51a` molten sentinel fix |
| `cargo check --workspace --all-targets` | passed; latest rerun 2m47s after `cargo clean` |
| `cargo nextest run -p vix --features real-process` | passed; latest rerun 206 passed, 23 skipped, 96.055s |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed; latest rerun 1m12s |
| `git diff --check` | passed latest rerun |

Additional current-pass probes:

| Probe | Result |
|---|---|
| `cargo nextest run -p vix --run-ignored only -E 'test(=pinned_sparse_row_parses_in_cargo_manifest_module) | test(=real_workspace_member_direct_sparse_solve_ring_lock_diff_8) | test(=real_workspace_member_only_solve_ring_lock_diff_32_interp_lane)'` | passed; 3 passed, 218 skipped, 53.707s |

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

Once the branch already contains the machine fix, the default remeasurement command runs through the largest currently passing member-only solve ring, 64:

```sh
TIER_A_FETCH_SPARSE=0 scripts/tier-a-scale-measurement.sh
```

The full 145-member solve is opt-in because it currently times out:

```sh
TIER_A_FETCH_SPARSE=0 TIER_A_FULL_MEMBER_RING=1 scripts/tier-a-scale-measurement.sh
```

## Precise Frontier

Largest reachable subgraph today:

- Cargo oracle scale: 863 package-version rows, 881 units.
- Vix real-manifest ingestion/projection scale: 145 workspace members and 1,124 direct deps, sharded.
- Vix composed workspace-member solve ring: 64 / 145 members solved and diffed; 145 / 145 members still measured as an explicit index-construction probe; stable and prerelease 1 / 1 tiny member solves pass.
- Vix composed member+direct sparse solve ring: 8 workspace members plus their direct crates.io sparse rows solved and diffed; first `tokio` divergence is closed.
- Vix solve-to-unit scale: 4 package fixture, Cargo unit-graph matched.

The next ring is no longer blocked by the prerelease/root-clause empty solve, and the first direct sparse `tokio` skew is closed. The current frontier is runtime growth before full member-only ring completion and before widening direct sparse rings:

1. Make the 145-member member-only solve complete under the watchdog; the latest run timed out at 180.020s after rings 32 and 64 passed.
2. Widen the direct sparse ring beyond 8 once the perf lane lands the off-CPU/module-load fixes.
3. Compose the pinned sparse snapshot rows into the workspace `Index`, replacing the harness-fed JSONL surface with vix-side sparse path lookup.
4. Emit `UnitTargetTable` from real manifests using join-only `Path` provenance, then rerun `crate_solution_*` against the full Cargo unit graph.

Until those exist, the numeric answer remains:

```text
resolve match at real scale: 64 / 863 selected Cargo packages measured
unit-graph match at real scale: 0 / 881 Cargo units measured
composed workspace-member solve ring: 64 / 145 members solved and diffed; 145-member solve timed out at 180.020s
composed member+direct sparse ring: 8 members + 16 direct sparse files solved; 9 exact matches, 0 version skew, 1 pseudo-root
composed workspace-member full index: 145 / 145 members measured explicitly
composed tiny workspace solve: 1 / 1 stable member selected; 1 / 1 prerelease member selected
largest solve-to-unit match: 4 / 4 fixture units
manifest ingestion match: 1,124 / 1,124 direct workspace deps
```
