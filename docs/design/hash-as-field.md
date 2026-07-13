# Hash as Field

Committee proposal only. No layout or identity epoch migration is implemented
here.

## Motivation

The direct sparse ring workload moved the bottleneck from composition to solve.
The exact-index native Rust reference probe now shows the multiplier directly,
rather than inferring it from LR microbenchmarks. The tracked table is in
`notes/tier-a-scale-measurement.md`; the flame receipt is in
`notes/solve-flame-run41.md`.

| Ring | Sparse rows | Package domains | Clauses | Vix solve wall | Native solve wall | Multiplier |
|---:|---:|---:|---:|---:|---:|---:|
| direct 16 | 2,638 | 67 | 160 | 40,867.649 ms solve+diff | 0.426006 ms | ~95k× |
| direct 32 | 4,424 | 114 | 449 | >180 s solve cap | 2.367375 ms | >76k× |

Scope caveat: the native reference uses the exact `Index` built by the Vix
probe and solves the package-version portion. It counts feature clauses but
does not perform feature-unit derivation.

The post-lever flame on the ring-32 workload says the solve wall is still
identity bookkeeping across the host boundary:

| Trunk | Active share | Meaning |
|---|---:|---|
| `Machine::demand_i64` | 95.8% | solve dominates the target run |
| `Task::run_hosted` | 49.6% | hosted demand execution dominates active time |
| `ValueStore::alloc_raw_tainted -> raw_value_content_hash -> blake3` | 23.4% | value identity is repeatedly recomputed at allocation |
| `Driver::intern_molten_word` | 29.1% | molten interning/allocation remains central |
| `intern_value_bytes_children -> intern_molten_word -> ValueStore::alloc_map` | 21.3% | map identity construction is a hot allocation path |
| `canonical_map_pairs` | 12.8% | canonical row construction is still visible |
| `hash_map_pairs` | 5.5% | old-epoch map byte hashing remains visible |
| `projection_memo_hit -> projection_observation_hash` | 13.6% | projection verification hashes still cross the host path |

The conclusion is not "make hashing a little faster." For solve-class
workloads, the current identity shape is existential: the machine spends orders
of magnitude more time proving and rebuilding identities than the native
resolver spends solving the same package-version problem.

## Target Shape

Treat a value's content hash as a field in the value layout, not as a repeated
host computation.

- Compute the content hash once at intern/allocation.
- Carry it incrementally through molten mutation where the mutation path can
  update the state without changing identity bytes.
- Store the finalized hash in a descriptor-reserved identity slot in the value
  layout.
- Lower `hash(value)` to an inline field load in JIT lanes.
- Keep the host boundary for allocation/intern and exec, not per-access hashing.

This is the ideal target. Current canonical-payload-encoding hash bytes are not
sacred for this proposal. If canonical zero padding plus flat-memory hashing is
ratified, identity should migrate in a second sanctioned epoch with explicit
breakage, gates, and differential oracles.

Anything landed before that epoch remains old-epoch hash-neutral.

## Canonical Memory

Weavy should adopt canonical zero padding for the b-ABI: always, with no
exceptions. With canonical padding, the hash input becomes the value's memory
representation, so identity can be BLAKE3 over flat bytes instead of a separate
canonical payload encoder.

Required invariants:

- Aggregate construction zero-initializes frame slots and arena allocations.
  Fresh pages may make this cheap; arena reuse must zero explicitly.
- Enum variant switches zero slack for the newly active variant. This is the
  recurring tax and should be quantified on the direct sparse ring workloads.
- `memcpy` preserves canonicity. Add an invariant test that copying canonical
  values produces canonical values.
- Debug builds check padding canaries at intern time: padding bytes must be
  zero, and a nonzero padding byte is a writer bug.
- The facet bridge canonicalizes discovered values at copy-in. The guarantee is
  minted at the bridge boundary and is never assumed for arbitrary external
  memory.

With canonical memory, `memcmp` equality becomes a follow-on lever for values
whose descriptors admit byte equality.

## BLAKE3 in the Machine

The current root cause is ABI ownership. Hashing crossed to Rust when only Rust
knew layouts. Weavy now owns the ABI, and the epoch makes payload bytes
little-endian canonical. The hash input is therefore the machine-visible value
memory.

Preferred lowering vehicle: a copypatch stencil for the portable BLAKE3
compression function.

- Start with the portable compression path: add, xor, rotate over the 16-word
  message and state words.
- Stencils compile optimized code and are callable from JIT lanes without a
  Rust host call.
- SIMD-tuned stencils can follow after the portable path is correct.
- IR orchestration feeds chunks, manages flags/counters, and finalizes output.
- New IR needed: rotate is the minimum required operation if it is not already
  available in the lane IR.

The interpreter lane can use the same semantic IR operation set first, with a
Rust fallback for the compression primitive if needed. Once the stencil calling
path is shared, the interpreter can call the portable stencil too. The key
design point is that stencil-backed hashing is a machine primitive, not a
JIT-only optimization that leaves interp on the old Rust host boundary.

## Representation

Descriptor-reserved identity storage should be part of the value layout:

- Store value: finalized content hash lives in the reserved identity slot.
- Molten value: either carries a finalized hash if no further mutation occurs,
  or carries mutation-local state sufficient to finalize the hash when interned.
- Descriptor metadata records where the identity slot lives and whether flat
  memory equality is valid.
- Hash reads load the identity slot. They do not reconstruct canonical bytes.

Molten mutation rules:

- Mutations that preserve a carried state update that state inline.
- Mutations that invalidate carried state clear it immediately and fall back to
  finalization at intern.
- Any cached decode or carried state is keyed by handle plus content identity,
  and mutable-in-place updates at `refs == 1` invalidate stale state.

## Migration Epoch

Canonical-memory hashing should be a second sanctioned identity epoch.

Expected breakage:

- Content hash bytes change for values whose old canonical payload encoding is
  not byte-identical to canonical memory.
- Memo keys, projection observation hashes, and store dedupe keys change with
  the epoch.
- Persisted receipts from the old epoch require an epoch tag or rebuild.

Gates and oracles:

- Old-epoch code remains byte-neutral until the epoch flag is deliberately
  flipped.
- Force-copy and demand-driven tripwires run in both epochs.
- Differential oracle compares semantic outputs, read sets, and memo
  verification behavior between old payload hashing and canonical-memory
  hashing.
- Expected hash-byte differences are allowed only behind the epoch boundary.
- Observation read-set recording must not lose captured reads; hash speedups
  cannot punch holes in receipts.

Migration steps:

1. Add descriptor-reserved identity slots and canonical padding checks without
   changing old hash bytes.
2. Lower portable BLAKE3 compression as a machine stencil and add the minimum IR
   operations.
3. Add field-load hash reads for values that already carry an old-epoch hash.
4. Add canonical-memory hashing behind an explicit second-epoch gate.
5. Run force-copy, demand-driven, and semantic differential oracles over the
   tier-A direct sparse ring probes.
6. Ratify the epoch break and remove old canonical-payload hashing only after
   the oracles and committee review accept the new identity.
