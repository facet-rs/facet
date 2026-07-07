# CDCL hot-loop rematch

Branch: `cdcl-incremental-hash`

This spike re-ran the CDCL hot-loop experiment with the content-hash
confounder separated from molten copy amplification. The benchmark code lives
in `vix/src/bin/cdcl_molten_bench.rs`.

## Workload shapes

The same word "trail" covers several materially different paths:

| Mode | Shape | What it measures |
| --- | --- | --- |
| `store-copy-hash` | Direct `ValueStore` growing-prefix intern loop, cloning the whole `Vec<i64>` and interning each prefix. | Historical prior-art shape: O(N^2) prefix hashing plus store allocation/dedup work. |
| `store-hash-only` | Same growing prefixes, but only computes the full array hash for each prefix. | Hash-only component of the direct store loop. |
| `store-copy-only` | Same growing prefixes, but only clones the vector. | Raw copy component, without store/hash. |
| `store-append` | Direct `ValueStore` append path added in this spike. | Prefix metadata plus O(1) append-hash combine. |
| `machine-rebind` | Generated vix source: `let trail = trail.push(i)` repeated. | Current flagship language idiom. `resolve_binding` emits `MOLTEN_DUP`; refs become 2; the `ARRAY_PUSH` `refs == 1` gate does not fire. This is copy amplification, not hash amplification. |
| `machine-chain-reuse` | Generated vix source: `([0]).push(1).push(2)...`. | Temporary-chain shape where refs stay 1 and molten reuse fires. |
| `synthetic-blake3-append` | Benchmark-only append hash using blake3 and a local `HashMap`. | Synthetic hash-family comparison only. It is not a runtime content-hash migration. |

The sibling `molten-consume` worktree existed during measurement, but it was at
the same HEAD as this branch at the time I checked. I did not measure a distinct
consume-move implementation.

## Rust reference baseline

Recovered read-only from `/Users/amos/vixenware/vixen` at `10df3a05^` into a
throwaway `/tmp/rodin-ref.GuEcJL` copy. The vixenware checkout was not modified.
I narrowed the throwaway workspace manifest to the retired Rodin crates and
added a temporary release harness that repeatedly calls
`rodin_core::solve_round3_record("downgrade-cascade")`.

Representative command:

```sh
./target/release/rodin-solve-bench --scenario downgrade-cascade --runs 20000
```

Observed medians varied at microbenchmark scale: 139 us, 229 us, 240 us,
267 us, and 381 us across repeated runs. I use the 20k-run median, 267 us, as
the factor baseline below and report factors with that caveat.

## Numbers

| Row | Steps | Median | Factor vs 267 us Rust ref | Notes |
| --- | ---: | ---: | ---: | --- |
| Rust reference `downgrade-cascade` | n/a | 0.267 ms | 1.0x | Release build, recovered retired solver. |
| vix before, `store-copy-hash` | 4096 | 1948.417 ms | 7292x | Current-base pre-fix direct growing-prefix intern path. |
| vix after, `store-append` | 4096 | 4.491 ms | 16.8x | Prefix metadata + O(1) append-combine, SHA-256. |
| Synthetic `blake3` append | 4096 | 1.693 ms | 6.3x | Benchmark-only hash-family swap, not runtime vix. |
| Rust `Vec` ceiling | 4096 | 0.008 ms | 0.03x | Plain push loop. |

Machine-shape timings are a separate attribution table because they do not
intern every prefix into the store:

| Row | Steps | Median seen | Factor vs 267 us Rust ref | Notes |
| --- | ---: | ---: | ---: | --- |
| `machine-rebind` | 2048 | 1.6-3.7 ms | 6.0-14.0x | Named rebind triggers `MOLTEN_DUP`, so reuse does not fire. |
| `machine-chain-reuse` | 2048 | 0.10-0.20 ms | 0.39-0.73x | Temporary chain keeps refs at 1; reuse fires. |
| `machine-chain-copy` | 2048 | 1.4-2.4 ms | 5.4-9.0x | Forced-copy comparator for the temporary-chain source. |

