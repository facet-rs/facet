# JIT decode entry overhead — nperf annotate findings

Workload: `examples/profile_many_variants_v0` decoding `ManyVariants::V00`
(1-byte payload — pure entry overhead, no real decode work) in a tight loop.

## Per-iteration cost (release, 8s warm window)

| API path                                                  | iters/sec | ns/iter |
|-----------------------------------------------------------|----------:|--------:|
| `try_decode_owned` (public API, used by benches)          |  16.0M    |   62 ns |
| `decode_owned_with(&CompiledDecoder)` (pre-resolved)      |  91.0M    |   11 ns |

→ ~51 ns per call is **pure dispatch glue** that has nothing to do with the
   decoded value. The actual JIT'd decode + result move costs ~11 ns.

## Where the 51 ns goes (samples / 23,562 total at 4 kHz)

```
vox_jit::cache::CompiledCache::get_decode    6176  26%   ArcSwap.load + HashMap<DecodeKey> lookup (SipHash)
vox_jit::JitRuntime::try_decode_owned        5462  23%   stack setup, DecodeCtx zeroing, jumps
arc_swap::debt::list::LocalNode::with        3789  16%   ArcSwap RCU debt-slot acquire (xchg + inc)
vox_decode_ManyVariants_0 [JIT]              3386  14%   actual decode (variant tag varint + write)
profile_many_variants_v0::main               1761   7%   bench loop overhead
core::hash::BuildHasher::hash_one            1187   5%   SipHash13 driver
core::hash::sip::Hasher::write                712   3%   SipHash13 round body
vox_jit::JitRuntime::prepare_decoder          629   3%   force_fallback check + cache miss path
facet_core::types::const_typeid::of           397   2%   typeid lookup (probably feeding hash)
vox_jit::CodecMode::from_env                   56  ~0%   OnceLock-cached, basically free
```

So: **34% of every decode is hashing a `DecodeKey { &Shape, BorrowMode, u64 }`
through SipHash13** — for a key whose first field is a stable code-segment
pointer, second is a single byte, third is a u64. Cryptographic hashing is
strictly wasted work here.

**16% more is ArcSwap debt-slot acquire** — the cost of holding a reference
through `arc_swap::ArcSwap::load`. Visible as `LocalNode::with` and dominated
by an `xchg` + `inc` on a thread-local slot.

## Hottest single instructions

In the `try_decode_owned` body the `xorps + movups` zeroing of the on-stack
`DecodeCtx` has 2796 samples — i.e. allocating zeroed stack space dwarfs the
JIT'd work. Cranelift writes results back to a `MaybeUninit<T>` and we then
read them out with two `movups` — those reads stall on store-to-load
forwarding (16444 samples on one `movups xmm1, [rsp + 0x158]` in the
pre-resolved-path bench, where it dominates the loop). That STLF stall is
the real floor of the 11 ns pre-resolved path.

## Highest-ROI fixes

1. **Replace SipHash on `CompiledCache` HashMaps with a non-adversarial
   hasher** keyed on pointer identity (FxHash, foldhash, or a custom u64
   identity hash). Kills the ~34% hashing slice cleanly. This is one type-
   alias change in `cache.rs`. Should drop public-API decode from 62 → ~40 ns.

2. **Make production call sites use the pre-resolved path.** Generated
   dispatchers / handlers / conduits already have access to `prepare_decoder`
   and can hold the `&'static CompiledDecoder`. Once they do, the 51 ns of
   glue collapses to 0. (This is the existing TODO "push macro into
   generated dispatcher/handler/caller code".) Benches should also use
   `decode_owned_with` directly so they measure decode, not cache lookup.

3. **Skip ArcSwap on read** for the global cache. Since insertions already
   `Box::leak` and are append-only-ish, we can replace `ArcSwap<HashMap>`
   with a `RwLock<HashMap>` (read-mostly so the rw-lock is cheap) or
   `boxcar::Vec` indexed by a small per-shape id. Not as easy as (1).

4. **Avoid the `MaybeUninit<T>` round-trip** in `decode_owned_with`. Pass a
   `*mut T` straight through to the JIT'd function and return `Result<()>`.
   Removes the read-back `movups` pair that hits STLF. Tricky because the
   public API has to return a `T` — but we can construct it via
   `MaybeUninit::write` and let LLVM see through the dependency.

## After landing (1) [museair] + drop BorrowMode from cache key

Cache redesign: `(shape, schema_id)` → one `CompiledDecoder` with
`OnceLock<owned_fn>` + `OnceLock<borrowed_fn>` (both lazily filled), instead
of two cache entries per `(shape, mode, schema_id)`. Hash input shrinks from
17 → 16 bytes and entry duplication goes away. Hasher swaps from
`std::collections::HashMap`'s `RandomState` (SipHash13) to
`museair::FixedState`.

| API path                                                  | iters/sec | ns/iter |
|-----------------------------------------------------------|----------:|--------:|
| `try_decode_owned` — before                               |  16.0M    |   62 ns |
| `try_decode_owned` — after BorrowMode drop                |  16.7M    |   60 ns |
| `try_decode_owned` — after BorrowMode drop + museair      |  23.5M    |   43 ns |
| `decode_owned_with` (pre-resolved, control)               |  90.5M    |   11 ns |

Public-API entry overhead: 62 → 43 ns (**~31% off**, ~19 ns saved).

Divan `many_variants jit_decode` median across V0..V15: ~42–55 ns
(was ~58–72 ns). Gap to serde shrunk from ~50 ns to ~30 ns for the
no-payload variant.

Re-profiled with nperf annotate after the change:

```
vox_jit::cache::CompiledCache::get_decode    6359  27%   ArcSwap.load + HashMap<DecodeKey> lookup (museair)
vox_jit::JitRuntime::try_decode_owned        5036  21%
arc_swap::debt::list::LocalNode::with        4241  18%
profile_many_variants_v0::main               2875  12%   bench loop
vox_jit::JitRuntime::prepare_decoder         2777  12%   incl. OnceLock fast-path check on the entry
vox_decode_ManyVariants_0 [JIT]              1255   5%
museair::IncrementalHasher::finish            581   2%
facet_core::types::const_typeid::of           344   1%
```

Hashing collapsed from 8% (SipHash) to 2% (museair), and the absolute
sample count for hashing went from 1899 → 581 even though throughput is
~2.7× higher. Remaining big slices are still ArcSwap debt-slot acquire
(~18%) and the cache `get_decode` body (~27% — half of which is the
ArcSwap load itself). Those are the next two levers — fix (3) and fix (4).
