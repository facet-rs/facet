# Tier-A scale measurement

Base: `tier-a-scale-measurement`, originally from `d27e2478f` (`Model target cfg facts in vix`), rebased through the molten sentinel fix and measured after `origin/rodin` advanced past the hasher/perf folds.

Question: how close is "vixen resolves and plans the entire monorepo, verified against cargo" as a number?

Answer after the prerelease/root-clause fix: **the composed path now produces a Cargo.lock-diffable real-workspace solve ring.** Full real-workspace resolve/plan scale is still partial, but the previous semantic empty-solve frontier is crossed for workspace-member roots.

- Before this pass, resolve at real scale was **0 / 864 Cargo-resolved package-version rows measured** because there was no vix entrypoint composing workspace manifests into a Rodin `Index`.
- After the widening pass, the largest measured solve ring is **64 / 146 workspace members -> 65 selected rows**, diffed against the real `Cargo.lock`: **64 / 65 solve rows match Cargo.lock**, with the remaining solve-only row being the pseudo workspace root.
- The explicit ignored scale probe still builds the full member-only index: **146 / 146 workspace members -> 147 package domains**; root clauses are now the old 2-per-member baseline plus root default-feature clauses.
- Tiny composed solves now pass for both stable and prerelease workspace members: **1 / 1 workspace member selected** through manifest tree -> Rodin `Index`/`Problem` -> `solve`, including `facet 0.50.0-rc.5`.
- Real-workspace full solve remains **64 / 864 Cargo-resolved package-version rows diffed against Cargo.lock** at the largest measured member-only ring. The machine-lane `molten handle -1` failure is gone, and the root prerelease empty-solve is fixed; the full-member solve remains a wall-clock frontier.
- The widened direct-deps sparse ring now runs through **16 workspace members + direct crates.io rows from 48 sparse package files -> 65 package domains, 104 clauses, 27 selected rows**, diffed against `Cargo.lock` with **26 exact matches, 0 version-skew packages, and 1 expected pseudo-root solve-only row**. Ring 32 was attempted in optimized profile and timed out at 180.212s before index-count artifacts.
- The first transitive sparse composition is wired in vix and reaches **ring 2**: 2 workspace members + first transitive sparse rows -> 17 package domains, 24 clauses, 3 selected rows, 2 exact matches, 0 version skew, and 1 expected pseudo-root. Ring 3 expands to 80 sparse input crates and times out at 180.279s before index-count artifacts.
- Manifest ingestion at real scale remains **closed for the direct-dependency oracle**. Current tests assert 146 workspace members, 1,133 direct deps, 55 cfg/target deps, 765 legacy allowlist failures retired, and zero name/kind/target mismatches across 16 shards.
- Unit derivation at ring scale now has a numerator: **direct sparse ring 8 derives 10 Vix unit target rows for 9 selected real packages**, diffed against **10 Cargo unit rows** from a ring-rooted `cargo +nightly build --unit-graph -p ...` oracle. Exact feature+profile unit-key matches are **9 / 10 Cargo rows**; target-kind/source/profile-shape matches are **10 / 10 Vix target rows**; edge-set matches are **4 / 40 Cargo edges**, with the remaining 36 Cargo-only edges categorized as dependencies outside the Vix-selected direct closure.
- Largest artifact-producing solve-to-unit path remains **4 packages / 4 units** in the `lock_graph` fixture, verified against Cargo `--unit-graph`; real ring-scale unit rows are now emitted and diffed, but real source-tree artifact builds remain fixture-scoped.

## Gap-Closer Delta

New vix/Rust surfaces:

- Added stage-one typed deserialization for parser-family host calls: `json_typed(...)`
  is context-typed by the vix lowerer, travels through the existing document-parse
  request queue, parses with the existing facet-json -> internal `Value` path, and
  materializes directly into the requested vix schema using the established store
  allocators (`alloc_raw`, `alloc_array_words`, `alloc_map`, declared struct
  descriptors). Schema mismatch is loud and includes the offending input row.
- Switched sparse-row ingestion from `json(line) -> Doc` projection walking to
  `let row: SparseIndexRow = json_typed(line)`. `SparseIndexRow.features` is now
  `Map<String, [String]>`, so feature names and expansions operate on typed maps
  instead of `Doc` maps. This is the sanctioned parser-family atomic boundary; no
  Rust-side sparse-row composition bypass was added.
- Extended `keys()` to typed `Map<String, V>` receivers so typed sparse feature
  maps can enumerate feature names without falling back through `Doc`.