The machine numbers are noisier because the generated source goes through the
vix parser/lowerer in the same process, and stax samples process startup unless
attaching after load. The wall-clock medians still show the key shape result:
temporary-chain reuse is roughly an order of magnitude faster than the rebind or
forced-copy paths.

## Baseline decomposition

Sequential pre-fix direct-store timings at 4096 prefixes:

| Component | Median |
| --- | ---: |
| `store-copy-hash` | 1948.417 ms |
| `store-hash-only` | 1432.428 ms |
| `store-copy-only` | 1.297 ms |

So the historical direct intern workload was not primarily raw vector copying.
The raw clone-only component is below 0.1% of the full path. Hash-only explains
most of the wall time; the rest is store encoding, allocation/dedup, and map
work.

`stax` baseline run:

```sh
stax record -- ./target/profiling/cdcl_molten_bench --mode store-copy-hash --steps 4096 --runs 1
stax flame -d 18 --threshold-pct 1
stax top -n 30 --sort self
```

Run 1 printed `store-copy-hash: median=1488.021ms` and collected 908 kperf
samples. `stax top` put `sha2::sha256::compress256` at 81.474 active ms out of
92.46 active ms, about 88% of sampled active time. The flame showed
`ValueStore::alloc_array_words` under `store_copy_hash`, with the dominant
children in SHA-256 compression and `canonical_word_hash_in_store`.

That confirms the prior 86% SHA profile for the direct intern loop, once the
workload is named precisely.

## After-fix decomposition

The port adds:

- `ValueStore::array_words_meta`, keyed by store handle.
- Canonical array word hashes defined as empty-prefix hash plus repeated
  append-combine.
- `ValueStore::alloc_array_words_append`, which writes a compact append entry
  and computes the next content hash in O(1).
- Oracle tests proving append construction dedupes with literal construction,
  including tainted child handles.

`stax` after-fix run:

```sh
stax record -- ./target/profiling/cdcl_molten_bench --mode store-append --steps 4096 --runs 1000
stax flame -d 18 --threshold-pct 1
stax top -n 30 --sort self
```

Run 4 printed `store-append: median=4.700ms` and collected 1037 kperf samples.
The flame put 95.9% under `ValueStore::alloc_array_words_append_for_bench`.
Residual sampled active time was roughly:

| Class | Evidence |
| --- | --- |
| SHA-256 append/scalar hashing | `sha2::sha256::compress256` 156.747 active ms self, about 50% of sampled active time. |
| Store dedup map work | `alloc_with_hash_tainted`, `HashMap::insert`, `RawTable::reserve_rehash`, SipHash/`BuildHasher::hash_one`; about 25-30% by flame/top grouping. |
| Memory movement/allocation | `_platform_memmove`, allocator/free/drop frames; visible but no longer the O(N^2) trunk. |

The O(N^2) SHA walk is removed. The residual is per-append SHA-256 plus hash-map
dedup bookkeeping.

## Verdict

For the historical direct growing-prefix intern loop, the old reuse result was
indeed masked by content hashing. The prefix-hash port moved 4096 prefixes from
about 1.95 s to about 4.5-5.0 ms, a roughly 430x speedup on this machine.

Against the Rust reference baseline, the SHA-256 `store-append` row is still not
inside the 4.8x bar. With the 267 us reference median, the bar is 1.28 ms and
`store-append` is 16.8x, about 3.5x over the bar. The benchmark-only blake3 row
gets to 1.69 ms, which is near the bar but still above it against the 267 us
baseline; because the reference ranged from 139-381 us, this remains a noisy
"near" result, not a clean pass.

For the vix language hot loop, the key new attribution is the shape split:

- `let trail = trail.push(lit)` currently measures molten copy amplification,
  because `MOLTEN_DUP` raises refs and the reuse gate does not fire.
- A temporary chain does hit molten reuse and is already faster than the Rust
  reference on the 2048-push shape.

So the runtime/language still needs:

1. Consuming-move/drop semantics for the shadowing aggregate-update idiom, so
   the flagship `let trail = trail.push(lit)` path reaches refs=1.
2. A planned hash epoch if vix wants blake3-class append hashing in the actual
   content identity path.
3. Less per-append store bookkeeping if direct prefix interning remains the
   modeled CDCL path: the post-fix profile points at `HashMap`/SipHash/rehasher
   work after SHA-256.

The answer is therefore: the hash confounder was real and large, but removing it
does not by itself make the direct store-intern workload clear the 4.8x bar.
The temporary-chain machine shape shows the molten reuse design can clear the
bar when refs stay unique; the named-rebind shape still needs the parallel
consume-move fix before it is a fair reuse measurement.

## Rematch after consuming-move rebase

Rematch base: fetched `origin/rodin` and rebased this branch over
`0b62d0ec1 vix: consume rebound molten update receivers`. The rebase completed
without conflicts. The benchmark commit now sits at `135a0ba39` before this
report-only update.

The important semantic change is that the flagship shape:

```vix
let trail = trail.push(lit)
```

now lowers the receiver as a consuming move. That removes the `MOLTEN_DUP`
refcount bump for the shadowed receiver, so the `ARRAY_PUSH` `refs == 1` reuse
gate fires for the named-rebind idiom.

### Rematch numbers

Commands used the rebased profiling binary:

```sh
cargo build --profile profiling -p vix --bin cdcl_molten_bench
./target/profiling/cdcl_molten_bench --mode machine-rebind --steps 2048 --runs 1000
./target/profiling/cdcl_molten_bench --mode machine-chain-reuse --steps 2048 --runs 1000
./target/profiling/cdcl_molten_bench --mode machine-chain-copy --steps 2048 --runs 1000
./target/profiling/cdcl_molten_bench --mode machine-rebind --steps 4096 --runs 1000
./target/profiling/cdcl_molten_bench --mode store-append --steps 4096 --runs 1000
./target/profiling/cdcl_molten_bench --mode synthetic-blake3-append --steps 4096 --runs 1000
```

| Row | Steps | Median | Factor vs 267 us Rust ref | Notes |
| --- | ---: | ---: | ---: | --- |
| `machine-rebind` before consume move | 2048 | 1.6-3.7 ms | 6.0-14.0x | From the first pass; reuse did not fire. |
| `machine-rebind` after consume move | 2048 | 70.792 us | 0.27x | Named rebind now matches the reuse shape. |
| `machine-chain-reuse` after rebase | 2048 | 66.916 us | 0.25x | Temporary-chain comparator. |
| `machine-chain-copy` after rebase | 2048 | 786.583 us | 2.94x | Forced-copy comparator; still faster than the old 1.4-2.4 ms run, but much slower than reuse. |
| `machine-rebind` after consume move | 4096 | 193.584 us | 0.72x | Same named-rebind idiom at the direct-store step count. |
| `machine-chain-reuse` after rebase | 4096 | 187.000 us | 0.70x | Same scale, temporary-chain comparator. |
| `machine-chain-copy` after rebase | 4096 | 3.639 ms | 13.6x | Copy amplification remains visible when reuse is disabled. |
| `store-append` after rebase | 4096 | 2.449 ms | 9.2x | Direct per-prefix store append-hash path, SHA-256. |
| `synthetic-blake3-append` after rebase | 4096 | 1.392 ms | 5.2x | Benchmark-only hash-family projection. |

The stax run for `machine-rebind --steps 4096 --runs 10000` printed
`machine-rebind: median=143.875us mean=146.457us`. That is the best measured
end-to-end named-rebind run on the rebased tree: 0.54x the 267 us Rust
reference. The non-stax 1000-run median was 0.72x. Either way, the language
hot-loop bar is cleared by a wide margin: the 4.8x threshold is 1.282 ms.

This is not the same as the direct `store-append` workload. The machine
benchmark mutates a molten array in place and asks for `trail.len()`, so the
content-hash path is not the sampled trunk there. The direct store workload
still measures per-prefix store identity and dedup bookkeeping.

