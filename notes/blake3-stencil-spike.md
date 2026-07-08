# BLAKE3 Stencil Spike Receipts

This note pins the evidence from the hash-as-field BLAKE3 stencil spike on
`blake3-stencil-microbench`.

## Round 1: Portable Scalar Stencil

Round 1 implemented a self-contained portable scalar BLAKE3 compression stencil
and called it from a copied-code chain with a task-shaped `Ctx`. The measured
shape was streaming one-shot buffers, which is not the allocator/intern hot
shape, but it established the copy-patch expression constraints.

| size | native blake3 | host-call blake3 | stencil portable | host/native | stencil/host |
|---:|---:|---:|---:|---:|---:|
| 1 KiB | 1288.6 ns (0.74 GiB/s) | 1405.1 ns (0.68 GiB/s) | 1270.8 ns (0.75 GiB/s) | 1.09x | 0.90x |
| 64 KiB | 51767.8 ns (1.18 GiB/s) | 52293.8 ns (1.17 GiB/s) | 106489.3 ns (0.57 GiB/s) | 1.01x | 2.04x |
| 1 MiB | 963743.0 ns (1.01 GiB/s) | 1049944.2 ns (0.93 GiB/s) | 1680143.8 ns (0.58 GiB/s) | 1.09x | 1.60x |

Per-call overhead from the same bench:

| component | ns/call |
|---|---:|
| empty copied-code entry | ~2 |
| empty host-call chain | ~5 |
| host-call boundary delta | ~3 |

Round-1 infrastructure findings:

- Copy-patch extraction rejects non-continuation relocations. Any stencil helper
  call, table load, literal pool, static schedule, panic path, or compiler
  builtin such as `memcpy` can make extraction fail.
- Stencil sources compile with `-O -C panic=abort` regardless of the Cargo
  profile, so benchmark profile changes do not relax stencil expression rules.
- The BLAKE3 message schedule must be expanded into literal call sites, not
  stored in a static table.
- Helper functions must be forced inline, and even then the generated object must
  be inspected by the extractor. "Looks inlineable" is not enough.
- `u32::rotate_right` worked in the scalar stencil; there was no scalar rotate IR
  blocker.
- The task JIT has no custom consumer-op extension point for a native BLAKE3 op.
  The bench had to assemble a copied-code chain directly rather than add a
  principled task op.
- The typed task vocabulary has no principled pointer/capability argument story.
  The bench passed raw pointer words in frame slots, which is acceptable for the
  spike but not a real machine ABI for weavy-native collections.

## Round 2: AArch64 NEON Stencil

Round 2 replaced the portable scalar competitor with a self-contained aarch64
NEON single-compression implementation. The BLAKE3 crate's C NEON source was
used as reference, but `blake3` 1.8.5 does not provide a single-compression NEON
function: `blake3_neon.c` has `TODO: compress_neon` and routes `hash_one_neon`
through the portable compression. The stencil therefore implements the missing
one-block NEON compression directly.

The measured command was:

```sh
cargo bench -p weavy --features jit --bench blake3_stencil
```

All stencil and inline outputs were validated against the `blake3` crate before
timing.

### Single Intern Value

This shape hashes one small value with one hasher init per value. The host-call
column intentionally includes the current per-value crate init/dispatch shape.

| size | host-call crate incl init | stencil NEON | native crate ceiling | inline NEON no boundary | stencil/host | inline/host |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 213.4 ns (0.14 GiB/s) | 706.1 ns (0.04 GiB/s) | 295.8 ns (0.10 GiB/s) | 704.8 ns (0.04 GiB/s) | 3.31x | 3.30x |
| 128 B | 430.0 ns (0.28 GiB/s) | 1084.2 ns (0.11 GiB/s) | 540.3 ns (0.22 GiB/s) | 956.6 ns (0.12 GiB/s) | 2.52x | 2.22x |
| 256 B | 659.4 ns (0.36 GiB/s) | 1452.3 ns (0.16 GiB/s) | 772.5 ns (0.31 GiB/s) | 1528.4 ns (0.16 GiB/s) | 2.20x | 2.32x |
| 1 KiB | 2071.9 ns (0.46 GiB/s) | 5249.3 ns (0.18 GiB/s) | 2108.8 ns (0.45 GiB/s) | 5468.1 ns (0.17 GiB/s) | 2.53x | 2.64x |

### Carried Fold

This shape folds two 32-byte chaining values with one parent compression.

| op | host-call crate | stencil NEON | native crate ceiling | inline NEON no boundary | stencil/host | inline/host |
|---|---:|---:|---:|---:|---:|---:|
| parent fold | 138.0 ns | 364.5 ns | 140.1 ns | 356.4 ns | 2.64x | 2.58x |

### Batched Cache-Resident Values

This shape hashes N same-sized cache-resident values back-to-back. The host path
still pays one host-call crate hash per value; the stencil path hashes the batch
under one copied-code entry.

| batch | host-call crate per value | stencil NEON per value | native crate per value | inline NEON per value | stencil/host | inline/host |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B x256 | 129.3 ns | 174.9 ns | 116.7 ns | 195.4 ns | 1.35x | 1.51x |
| 128 B x256 | 332.8 ns | 584.4 ns | 348.0 ns | 577.7 ns | 1.76x | 1.74x |
| 256 B x256 | 212.4 ns | 612.9 ns | 239.9 ns | 568.4 ns | 2.89x | 2.68x |
| 1 KiB x256 | 809.7 ns | 2046.5 ns | 805.4 ns | 2039.5 ns | 2.53x | 2.52x |