- Added pure host/lowerer support for `Doc.keys() -> [String]`, sorted for deterministic dependency-table enumeration.
- Fixed `Path.join` for an empty granted root so `Path("") .join("member")` yields relative `member` instead of absolute `/member`.
- Added `cargo_manifest.vix` workspace bridge state that derives Rodin `Index` fields from real manifests: pseudo workspace root, member package/version rows, root `selected -> in_graph` clauses, and a direct-dependency clause bridge for required workspace-known deps.
- Added member-only and member+direct ring entrypoints so the harness can measure the frontier without Rust-side composition bypasses.
- Added sparse JSONL row parsing in `cargo_manifest.vix` and direct sparse ring probes that compose selected real workspace members plus direct crates.io rows from the pinned sparse snapshot into one Rodin `Index`.
- Added transitive sparse ring entrypoints that add dependency clauses from crates.io sparse rows in vix, with optional/dev deps skipped and target cfg carried as clause gate data.
- Added sparse-index fetch/pin script: `scripts/fetch-tier-a-sparse-index.sh`.
- Added Cargo.lock and unit-graph diff harness prep. `scripts/tier-a-scale-measurement.sh` now exports `TIER_A_OUT`, `TIER_A_CARGO_METADATA`, `TIER_A_UNIT_GRAPH`, and `TIER_A_SPARSE_OUT`; runs the tiny live solve package diff; runs the member-ring live solve package diffs through ring 64 by default; runs the direct sparse ring-8 solve diff/timings; runs the 4/4 derived-unit fixture diff; runs the real direct ring-8 unit diff; and includes the TSV summaries in `summary.txt`. The timed-out full-member solve is opt-in via `TIER_A_FULL_MEMBER_RING=1`.
- After `c868cd51a`, root member clauses now emit the same two-step shape as direct deps: `in_graph` activation plus an exact `version_set` clause for the workspace member's manifest version. Root default-feature clauses are now added for members with `[features].default`.
- Fixed Rodin's prerelease root semantics in `rodin.vix`: a default domain now acts as an unconstrained sentinel on the first real `version_set` narrow, and `in_graph` activation marks reachability without re-filtering through Cargo's plain `*` range. The semver differential tests still pin Cargo's rule that plain ranges exclude prereleases, while the unignored `tiny_workspace_prerelease_member_solve_selects_member` now proves exact workspace-root pins admit prereleases.
- Added `workspace_member_only_solve_selected_versions_text_limit` and ignored ring lock-diff measurement probes for 16, 32, 64, and 146 so ring solves produce the same `(package, version)` Cargo.lock diff table as the tiny case.
- Added `workspace_member_direct_sparse_solution_units_text_limit`, which composes the direct sparse ring solve into Vix-emitted unit rows carrying package/version, target kind, source suffix, mode, features, profile fields, and selected dependency edges. The Rust harness now roots Cargo `--unit-graph` with the ring workspace package list (`-p ...`) and then filters to the same selected `(package, version)` set. Full-workspace `--unit-graph` filtered after the fact is contaminated by Cargo's global feature unification and must not be used as the ring oracle.
- Added Vix-side unit feature emission from solved feature IDs. Workspace member defaults and manifest feature expansion are emitted in unit rows; sparse row `features`/`features2`, dependency `features`, and `default_features` are carried through the JSONL bridge, with selected sparse-row feature definitions expanded during registry unit derivation.

Typed sparse-row remeasurement:

| Ring | Sparse rows | Package domains | Clauses | Selected rows | Lock matches | Version skew | Wall / frontier |
|---:|---:|---:|---:|---:|---:|---:|---|
| direct 8 + typed sparse rows | 1,089 | 27 | 31 | 10 | 9 | 0 | completed in default script |
| direct 16 + typed sparse rows | 2,638 | 67 | 160 | 27 | 26 | 0 | completed; scoped nextest wall 65.444s |
| direct 32 + typed sparse rows | 4,424 | 114 | 449 | n/a | n/a | n/a | timed out at 180.193s during solve after index/debug completed |

Ring 8 typed timing buckets (`/tmp/tier-a-scale-measurement/real-direct-ring-8-timings.tsv`, from `TIER_A_FETCH_SPARSE=0 scripts/tier-a-scale-measurement.sh`):

| Step | Wall ms |
|---|---:|
| sparse_snapshot | 503.996 |
| typed_sparse_row_count | 2,387.635 |
| typed_sparse_index_and_debug | 2,804.386 |
| solve_and_lock_diff | 2,953.331 |

Ring 16 typed timing (`/tmp/tier-a-scale-measurement/real-direct-ring-16-timings.tsv`):

| Step | Wall ms |
|---|---:|
| sparse_snapshot | 880.661 |
| typed_sparse_row_count | 8,755.556 |
| typed_sparse_index_and_debug | 12,832.043 |
| solve_and_lock_diff | 40,867.649 |

Ring 32 typed partial timing (`/tmp/tier-a-scale-measurement/real-direct-ring-32-timings.tsv`):

| Step | Wall ms |
|---|---:|
| sparse_snapshot | 1,372.017 |
| typed_sparse_row_count | 12,111.810 |
| typed_sparse_index_and_debug | 28,185.889 |

