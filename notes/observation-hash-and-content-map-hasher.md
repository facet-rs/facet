# Observation Hash and Content Map Hasher

## Persistence Answer

`ProjectionRead.observed` is memo-internal. It is stored in
`MemoEntry.read_set`, and `MemoEntry` lives only in `Driver.memo`.
`verify_projection_read_set` recomputes the observation hash against current
arguments and compares it with the stored in-memory value.

The observation hash does not cross the durable value/export boundary:
`ValueBundle` exports only `StoreValue { handle, schema, tier, bytes,
content_hash, taint }` plus code bundles. Read sets and observation hashes are
not serialized there and are not emitted as exec receipts.

Changing observation-hash bytes is therefore not an old-epoch content-hash
break. It can still change warm in-process memo verification behavior, so the
demand-driven read-set tripwires remain load-bearing.

## Change

Two map-heavy tactical changes:

- Field projection observation hashing now hashes the descriptor `SchemaRef`
  directly through `canonical_word_hash_for_descriptor`, avoiding
  `descriptor_word_schema` and `SchemaTables::display_ref` in the verification
  path.
- Internal Rust maps keyed by content-derived values use `IdentityBuildHasher`
  instead of the default SipHash path:
  - `ValueStore.by_content`
  - `ValueStore.decoded_map_rows`
  - `Driver.memo`
  - `Driver.memo_candidates`
  - `InFlightInvocations.keys`
  - the local parked-waiter map keyed by `CanonMemoKey`
  - ELF/AST/crate/OCI projection memo tables keyed by `ContentHash`

Persisted value content hashes still use the existing content-hash functions.

## Proof

Focused projection/read-set selection:

```sh
cargo nextest run -p vix --features real-process \
  -E 'test(carried_map_rows_allocate_with_recomputed_hash) | test(map_projection_hit_ignores_untouched_entry_and_misses_touched_entry) | test(record_projection_hit_ignores_untouched_field_and_misses_touched_field) | test(projection_read_sets_survive_warm_reload) | test(shared_calls_spawn_once)'
```

Result: 5 passed.

Demand-driven tripwire:

```sh
cargo nextest run -p vix --features real-process -E 'binary(demand_driven)'
```

Result: 35 passed.

Force-copy and molten mutation tripwires:

```sh
cargo nextest run -p vix --features real-process \
  -E 'test(molten_reuse_is_unobservable_for_aggregate_updates) | test(map_get_cache_observes_insert_after_get_after_insert) | test(molten_array_carried_hash_matches_from_scratch_after_many_updates)'
```

Result: 3 passed.

Compile/lint:

```sh
cargo check -p vix --features real-process --all-targets
cargo clippy -p vix --features real-process --all-targets -- -D warnings
cargo nextest run -p vix --features real-process
```

Result: check and clippy passed; full vix suite passed 230 tests with 36
skipped.

## Ring-16 Measurement

Non-stax control command:

```sh
TIER_A_OUT=/tmp/tier-a-observation-hash-after-ring16-direct \
target/profiling/deps/cargo_manifest-689b8bbc891035f7 \
real_workspace_member_direct_sparse_solve_ring_lock_diff_16 \
--ignored --exact --nocapture
```

Result: test passed in 49.17s wall.

Bucket comparison against the previous lever-4 column:

| Step | Lever 4 | After |
|---|---:|---:|
| sparse rows | 2,638 | 2,638 |
| packages | 67 | 67 |
| clauses | 160 | 160 |
| `solve_and_lock_diff` | 38,586.418 ms | 33,784.507 ms |
| matches | 26 | 26 |
| version skew | 0 | 0 |

The controlled solve bucket improved by about 12.4% relative to the lever-4
column, with the same lock-diff shape.

## Stax Run 43

Stax command:

```sh
TIER_A_OUT=/tmp/tier-a-observation-hash-after-ring16-stax \
stax record -- \
target/profiling/deps/cargo_manifest-689b8bbc891035f7 \
real_workspace_member_direct_sparse_solve_ring_lock_diff_16 \
--ignored --exact --nocapture
```

Stax-wrapped result: test passed in 53.23s wall; `solve_and_lock_diff`
37,787.494 ms.

Flame:

| Trunk | Run 42 | Run 43 |
|---|---:|---:|
| `Machine::demand_i64` | 98.6% | 95.2% |
| `Driver::projection_memo_hit` | 75.6% | 39.5% |
| `projection_observation_hash` | 74.7% | 36.9% |
| `SchemaTables::display_ref` | 66.8% | not visible over 1% |
| `canonical_word_hash_for_descriptor` | n/a | 25.8% |
| `Task::run_hosted` | 21.0% | 42.6% |
| `intern_molten_word` | 14.5% | 23.6% under hosted, plus 7.9% under projection path |
| `ValueStore::alloc_raw_tainted` | n/a | 5.9% |
| `ValueStore::map_get` | 5.1% | 4.4% |

The intended display-name trunk is gone. The next visible observation trunk is
descriptor-ref word hashing plus descriptor lookup.