### Round-2 Overhead Breakdown

| component | ns/call |
|---|---:|
| empty copied-code entry | 1.8 |
| empty host-call chain | 5.2 |
| host-call boundary delta | 3.3 |
| 32 B host-call minus native crate | -82.5 |
| 32 B stencil entry minus inline NEON | 1.3 |
| 128 B host-call minus native crate | -110.4 |
| 128 B stencil entry minus inline NEON | 127.6 |
| 256 B host-call minus native crate | -113.1 |
| 256 B stencil entry minus inline NEON | -76.0 |
| 1 KiB host-call minus native crate | -37.0 |
| 1 KiB stencil entry minus inline NEON | -218.8 |
| fold host-call minus native crate | -2.1 |
| fold stencil entry minus inline NEON | 8.2 |

Round-2 infrastructure findings:

- The NEON stencil is expressible and callable from copied code without a host
  boundary.
- The particular single-compression NEON implementation measured here does not
  beat the host-call crate path in any measured intern, fold, or batch shape.
- NEON rotate support is expressible with `core::arch::aarch64` shifts/or and
  lane rotates; no rotate IR blocker appeared.
- Non-continuation relocation constraints are stricter in the SIMD version:
  vector constants, literal pools, byte-mask lowering, schedule arrays, helper
  calls, and compiler-emitted `memcpy` all had to be avoided or forced into
  inline scalar/volatile construction.
- The BLAKE3 crate's production SIMD advantage comes from platform-specific
  implementation and multi-input chunk batching. A hand-written one-compression
  NEON stencil is not the crate's optimized path.
- The task-vocabulary gaps from round 1 remain the critical proposal input:
  weavy-native collections need a principled pointer/capability argument
  vocabulary and a consumer-op extension path before this can become a normal
  machine operation instead of a harness-only copied-code chain.

## Projection

The counter instrumentation is opt-in via `VIX_HASH_WORKLOAD_COUNTS=1`. The
ring harness writes `real-direct-ring-*-hash-workload-*.tsv` under `TIER_A_OUT`
at each major phase.

Projection model:

- Hash-as-field has landed.
- Observation/map/alloc recomputation is gone.
- Residual counted workload is `raw_new` intern payloads plus carried folds.
- `raw_attempt` is retained as a diagnostic for today's recompute-heavy world,
  but not used for the post-lever projection.
- Buckets are charged at the matching measured sizes: `0..32` -> 32 B,
  `33..128` -> 128 B, `129..256` -> 256 B, `257..1024` -> 1 KiB.
- `>1024` has count but not byte totals in this probe, so it is charged at the
  1 KiB rate as a lower bound.

Commands:

```sh
VIX_HASH_WORKLOAD_COUNTS=1 \
TIER_A_OUT=/tmp/blake3-stencil-ring16-debug \
cargo nextest run --profile debug -p vix --run-ignored only \
  -E 'test(=real_workspace_member_direct_sparse_solve_ring_lock_diff_16)'

VIX_HASH_WORKLOAD_COUNTS=1 \
TIER_A_OUT=/tmp/blake3-stencil-ring32-debug \
cargo nextest run --profile debug -p vix --run-ignored only \
  -E 'test(=real_workspace_member_direct_sparse_solve_ring_lock_diff_32)'
```

Ring-16 completed. Ring-32 was interrupted during solve/diff after 1109.238s;
the ring-32 projection below is therefore the final pre-solve/index-debug
lower-bound, not a completed solve count.

### Workload Counts

| ring | phase | raw attempts | raw new | carried folds | raw new 0..32 | 33..128 | 129..256 | 257..1024 | >1024 |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 16 | solve-lock-diff | 178764 | 9610 | 568 | 4189 | 294 | 227 | 1031 | 3869 |
| 32 | typed-index-debug | 288096 | 14719 | 244 | 5758 | 352 | 354 | 1795 | 6460 |

### Post-Lever Residual Projection

Seconds per ring if residual hashing is the last remaining cost:

| ring | host-call crate | stencil NEON | native crate ceiling | inline NEON no boundary |
|---|---:|---:|---:|---:|
| 16 completed | 0.011401 s | 0.029535 s | 0.011986 s | 0.030577 s |
| 32 pre-solve lower bound | 0.018751 s | 0.048383 s | 0.019609 s | 0.050162 s |

For comparison, charging all current raw hash attempts instead of post-lever
`raw_new` gives:

| ring | host-call crate | stencil NEON | native crate ceiling | inline NEON no boundary |
|---|---:|---:|---:|---:|
| 16 attempts | 0.047848 s | 0.149584 s | 0.062416 s | 0.150203 s |
| 32 attempts pre-solve | 0.077429 s | 0.242008 s | 0.100858 s | 0.243233 s |

Interpretation: in the post-lever world measured here, residual hashing is
small in absolute seconds per ring, and this NEON stencil is slower than the
host-call crate path. The result does not say "SIMD hash is useless"; it says
this copied-code single-compression NEON stencil is not the profitable
implementation lane. The durable proposal input is the ABI gap: weavy-native
collections still need pointer/capability vocabulary and a consumer-op extension
path before a better native hash implementation can be integrated cleanly.