The ring-32 timeout occurs after typed row count and index/debug composition
complete, and before a solve/diff artifact emits. The current typed-deserialization
frontier is therefore solve-scale work for the 32-member direct sparse ring, not
an unexplained version selection divergence.

Stage two composition note: this stage keeps the parser as one host-family
atomic operation and writes ordinary store values through schema descriptors.
The next design step is to lower per-schema deserializers to weavy so the same
schema-directed writes can be generated/executed without this Rust parser host
remaining on the hot path. The current checkout's
`docs/design/capabilities-ambient-vs-materialized.md` copy found under
`/Users/amos/vixenware/vixen` contains the first three dictation sections; the
fourth dictation referenced for stage two was not present in that file during
this pass.

Prepared diff surfaces:

| Surface | Exercised Case | Match | Divergence | Artifact |
|---|---:|---:|---:|---|
| Solve `(package, version)` vs real `Cargo.lock` | tiny live solve: `__workspace__ 0.0.0`, `bytes 1.12.0` | 1 / 2 solve rows | 1 solve-only pseudo-root; 862 Cargo-selected lock rows not in tiny solve; 30 lock residue rows | `/tmp/tier-a-scale-measurement/tiny-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member-only ring 16 | 16 / 17 solve rows | 1 solve-only pseudo-root; 847 Cargo-selected lock rows not in ring 16; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-ring-16-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member-only ring 32 | 32 / 33 solve rows | 1 solve-only pseudo-root; 831 Cargo-selected lock rows not in ring 32; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-ring-32-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member-only ring 64 | 64 / 65 solve rows | 1 solve-only pseudo-root; 799 Cargo-selected lock rows not in ring 64; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-ring-64-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member+direct sparse ring 8 | 9 / 10 solve rows | 1 solve-only pseudo-root; 0 version skew; 854 Cargo-selected lock rows not in ring 8; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-direct-ring-8-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member+direct sparse ring 16 | 26 / 27 solve rows | 1 solve-only pseudo-root; 0 version skew; 837 Cargo-selected lock rows not in ring 16; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-direct-ring-16-solve-vs-lock-summary.tsv` |
| Solve `(package, version)` vs real `Cargo.lock` | real member+direct+transitive sparse ring 2 | 2 / 3 solve rows | 1 solve-only pseudo-root; 0 version skew; 861 Cargo-selected lock rows not in ring 2; 30 lock residue rows | `/tmp/tier-a-scale-measurement/real-transitive-ring-2-solve-vs-lock-summary.tsv` |
| Derived units vs Cargo `--unit-graph` | `lock_graph` fixture | 4 / 4 units, 3 / 3 edges | 0 machine-only, 0 Cargo-only | `/tmp/tier-a-scale-measurement/lock-fixture-unit-diff-summary.tsv` |
| Derived units vs Cargo `--unit-graph` | real member+direct sparse ring 8 | 9 / 10 exact unit keys; 10 / 10 Vix target keys; 4 / 40 edges | tokio feature-set gap: 1 Vix-only + 1 Cargo-only; Cargo-only dependency edges outside Vix-selected direct closure: 36 | `/tmp/tier-a-scale-measurement/real-direct-ring-8-unit-diff-summary.tsv` |

After-run ring table:

| Ring | Cargo / input scale | Vix/Rodin result | Status |
|---|---:|---:|---|
| Tiny composed solve | 1 workspace member | 1 / 1 selected | passes |
| Tiny prerelease composed solve | 1 workspace member, version `0.50.0-rc.5` | 1 / 1 selected | passes; exact root pin admits prerelease |
| Real workspace member-only index, bounded | 16 workspace members | 17 package domains, 35 member/root clauses | passes |
| Real workspace member-only index, full | 146 workspace members | 147 package domains plus default-feature root clauses | explicit ignored probe remains available; old 290-clause exact count retired |
| Real workspace member-only solve rings | 1, 2, 4, 8, 16, 32, 64 workspace members | selected member counts equal ring size; ring 64 writes 65 solve rows | passes through 64; full-member solve remains the wall-clock frontier |
| Real workspace member+direct deps, small sparse ring | 8 workspace members plus direct crates.io rows | 27 package domains, 31 clauses, 1,089 sparse rows, 10 selected rows | passes; typed sparse-row timing captured |
| Real workspace member+direct deps, unit derivation | ring 8 selected set: 9 real packages, 10 Vix unit target rows | 10 Cargo unit rows from ring-rooted `--unit-graph`; 9 exact unit-key matches, 10 target-key matches, 4 edge matches | passes; tokio feature-set gap and direct-closure edge gaps categorized |
| Real workspace member+direct deps, widened sparse ring | 16 workspace members plus direct crates.io rows | 67 package domains, 160 clauses, 2,638 sparse rows, 27 selected rows | passes; 26 matches, 0 version skew, 1 pseudo-root |
| Real workspace member+direct deps, next widened sparse ring | 32 workspace members plus direct crates.io rows | 114 package domains, 449 clauses, 4,424 sparse rows | timed out at 180.193s during solve after index/debug artifacts |
| Real workspace member+direct+transitive deps | 1 and 2 workspace members plus direct crates plus direct crates' deps | ring 1: 16 package domains, 22 clauses, 2 selected rows; ring 2: 17 package domains, 24 clauses, 3 selected rows | passes through ring 2; only workspace members selected so far, 0 version skew |
| Real workspace member+direct+transitive deps, next ring | 3 workspace members; first transitive expansion jumps to 80 sparse input crate files | no index-count artifact | timed out at 180.279s in optimized profile |
| Real workspace member+direct deps, full workspace | 146 members, 1,133 direct deps | required workspace-known direct clauses implemented; full direct sparse solve not attempted after the full member-only wall-clock timeout | pending perf frontier |
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
| packages | 864 |
| workspace members | 146 |
| resolve nodes | 864 |
| resolve deps | 2,801 |
| cfg-gated resolve dep-kinds | 299 |
| registry package-version rows | 718 |
| path package-version rows | 146 |

