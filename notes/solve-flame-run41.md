# Solve Flame Run 41

Durable receipt for the stax flame used by `docs/design/hash-as-field.md`.

## Workload

- Branch measured: `tier-a-scale-measurement`
- Measured commit: `953544056`
- Profile: Cargo `profiling` profile, optimized with line tables
- Test binary: `target/profiling/deps/cargo_manifest-689b8bbc891035f7`
- Workload: `real_workspace_member_direct_sparse_solve_ring_lock_diff_32`
- Command:

```sh
TIER_A_OUT=/tmp/tier-a-scale-measurement \
TIER_A_NATIVE_REFERENCE_REPEATS=20 \
stax record -- \
target/profiling/deps/cargo_manifest-689b8bbc891035f7 \
real_workspace_member_direct_sparse_solve_ring_lock_diff_32 \
--ignored --exact --nocapture
```

Run 41 was queried with `stax flame` at 20k samples. The run was a live query
against the ring-32 solve wall; the retained receipt is this note plus the
timing/index artifacts listed below.

## Ring-32 Workload Shape

Artifacts from `/tmp/tier-a-scale-measurement`:

| Artifact | Value |
|---|---:|
| sparse rows | 4,424 |
| packages | 114 |
| clauses | 449 |
| `sparse_snapshot` | 167.713 ms |
| `typed_sparse_row_count` | 12,731.197 ms |
| `typed_sparse_index_and_debug` | 25,813.934 ms |
| solve/diff bucket | did not emit before the solve wall |

## Flame Answer

| Trunk | Active share | Current source anchor |
|---|---:|---|
| `Machine::demand_i64` | 95.8% | demand entry, `vix/src/machine/lower.rs` |
| `Task::run_hosted` | 49.6% | hosted demand execution under driver task scheduling |
| `ValueStore::alloc_raw_tainted -> raw_value_content_hash -> blake3` | 23.4% | `vix/src/machine/driver.rs:1465`, `vix/src/machine/driver.rs:9943` |
| `Driver::intern_molten_word` | 29.1% | `vix/src/machine/driver.rs:2096` |
| `intern_value_bytes_children -> intern_molten_word -> ValueStore::alloc_map` | 21.3% | `vix/src/machine/driver.rs:2107`, `vix/src/machine/driver.rs:1482` |
| `canonical_map_pairs` | 12.8% | `vix/src/machine/driver.rs:10468` |
| `hash_map_pairs` | 5.5% | `vix/src/machine/driver.rs:10723` |
| `projection_memo_hit -> projection_observation_hash` | 13.6% | `vix/src/machine/driver.rs:10072` |

The trunk is identity bookkeeping during solve execution, not sparse-row
ingestion. Map allocation, canonical map rows, raw value hashing, and projection
observation hashing are the visible subtrees.

## Lever Implication

Lever 4 targets the `alloc_map` / `canonical_map_pairs` subtree by carrying
canonical map rows through molten map mutation and interning while preserving
old-epoch hash bytes. The broader hash-as-field proposal targets the same
evidence at the layout level: hashes become carried/stored identity fields and
machine-lowered BLAKE3 replaces repeated host recomputation.