### Rematch stax decomposition

Named-rebind language path:

```sh
stax record -- ./target/profiling/cdcl_molten_bench --mode machine-rebind --steps 4096 --runs 10000
stax flame --run 5 -d 18 --threshold-pct 0.5
stax top --run 5 -n 40 --sort self
stax threads --run 5 -n 20
```

Run 5 collected 1346 kperf samples, with total active time 190.41 ms and
`machine-rebind: median=143.875us`. The flame showed 97.2% under
`weavy::task::Task::run_hosted`, with 69.2% under `Driver::burst`. The largest
visible child was `String::clone`/allocator work around driver execution
(27.4% total under the clone frame, with allocator and memmove children).
`intern_molten_word` was about 1.1% total. `canonical_word_hash_in_store` showed
0 samples in `stax top`; no SHA-256 frame appeared in this profile.

So for the fixed named-rebind idiom, the residual is now language-driver/task
dispatch plus allocation/string-clone overhead, not array copying and not
content hashing.

Direct append-hash store path:

```sh
stax record -- ./target/profiling/cdcl_molten_bench --mode store-append --steps 4096 --runs 10000
stax flame --run 6 -d 18 --threshold-pct 0.5
stax top --run 6 -n 50 --sort self
stax threads --run 6 -n 20
```

Run 6 printed `store-append: median=2.594ms mean=2.639ms`, with total active
time 3.430 s. The flame put 95.1% under
`ValueStore::alloc_array_words_append_for_bench`.

| Residual class | Evidence from run 6 |
| --- | --- |
| SHA-256 | `sha2::sha256::compress256` had 1867.013 active ms self in `top`, about 54.4% of active time. In the flame it appears under `alloc_with_hash_tainted`, `array_words_push_hash`, and `canonical_word_hash_in_store`. |
| HashMap/SipHash/rehasher | `HashMap::insert`, `RawTable::reserve_rehash`, `BuildHasher::hash_one`, and SipHash frames account for the next large bucket, roughly 20-25% by flame grouping. |
| Allocation/free/drop | `ArrayWordsMeta` drop, `ValueStore` drop, allocator/free frames, and `mach_absolute_time` under allocator paths are visible residuals after hashing. |
| Memmove | `_platform_memmove` had 107.318 active ms self in `top`, about 3.1% of active time. It is no longer the trunk. |

### Rematch verdict

For the actual named-rebind vix language idiom after `0b62d0ec1`, the 4.8x bar
is met. The best rebased stax run is 0.144 ms, or 0.54x the 0.267 ms Rust
reference; the conservative non-stax 4096-run median is 0.194 ms, or 0.72x.

For the direct per-prefix store-intern workload, the bar is still short with
SHA-256: `store-append` is 2.45-2.59 ms, or 9.2-9.7x the Rust reference. Against
the 1.282 ms bar, that is still about 1.17-1.31 ms slow, roughly 1.9-2.0x over
the bar. The benchmark-only blake3 projection improves to 1.392 ms, or 5.2x,
which is close but still about 0.109 ms over the 4.8x threshold using the fixed
267 us reference baseline.

The updated answer is therefore split:

- Named-rebind language hot loop with molten reuse: bar met.
- Direct per-prefix store identity/dedup path with SHA-256 append hashing:
  still short, now dominated by SHA-256 plus HashMap/SipHash rather than O(N^2)
  prefix walks.
- Blake3 projection for the direct store path: near, but still not a clean pass
  against the chosen 267 us reference median.

Focused verification after the rebase:

```sh
cargo nextest list -p vix -E 'test(array_incremental_hash) | test(molten_consuming_rebind) | test(molten_reuse_is_unobservable_for_aggregate_updates)'
cargo nextest run -p vix -E 'test(array_incremental_hash) | test(molten_consuming_rebind) | test(molten_reuse_is_unobservable_for_aggregate_updates)'
```

The selected 5 tests all passed.