Cargo unit graph stats:

| Measure | Count |
|---|---:|
| units | 880 |
| roots | 157 |
| dependency edges | 2,489 |
| build-mode units | 804 |
| run-custom-build units | 76 |
| custom-build target-kind units | 152 |
| proc-macro target-kind units | 48 |
| lib target-kind units | 653 |
| bin target-kind units | 20 |

Cargo.lock projection:

| Set | Count |
|---|---:|
| Cargo.lock package-version rows | 894 |
| Cargo metadata selected package-version rows | 864 |
| Matched package-version rows | 864 |
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
| Selected package-version rows vs Cargo.lock | 864 selected / 894 locked | 65 ring-64 solve rows | 64 | 800 selected rows not in ring 64; 30 lock residue rows |
| Registry selected rows | 718 selected / 748 locked | 0 registry rows in member-only ring | 0 | 718 unmeasured selected registry rows |
| Lock residue relative to Cargo metadata | 30 | n/a | n/a | 30 lock-only rows |
| Workspace member-only solve ring | 146 members | 64 bounded members solved and diffed; full 146-member index measured explicitly; tiny stable/prerelease solves pass | 64 | 82 workspace members not yet solved in a completed ring |
| Workspace member+direct sparse solve ring | 16 members + direct sparse candidates | 27 solve rows | 26 | 1 pseudo-root row; remaining Cargo rows outside the small ring |
| Workspace member+direct+transitive sparse solve ring | 2 members + first transitive sparse candidates | 3 solve rows | 2 | 1 pseudo-root row; no registry dependency selected at the reachable transitive ring |

Full-workspace divergence categories cannot be assigned yet, because the largest rodin-selected real-workspace output is the member-only ring 64, not the full 864-row Cargo closure. At the measured ring, there is no version skew: all 64 real member rows match `Cargo.lock`; the only solve-only row is the pseudo workspace root.

Direct sparse divergence tables:

| Ring | Exact lock matches | Solve-only pseudo-root | Version skew | Cargo-selected rows outside ring | Cargo.lock residue | Current read |
|---:|---:|---:|---:|---:|---:|---|
| direct 8 | 9 | 1 | 0 | 854 | 30 | all workspace members in the 8-member ring plus the first direct crates.io package match lock versions |
| direct 16 | 26 | 1 | 0 | 838 | 30 | widened direct sparse boundary remains zero-skew; `tokio` has three emitted `1` req lines, all narrowed |

Transitive sparse divergence table:

| Ring | Sparse input crates | Sparse rows | Package domains | Clauses | Solve rows | Exact lock matches | Version skew | Current read |
|---:|---:|---:|---:|---:|---:|---:|---:|---|
| transitive 1 | 14 | 736 | 16 | 22 | 2 | 1 | 0 | reachable but selects only the first workspace member plus pseudo-root |
| transitive 2 | 14 | 736 | 17 | 24 | 3 | 2 | 0 | reachable but still selects only workspace members plus pseudo-root |
| transitive 3 | 80 | n/a | n/a | n/a | n/a | n/a | n/a | times out at 180.279s before index-count artifacts |
| transitive 4 | 80 | n/a | n/a | n/a | n/a | n/a | n/a | times out at 180.225s before index-count artifacts |
| transitive 8 | 125 | n/a | n/a | n/a | n/a | n/a | n/a | times out at 180.218s before index-count artifacts |

No unexplained version pick has appeared in the widened direct or reachable transitive rings. The first transitive ring that would select registry packages is not yet reached; the immediate blocker is solve/index runtime after the input expands from 14 to 80 sparse package files.

