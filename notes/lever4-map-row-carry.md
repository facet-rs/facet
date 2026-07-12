# Lever 4: Molten Map Row Carry

## Change

Carry canonical map rows through molten map allocation, in-place `map_insert`,
and `intern_molten_word`, so `ValueStore::alloc_map` can skip rebuilding the
same ordered key/value hash rows when the molten path has already maintained
them.

Old-epoch identity remains byte-neutral: final map content hashes are still
produced by `hash_map_pairs`, and the carried path is tested against the
recomputed path.

## Proof

Focused equivalence:

```sh
cargo nextest run -p vix --features real-process \
  -E 'test(carried_map_rows_allocate_with_recomputed_hash)'
```

Result: 1 passed.

Force-copy and molten mutation tripwires:

```sh
cargo nextest run -p vix --features real-process \
  -E 'test(molten_reuse_is_unobservable_for_aggregate_updates) | test(map_get_cache_observes_insert_after_get_after_insert) | test(molten_array_carried_hash_matches_from_scratch_after_many_updates)'
```

Result: 3 passed.

Read-set / demand-driven tripwire:

```sh
cargo nextest run -p vix --features real-process -E 'binary(demand_driven)'
```

Result: 35 passed.

Full fold gate:

```sh
cargo check --workspace --all-targets
cargo nextest run -p vix --features real-process
cargo nextest run -p weavy --all-features
cargo nextest run -p facet-core --no-default-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Results:

| Gate | Result |
|---|---:|
| `cargo check --workspace --all-targets` | passed |
| `cargo nextest run -p vix --features real-process` | 230 passed |
| `cargo nextest run -p weavy --all-features` | 122 passed |
| `cargo nextest run -p facet-core --no-default-features` | 69 passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed |

## Ring-16 After Measurement

Non-stax control command:

```sh
TIER_A_OUT=/tmp/tier-a-lever4-after-ring16-direct \
target/profiling/deps/cargo_manifest-689b8bbc891035f7 \
real_workspace_member_direct_sparse_solve_ring_lock_diff_16 \
--ignored --exact --nocapture
```

Result: test passed in 57.80s wall.

Bucket comparison:

| Step | Before | After |
|---|---:|---:|
| sparse rows | 2,638 | 2,638 |
| packages | 67 | 67 |
| clauses | 160 | 160 |
| `solve_and_lock_diff` | 40,867.649 ms | 38,586.418 ms |
| matches | 26 | 26 |
| version skew | 0 | 0 |

The controlled solve bucket improved by about 5.6% while preserving the lock
diff shape.

## Stax Run 42

Stax command:

```sh
TIER_A_OUT=/tmp/tier-a-lever4-after-ring16 \
stax record -- \
target/profiling/deps/cargo_manifest-689b8bbc891035f7 \
real_workspace_member_direct_sparse_solve_ring_lock_diff_16 \
--ignored --exact --nocapture
```

The stax-wrapped run completed in 178.92s, so its wall is not the control wall.
Its flame is still useful for the next trunk:

| Trunk | Active share |
|---|---:|
| `Machine::demand_i64` | 98.6% |
| `Driver::projection_memo_hit` | 75.6% |
| `projection_observation_hash` | 74.7% |
| `SchemaTables::display_ref` | 66.8% |
| `Task::run_hosted` | 21.0% |
| `intern_molten_word` | 14.5% |
| `ValueStore::map_get` | 5.1% |

Lever 4 moved the map interning trunk down; the next evidenced trunk is
projection observation hashing, especially schema display during observation
hashing.