Tokio skew diagnosis:

| Half Checked | Result | Evidence |
|---|---|---|
| requirement narrowing | intact | `real-direct-ring-8-tokio-narrowing.tsv` records `tokio_emitted_req = 1`; the direct bridge did not emit `*` |
| candidate preference / sparse row order | fixed | sparse JSONL rows are now registered into the `Index` in file order; Rodin preserves that ascending candidate order so Vix back-pop tries the newest admissible row first. `real-direct-ring-8-tokio-candidates.txt` starts with `1.52.3`, and the selected row is `tokio 1.52.3` |

Current source frontier:

- `cargo_manifest.vix` can derive member counts, dependency declarations, cfg data, target shapes one at a time, `problem_of_member`, and a workspace-member Rodin `Index` ring.
- It now exposes `resolved_unit_adaptation_gap()` as: "Ring-scale solution unit rows are emitted from the composed workspace+sparse solve; artifact-producing crate.vix builds remain fixture-scoped because real source trees and feature-name UnitTargetTable emission are not complete."
- `rodin/index.vix` can parse sparse rows and bridge them to an `Index`, but `sparse_index_path` is still a demo hardcoded path table for a small crate set, and the bridge skips optional/dev deps.
- `crate.vix` has `crate_solution_bin[_check]`, but it requires a pre-built `Index`, `Problem`, and `UnitTargetTable`.

Categorization for the current resolve frontier:

| Category | Count / Blast Radius | Evidence |
|---|---:|---|
| Workspace manifest-to-Index composition, bounded | 64 / 146 workspace members solved by default; stable/prerelease tiny solves pass | new `workspace_member_only_*` entrypoints |
| Workspace manifest-to-Index composition, full index | 146 / 146 workspace members index-built explicitly | `real_workspace_member_only_index_builds_all_members`: 147 package domains plus default-feature root clauses |
| Workspace manifest-to-Index composition, full solve | 64 / 864 Cargo-resolved package-version rows diffed | member-only solve rings pass through 64; ring 64 diff table has 64 matches, 0 version skew; full-member solve times out |
| Workspace member+direct sparse composition | 16-member direct sparse ring solved and diffed | 2,538 sparse rows, 65 packages, 104 clauses, 27 solve rows; 26 matches, 0 version skew |
| Workspace member+direct+transitive sparse composition | 2-member transitive sparse ring solved and diffed | 736 sparse rows, 17 packages, 24 clauses, 3 solve rows; 2 matches, 0 version skew; ring 3 times out after expanding to 80 sparse package files |
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
| Vix full member-only index ignored probe | previous 79.198s for old 145-member index probe | not recaptured after 146-member trunk update |
| Vix real member-only solve rings after prerelease fix | ring 1: 13.896s; 2: 13.734s; 4: 14.152s; 8: 14.499s; 16: 16.580s; alias 16: 16.472s; ring-16 lock diff: 16.598s | all passed in the script run; max RSS not captured |
| Vix widened member-only lock diffs | current script: ring 16, 32, 64 pass; full-member solve remains opt-in | 32 and 64 passed with 0 version skew |
| Vix direct sparse lock diff ring 8 | current script typed timings: snapshot 503.996 ms, typed row count 2387.635 ms, index/debug 2804.386 ms, solve+diff 2953.331 ms | 1,089 sparse rows, 27 packages, 31 clauses; 9 matches, 0 version skew |
| Vix direct sparse lock diff ring 16 | 159.394s in optimized profile; debug-profile attempt reached post-index tokio narrowing at 194.068s before the old multiplicity assertion failed | 2,538 sparse rows, 65 package domains, 104 clauses, 27 solve rows; 26 matches, 0 version skew |
| Vix direct sparse lock diff ring 32 | timed out at 180.212s in optimized default profile | 82 sparse input crate files recorded; no index-count or diff artifact before timeout |
| Vix direct sparse ring 8 unit diff after ring-root oracle + feature-row emission | 89.368s latest focused debug run; intermediate runs ranged 88.186s-141.049s while feature emission was being adjusted | 10 Vix units, 10 Cargo units, 9 exact unit-key matches, 10 target-key matches, 4 / 40 edge matches |
| Vix transitive sparse lock diff ring 1 | 2.818s in optimized profile | 14 sparse input crate files, 736 sparse rows, 16 package domains, 22 clauses, 2 solve rows; 1 match, 0 version skew |
| Vix transitive sparse lock diff ring 2 | 3.015s in optimized profile | 14 sparse input crate files, 736 sparse rows, 17 package domains, 24 clauses, 3 solve rows; 2 matches, 0 version skew |
| Vix transitive sparse lock diff ring 3 | timed out at 180.279s in optimized default profile | 80 sparse input crate files recorded; no index-count or diff artifact before timeout |
| Vix transitive sparse lock diff ring 4 | timed out at 180.225s in optimized default profile | 80 sparse input crate files recorded; no index-count or diff artifact before timeout |
| Vix transitive sparse lock diff ring 8 | timed out at 180.218s in optimized default profile | 125 sparse input crate files recorded; no index-count or diff artifact before timeout |
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
| Full-workspace units | 880 | not widened past solve wall | 0 full-workspace rows | standing perf frontier before full unit derivation |
| Direct sparse ring-8 units | 10 Cargo unit rows from the ring-rooted package list | 10 Vix unit target rows | 9 exact feature+profile unit-key matches; 10 target-key matches | 1 tokio feature-set gap on each side |
| Direct sparse ring-8 dependency edges | 40 Cargo edges | 4 Vix selected-closure edges | 4 | 36 Cargo-only edges outside the Vix-selected direct closure |
| `(package, target-kind, features, profile)` shapes | 10 Cargo ring rows | 10 Vix ring rows | 9 exact, 10 target-shape | remaining tokio feature gap is at the direct/transitive closure boundary; profile payload included in the diff |
| Fixture solve-to-unit path | 4 packages / 4 units | 4 packages / 4 units | 4 | 0 on fixture |

Known Cargo unit categories that vix must eventually account for at scale:

| Category | Cargo Count | Status |
|---|---:|---|
| build-script companion units (`custom-build` build + run) | 152 custom-build target-kind units; 76 run-custom-build units | counted as a gap category; do not chase profile payload here |
| proc-macro host units | 48 | counted as a gap category |
| profile fields | all Cargo ring rows carry profile payloads | emitted and compared for ring 8; the previous Cargo-only `tokio` feature/profile variant was an oracle-contamination artifact and is gone under the ring-rooted oracle |
| feature-name emission | Cargo ring rows include feature sets for `facet-showcase`, `strid`, and `tokio` | Vix now emits workspace defaults, manifest feature expansion, dependency-requested feature names, and selected sparse-row feature expansion; the only remaining unit-key gap is tokio features activated by Cargo transitive deps outside the Vix direct closure plus one cfg artifact (`windows-sys`) |
| non-lib/bin crate types | 9 cdylib, 4 rlib, 1 staticlib crate-type entries full-workspace; ring 8 includes 2 cdylib rows | ring 8 cdylib target rows match Cargo target keys |

The ring-scale unit bridge is now real for measurement: `cargo_manifest.vix` composes the direct sparse ring solve into target rows and selected dependency edges, and the Rust harness diffs those rows against Cargo `--unit-graph` rooted with the ring package list. `crate.vix` artifact-producing recursive builds remain fixture-scoped: `solution_unit` still proves the build shape over `mini_app -> alpha_lib -> core_lib` plus `formatting_lib`, while real source-tree artifact builds are not widened yet.

Resolver-v2 feature partitioning remains parked as the design frontier. If/when host/build/proc-macro rings widen, feature atoms should be scoped by partition, with the existing fixture hint of `build:` / `dev:` scoped names preserved. The current ring-8 direct sparse table does not require that fix: the previous two-variant tokio profile/feature row came from the contaminated full-workspace-filtered oracle, not from a live v2 partition failure in the ring-rooted oracle.

## Scoped Verification

```sh
cargo nextest list -p vix -E 'test(=real_workspace_metadata_baseline_is_counted) | test(=real_workspace_dependency_probe_shard_0) | test(=direct_resolved_unit_adapter_gap_is_pinned)'
cargo nextest run -p vix -E 'test(=real_workspace_metadata_baseline_is_counted) | test(=real_workspace_dependency_probe_shard_0) | test(=direct_resolved_unit_adapter_gap_is_pinned)'
cargo nextest list -p vix --features real-process -E 'test(=solution_walk_derives_units_from_rodin_and_matches_cargo_oracle)'
cargo nextest run -p vix --features real-process -E 'test(=solution_walk_derives_units_from_rodin_and_matches_cargo_oracle)'
cargo nextest list --run-ignored only -p vix -E 'test(=real_workspace_member_direct_sparse_unit_diff_8)'
TIER_A_OUT=/tmp/tier-a-scale-measurement cargo nextest run --run-ignored only -p vix -E 'test(=real_workspace_member_direct_sparse_unit_diff_8)'
```

Results:

- `TIER_A_FETCH_SPARSE=0 scripts/tier-a-scale-measurement.sh`: completed after the typed sparse-row change; metadata reports 864 packages / 146 workspace members / 894 lock rows; unit graph reports 880 units.
- `tiny_workspace_solve_diff_is_categorized_against_real_cargo_lock`: latest script table reports 2 solve rows, 894 lock rows, 1 exact match, 1 solve-only pseudo-root, 893 lock-only rows categorized as 863 Cargo-selected-not-in-solve and 30 lock-residue-not-selected-by-metadata.
- `real_workspace_metadata_baseline_is_counted`, `real_workspace_dependency_probe_shard_0`, `direct_resolved_unit_adapter_gap_is_pinned`: 3 passed, 206 skipped; shard 0 took 13.084s in the post-`c868cd51a` script run.
- `projected_member_manifests_are_read_from_granted_root`, `dependency_declarations_extract_workspace_and_detailed_forms`, `real_workspace_member_only_index_builds_bounded_ring`: 3 passed, 206 skipped; bounded member ring took 3.994s.
- `real_workspace_member_only_solve_ring_{1,2,4,8,16}`, `real_workspace_member_index_solves_bounded_ring`, and member lock-diff rings 16/32/64: latest script run passed. Ring 16 reports 17 solve rows / 894 lock rows / 16 matches; ring 32 reports 33 / 894 / 32; ring 64 reports 65 / 894 / 64. All have 0 version skew and 1 pseudo-root solve-only row.
- Full-member lock diff: still opt-in and not part of the default script because it times out before producing a lock diff table.
- `real_workspace_member_only_solve_ring_lock_diff_32_interp_lane`: 1 passed; paired lane measurement 6.539s; latest scoped rerun 8.294s; wrote `real-ring-32-interp-*` lock-diff artifacts.
- `real_workspace_member_only_solve_ring_lock_diff_32_jit_lane`: 1 passed; 6.577s; wrote `real-ring-32-jit-*` lock-diff artifacts.
- `pinned_sparse_row_parses_in_cargo_manifest_module`: 1 passed; proves the `cargo_manifest.vix` sparse JSONL row parser over a real pinned `blake3` sparse row.
- `real_workspace_member_direct_sparse_solve_ring_lock_diff_8`: latest typed script run passed; ring 8 direct sparse table reports 1,089 sparse rows, 27 packages, 31 clauses, 10 solve rows, 9 matches, 0 version skew, 1 pseudo-root; `tokio_emitted_req = 1`; timing buckets are 503.996 ms snapshot, 2,387.635 ms typed row count, 2,804.386 ms typed index/debug, 2,953.331 ms solve+diff.
- `real_workspace_member_direct_sparse_unit_diff_8`: 1 passed; latest scoped rerun 17.803s after feature-row emission, the ring-rooted oracle, and folded perf levers. The unit diff table reports 10 Vix unit rows, 10 Cargo unit rows, 9 exact feature+profile unit-key matches, 10 target-key matches, 4 / 40 edge matches, 0 Vix-only edges, 36 Cargo-only edges categorized as outside the Vix-selected direct closure. The last tokio feature-set pair is not v2 host/target partitioning: Cargo's `-p peer-server` oracle still builds transitive path packages outside the Vix direct-ring selected closure (`vox-websocket`, `subject-rust`, etc.), and their tokio requirements unify `io-std`, `sync`, and `time` onto the tokio unit. The harness now categorizes that pair as Vix feature-subset vs Cargo transitive path closure / Cargo feature-superset from transitive path closure rather than leaving it as an unexplained feature-set gap.
- `real_workspace_member_direct_sparse_solve_ring_lock_diff_16`: latest typed scoped run passed in 65.444s; ring 16 direct sparse table reports 2,638 sparse rows, 67 packages, 160 clauses, 27 solve rows, 26 matches, 0 version skew, 1 pseudo-root. `tokio_emitted_req` has three `1` lines, proving narrowing is intact for all emitted tokio clauses.
- `real_workspace_member_direct_sparse_solve_ring_lock_diff_32`: latest typed scoped run timed out at 180.193s; it wrote index-count artifacts for 4,424 sparse rows, 114 packages, 449 clauses and timing buckets through `typed_sparse_index_and_debug` (28,185.889 ms), then timed out during solve.
- `real_workspace_member_transitive_sparse_solve_ring_lock_diff_1`: 1 passed under `--release`; 2.818s; 14 sparse input crates, 736 sparse rows, 16 packages, 22 clauses, 2 solve rows, 1 match, 0 version skew.
- `real_workspace_member_transitive_sparse_solve_ring_lock_diff_2`: 1 passed under `--release`; 3.015s; 14 sparse input crates, 736 sparse rows, 17 packages, 24 clauses, 3 solve rows, 2 matches, 0 version skew.
- `real_workspace_member_transitive_sparse_solve_ring_lock_diff_3`: timed out under `--release` default profile at 180.279s; wrote only the 80-entry sparse input crate artifact.
- `real_workspace_member_transitive_sparse_solve_ring_lock_diff_4`: timed out under `--release` default profile at 180.225s; wrote only the 80-entry sparse input crate artifact.
- `real_workspace_member_transitive_sparse_solve_ring_lock_diff_8`: timed out under `--release` default profile at 180.218s; wrote only the 125-entry sparse input crate artifact.
- `tiny_workspace_prerelease_member_solve_selects_member`: 1 passed; exact workspace-member root pin selects `facet 0.50.0-rc.5`.
- `solution_walk_derives_units_from_rodin_and_matches_cargo_oracle`: 1 passed, 217 skipped; 13.341s in the latest script run; diff table reports 4 machine units, 4 Cargo units, 4 unit matches, 3 machine edges, 3 Cargo edges, 3 edge matches, and zero machine-only/Cargo-only units or edges.
- `real_workspace_member_only_index_builds_all_members`: previous scoped run passed in 79.198s for 146 package domains and the old 290 root-clause baseline; current feature-emitting bridge adds default-feature root clauses and retires the exact 290 assertion.
- Escalated `/usr/bin/time -l cargo nextest run -p vix -E 'test(=real_workspace_dependency_probe_shard_0)'`: 1 passed, 191 skipped; 20.207s test time; 215,793,664-byte max RSS.
- `TIER_A_FETCH_SPARSE=0 scripts/tier-a-scale-measurement.sh`: completed end to end through the ring-64 member solve frontier; sparse refetch was disabled because `/tmp/tier-a-scale-measurement/sparse-index` was already fetched and pinned.

Gate status from the gap-closer pass:

| Gate | Result |
|---|---|
| `git fetch origin rodin && git rebase origin/rodin` | completed; picked up `c868cd51a` molten sentinel fix |
| `cargo check --workspace --all-targets` | passed; latest rerun 2.38s |
| `cargo nextest run -p vix --features real-process` | passed; latest rerun 206 passed, 29 skipped, 45.021s |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed; latest rerun 2.86s |
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

The full-member solve is opt-in because it currently times out:

```sh
TIER_A_FETCH_SPARSE=0 TIER_A_FULL_MEMBER_RING=1 scripts/tier-a-scale-measurement.sh
```

## Precise Frontier

Largest reachable subgraph today:

- Cargo oracle scale: 864 package-version rows, 880 units.
- Vix real-manifest ingestion/projection scale: 146 workspace members and 1,133 direct deps, sharded.
- Vix composed workspace-member solve ring: 64 / 146 members solved and diffed; 146 / 146 members still measured as an explicit index-construction probe; stable and prerelease 1 / 1 tiny member solves pass.
- Vix composed member+direct sparse solve ring: 16 workspace members plus their direct crates.io sparse rows solved and diffed; first `tokio` divergence is closed and the widened ring remains zero-skew.
- Vix composed member+direct+transitive sparse solve ring: 2 workspace members plus direct crates and direct crates' dependencies solved and diffed; no registry package is selected yet at the reachable transitive ring. Ring 3 is the current transitive wall-clock frontier, expanding from 14 to 80 sparse input crates and timing out before index counts.
- Vix solve-to-unit scale: direct sparse ring 8 emits and diffs 10 Vix unit target rows against 10 ring-rooted Cargo unit rows; artifact-producing build scale remains the 4 package fixture, Cargo unit-graph matched.

The next ring is no longer blocked by the prerelease/root-clause empty solve, and the first direct sparse `tokio` skew is closed. The current frontier is runtime growth before full member-only ring completion and before widening direct sparse rings:

1. Make the full-member member-only solve complete under the watchdog; the latest completed default script still stops at ring 64.
2. Widen the direct sparse ring beyond 16 once the perf lane lands the off-CPU/module-load fixes; ring 32 times out at 180.212s before index counts.
3. Widen the first transitive sparse ring beyond 2; ring 3 expands to 80 sparse input crates and times out at 180.279s before index counts.
4. Compose the pinned sparse snapshot rows into the workspace `Index`, replacing the harness-fed JSONL surface with vix-side sparse path lookup.
5. Emit `UnitTargetTable` from real manifests using join-only `Path` provenance, then rerun `crate_solution_*` against the full Cargo unit graph.

Until those exist, the numeric answer remains:

```text
resolve match at real scale: 64 / 864 selected Cargo packages measured
unit-graph match at full real scale: not widened past solve wall; ring-scale direct sparse unit match: 9 / 10 exact unit keys, 10 / 10 Vix target keys, 4 / 40 edges
composed workspace-member solve ring: 64 / 146 members solved and diffed; full-member solve times out
composed member+direct sparse ring: 16 members + direct sparse files solved; 26 exact matches, 0 version skew, 1 pseudo-root; ring 32 timed out at 180.212s
composed member+direct+transitive sparse ring: 2 members solved; 2 exact matches, 0 version skew, 1 pseudo-root; ring 3 timed out at 180.279s
composed workspace-member full index: 146 / 146 members measured explicitly
composed tiny workspace solve: 1 / 1 stable member selected; 1 / 1 prerelease member selected
largest artifact-producing solve-to-unit match: 4 / 4 fixture units
manifest ingestion match: 1,133 / 1,133 direct workspace deps
```
