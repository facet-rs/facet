# Cranelift Translation JIT

Design notes for replacing the reflection-heavy postcard execution path with
Cranelift-generated decode and encode stubs.

This is a runtime design note, not a spec change.

## Status

The current runtime is semantically in the right place:

- schema exchange is sender-driven and mandatory
- translation plans are built before data is processed
- serialization always uses the sender's local type definition
- deserialization adapts to the sender's layout

The current performance problem is that the execution engine for those rules is
almost entirely reflective. In steady state, most CPU time is spent in
`facet_reflect::Peek`, `facet_reflect::Partial`, shape dispatch, and generic
container construction rather than in transport or translation-plan
construction.

The goal of this design is to keep the current semantics and replace the hot
interpreter with generated machine code.

## Spec Invariants

This design preserves the following requirements:

- `schema.exchange.required`
  Runtime specialization only runs after the normal schema/binding tracking has
  established that the remote type metadata exists for the current connection.

- `schema.errors.early-detection`
  Structural incompatibilities are still detected when building the translation
  plan. The JIT only executes an already-validated plan.

- `schema.translation.serialization-unchanged`
  Encoding still uses the sender's local type definition. JIT encode is an
  implementation detail, not a change in semantics.

The JIT is therefore not a new compatibility mechanism. It is a new execution
engine for the existing one.

## Non-Goals

- No persistent code cache across process restarts.
- No attempt to discover arbitrary Rust type layout at runtime.
- No attempt to make all std or ecosystem container internals first-class.
- No unwind through JIT-generated code.
- No change to the wire format, handshake, schema payloads, or translation-plan
  semantics.

## Core Idea

The runtime keeps the current three logical layers:

1. Schema extraction and exchange.
2. Translation-plan construction and compatibility checking.
3. Execution of encode/decode against those plans.

Only step 3 changes.

Today, step 3 is a reflective interpreter. The replacement is:

1. Lower a validated translation plan plus local layout metadata into a compact
   internal IR.
2. Compile that IR to a small machine-code stub using Cranelift.
3. Cache that stub for the life of the process.
4. Fall back to the interpreter for unsupported cases or when calibration fails.

## Architecture

### Decode Path

The primary target is decode, since that is where the current runtime pays for:

- recursive `Partial` construction
- shape-driven dispatch at each field
- generic list/map/set assembly
- repeated field enter/exit operations

The generated decoder reads postcard bytes from a cursor and writes directly
into the destination object according to:

- the local layout metadata for Facet-owned types
- the translation plan
- a small set of runtime helper calls for allocations and hard cases

Two decode families are expected:

- owned decode
- borrowed decode

They share most lowering and codegen but differ in how string/bytes-like fields
are materialized and how lifetimes are represented in runtime helpers.

### Encode Path

Encode is secondary but still valuable. The generated encoder:

- walks the sender's local layout directly
- writes postcard bytes into a buffer builder
- uses helper calls only for dynamic growth or opaque/proxy cases

Encode does not use translation plans. The sender still serializes using the
local type definition only.

### Plan Lowering

The translation plan remains the semantic source of truth. The JIT does not try
to rediscover compatibility. It lowers already-validated plan nodes such as:

- identity
- struct field mapping
- tuple element mapping
- enum variant mapping
- list/array/option/map recursion
- skip operations based on remote schema

This keeps all schema mismatch handling in one place and preserves the current
error model.

## Metadata Model

### Facet-Owned Types

Facet already provides the required layout knowledge for user-controlled types.
That metadata should remain the authoritative source for:

- size
- alignment
- field offsets
- enum tag and payload layout
- transparent/proxy handling
- drop requirements

The JIT should never infer this for Facet-owned types. It should consume
metadata prepared by normal Rust code and lower it into a codegen-friendly
form.

Shape identity for caching must be structural, not pointer-based. `Shape`
already implements hashing and equality in a way that is appropriate for cache
keys. Equivalent shapes may exist at different addresses, and raw pointer
identity is process-local and too weak to describe semantic compatibility.

### Opaque Types

The hard part is not Facet types. The hard part is standard-library and other
non-Facet container internals that are performance-critical and too expensive
to treat as fully opaque runtime helper calls on every operation.

This design therefore introduces a narrow class of calibrated opaque types.

Initial whitelist:

- `Vec<T>`
- `String`
- optionally `Box<T>` and `Box<[T]>` later

Everything else stays on helper-based or interpreter paths until there is a
clear reason to widen the set.

## Runtime Calibration

### Philosophy

The runtime does not try to discover "Rust ABI" in general. It probes a small
whitelist of concrete opaque types that the generated code wants to fast-path.

Calibration is:

- process-local
- target-local
- concrete-type-specific
- disposable

No persistence means the system can be aggressive without taking on cache
compatibility problems across compiler versions or process restarts.

### `Vec<T>`

For a concrete `Vec<T>`, calibration discovers:

- which slots are pointer, length, and capacity
- the exact byte representation of `Vec::<T>::new()`
- any invariants needed before enabling direct field stores

This is done by probing real `Vec<T>` values under `ManuallyDrop`, including a
non-empty value with `len != cap` so slot identity can be determined.

The important detail is that the empty representation is not modeled as
"obviously three zero words". The runtime records the actual bytes of
`Vec::<T>::new()` and reuses those exact bytes.

This must be cached per concrete `T`, because the empty representation may
depend on alignment.

### `String`

`String` is calibrated separately, even if the observed representation matches
`Vec<u8>`.

That keeps the fast path honest and avoids turning an implementation detail
into an undocumented cross-type axiom inside the compiler.

### Calibration Output

The JIT consumes compact descriptors such as:

- size
- align
- empty bytes
- slot offsets
- helper selection data

If calibration fails or produces unexpected results, the type is marked
unsupported and the runtime falls back.

## JIT ABI

The generated code uses a runtime-owned ABI, not Rust's aggregate calling
conventions.

Example decode entry shape:

```rust
extern "C" fn(
    ctx: *mut DecodeCtx,
    input_ptr: *const u8,
    input_len: usize,
    out_ptr: *mut u8,
) -> DecodeStatus
```

Properties:

- one stable boundary for all generated decode stubs
- no need to infer Rust argument passing rules
- all type layout questions are handled by metadata and calibrated descriptors
- helper calls use the same narrow ABI

There may be several entrypoint variants for:

- owned decode
- borrowed decode
- encode

But they should remain runtime-defined and minimal.

## IR Shape

The JIT IR should be smaller and more explicit than `TranslationPlan`. It is a
codegen IR, not a semantic IR.

Likely operations:

- read varint / scalar / fixed-width primitive
- copy byte slice
- branch on option tag
- branch on enum discriminant
- skip remote value
- compute local field address
- write primitive to destination
- materialize empty opaque value from calibrated bytes
- allocate backing store through helper
- commit list/string length
- call slow helper for unsupported shape fragments

The IR should not mention `Peek`, `Partial`, or shape reflection primitives.

## Generated Decode Strategy

### Structs and Tuples

For Facet-owned structs and tuples:

- compute destination field address from local metadata
- execute plan nodes in remote field order
- store directly into local field slots
- perform skip operations using remote schema-derived skip code

### Enums

For enums:

- decode remote discriminant
- map remote variant to local variant
- branch to per-variant block
- materialize the local tag and payload directly

Unknown remote variants remain runtime errors for the specific message, exactly
as they are today.

### Lists and Arrays

Arrays are straightforward: fixed trip count, direct element stores.

Lists are where the opaque fast path matters.

For `Vec<T>` decode:

- if decoded length is zero, copy the calibrated empty bytes into the
  destination and return
- otherwise call one helper to allocate backing storage
- decode elements directly into the backing storage
- update `len` only after each element has been successfully initialized
- write final `len` on success and return

This avoids calling `Vec::new()` or rebuilding the vector through reflective
list APIs on the hot path.

### Strings and Byte Buffers

For `String`:

- empty string uses calibrated empty bytes
- non-empty decode allocates once and copies bytes directly
- UTF-8 validation stays explicit and may call a helper if keeping it in JIT is
  not worthwhile

For byte buffers:

- `bytes`
- `Vec<u8>`
- `String`

the generated code should special-case contiguous copies aggressively.

## Runtime Helpers

The JIT should not try to own every corner case.

Small helper surface:

- reserve storage for `Vec<T>` / `String`
- validate UTF-8 if left out of line
- construct or tear down difficult opaque/proxy cases
- report decode failure without panicking
- drop partially initialized aggregate state on the failure path

Helpers must be:

- non-panicking
- ABI-stable within the process
- explicit about ownership transfer and cleanup responsibility

No unwind across generated frames.

## Safety Invariants

### Partial Initialization

This is the real hard part.

Generated code must obey the same invariants the reflective runtime currently
gets "for free" from higher-level APIs:

- do not expose initialized length/count before elements are initialized
- do not call drop on uninitialized memory
- keep enough bookkeeping to clean up initialized prefixes on failure
- do not publish borrowed references that outlive the input backing

For `Vec<T>`, that means:

- backing allocation may happen before the first element is decoded
- `len` must be committed only after each element is initialized
- failure cleanup must drop only the initialized prefix

### Error Handling

The JIT executes only validated plans, but data-level failures still exist:

- EOF
- varint overflow
- invalid UTF-8
- invalid enum discriminant
- unknown variant at runtime

Generated code returns status codes and leaves the runtime to translate them
into the existing error types.

### Borrowed Decode

Borrowed decode must preserve the current lifetime model:

- borrowed strings and byte slices point into the input backing
- returned values are still wrapped through the existing `SelfRef` discipline
- no generated code path can leak a longer lifetime than the caller provides

The codegen boundary should stay pointer-based. Lifetime correctness stays in
the surrounding Rust types and APIs.

## Caching

Compiled code is cached in memory only.

Likely decode cache key:

- remote root schema ID
- local shape value, or a canonical owned key derived from it
- direction
- borrow mode
- target ISA/profile
- opaque calibration generation or descriptor IDs

Likely encode cache key:

- local shape value, or a canonical owned key derived from it
- borrow mode if relevant
- target ISA/profile
- opaque calibration generation or descriptor IDs

The cache must not use `&'static Shape` pointer identity as a semantic key.
Using the shape value itself avoids accidental misses or collisions when the
same shape is materialized at different addresses.

If calibration for a required opaque type is unavailable, no stub is cached for
that case and execution falls back.

## Fallbacks

The reflection interpreter stays in the runtime:

- as the correctness oracle
- for unsupported shapes
- for unsupported opaque types
- for debugging and differential testing
- for environments where codegen is disabled

This is not optional during rollout.

## Rollout Plan

1. Add a non-reflective lowering pass from translation plans into a compact IR.
2. Add a pure interpreter for that IR.
3. Differential-test IR interpreter against the current reflective runtime.
4. Add Cranelift backend for a minimal subset:
   - primitives
   - structs
   - tuples
   - arrays
   - `Vec<u8>`
   - `String`
5. Add calibrated `Vec<T>` for selected element types.
6. Expand to enums and nested containers.
7. Consider JIT encode after decode is clearly correct and useful.

The IR interpreter is valuable even if the JIT is not yet enabled. It forces
the runtime to separate semantics from reflection before machine code enters the
picture.

## Testing Strategy

- differential tests against the current interpreter
- fuzzing with random compatible schemas and payloads
- per-type calibration self-tests before enabling opaque fast paths
- failure-path tests for partial initialization and cleanup
- Miri coverage for helper-side unsafe code where practical
- sampled profiling before and after each rollout stage

The most important test discipline is:

- same plan
- same bytes
- same output
- same error

between the current interpreter and the new execution engine.

## Inlining Policy

### One Cranelift function per root type

`compile_decode` (`codegen.rs:165`) calls `declare_function` / `define_function` /
`get_finalized_function` once per invocation and emits **all** IR ops for the
entire root type into that single function body (`emit_decode_function`,
`codegen.rs:441`).

There are no per-field, per-struct, or per-variant Cranelift functions. Every
nested struct, enum variant, array element, and `Option<T>` inner decode is
emitted as straight-line Cranelift instructions and branches in the same
function. This means:

- The Cranelift register allocator sees the entire decode in one pass.
- No indirect calls exist for type-level nesting.
- Cross-type reuse is not attempted; two root types that share a sub-struct
  each receive their own copy of the sub-struct decode code.

### Helpers: always out-of-line `call_indirect`

Every runtime helper is a Rust `extern "C"` function registered as a symbol
in `JITBuilder::symbol` (`codegen.rs:132–145`) and called via
`call_indirect` with an `import_signature`. None are inlined.

| Helper | Call site | Condition |
|---|---|---|
| `vox_jit_vec_reserve` | `emit_read_byte_vec:857`, `emit_read_string:965`, `emit_alloc_backing:1336` | Vec/String, non-empty |
| `vox_jit_vec_commit_len` | `emit_commit_list_len:1050`, `emit_alloc_backing:1407`, `emit_read_byte_vec:890`, `emit_read_string:992` | After each element and on completion |
| `vox_jit_utf8_validate` | `emit_read_string:945` | String, non-empty |
| `vox_jit_box_alloc` | `emit_alloc_boxed:1442` | `Box<T>` |
| `init_none` / `init_some` | `emit_decode_option:1200,1228` | `Option<T>` |

SlowPath ops (`DecodeOp::SlowPath`) cause `compile_decode` to return
`Err(CodegenError::UnsupportedOp)` immediately (`codegen.rs:642–648`). The
entire stub is rejected and execution falls back to the IR interpreter; there
is no partial-inline path.

### Hot ops: inline Cranelift instructions, no call-outs

The following are emitted as actual Cranelift IR, not function calls:

- **Scalar reads** — `bool`, `u8`–`u64`, `i8`–`i64`, `f32`, `f64`,
  `usize`/`isize`: varint decode loop unrolled 10 bytes (`read_varint_u64`,
  `codegen.rs:377`), byte loads, bit shifts, stores (`emit_read_scalar`,
  `codegen.rs:668`).
- **Field offset stores** — `iadd_imm(out_ptr, offset)` + `store` via
  `dst_at` (`codegen.rs:337`); zero overhead for offset-0 fields.
- **EOF and varint-overflow guards** — inline `icmp` + `brif` inside
  `read_byte` (`codegen.rs:346`) and `read_varint_u64` (`codegen.rs:377`).
- **Empty container materialization** — `copy_empty_bytes` (`codegen.rs:1485`)
  emits byte-by-byte `load`/`store` pairs at compile time (no call).
- **AllocBacking result pointer** — load from `container_ptr[ptr_offset]` via
  `load(ptr_ty, …)` + arithmetic; no helper needed to locate the data pointer
  (`codegen.rs:1353`).
- **Enum discriminant dispatch** — chain of `icmp` + `brif` comparisons,
  no jump table helper (`emit_branch_on_variant`, `codegen.rs:1063`).
- **PushFrame / PopFrame** — no-ops in the JIT (`codegen.rs:608–615`); field
  offsets are absolute from the root `out_ptr`, baked in at lowering time.

### Element loops

Vec element decode (`emit_alloc_backing`, `codegen.rs:1318`) is a
Cranelift `brif`-based loop header with the element body ops inlined verbatim
into the loop body — `emit_element_body` (`codegen.rs:537`) skips `Return`
and `CommitListLen` ops, which are handled by the loop emitter. The IR body
block is recorded in `inlined_blocks` so the outer `emit_decode_function` loop
does not double-emit it (`codegen.rs:492–510`).

The same inline-body pattern applies to `Option<T>` Some payload
(`emit_decode_option`, `codegen.rs:1231–1246`) and `Box<T>` pointee
(`emit_alloc_boxed`, `codegen.rs:1462–1474`). Fixed arrays are unrolled
up to 64 elements (`emit_decode_array`, `codegen.rs:1260`).

## Current Benchmark Snapshot

On the current tree, the `GnarlyPayload` benchmark in
`rust/vox-jit-tests/benches/decode.rs` shows a real end-to-end win for the JIT
path on a heterogeneous nested payload:

| entries | reflective median | IR-interp median | JIT median | JIT vs reflective | JIT vs IR-interp |
|---|---:|---:|---:|---:|---:|
| 1 | 3.082 us | 3.166 us | 1.332 us | 2.31x | 2.38x |
| 4 | 9.999 us | 10.08 us | 4.707 us | 2.12x | 2.14x |
| 16 | 46.87 us | 36.66 us | 17.85 us | 2.63x | 2.05x |

These medians came from:

```bash
cargo bench -p vox-jit-tests --bench decode -- gnarly
```

Two points matter when interpreting that table:

- `gnarly/jit` is using a real Cranelift stub, not a reflective fallback.
  `compile_decode` rejects any lowered program that still contains
  `DecodeOp::SlowPath`, so a successful JIT bench implies the root program
  compiled without SlowPath ops.
- The remaining gap versus the simpler microbenchmarks therefore should not be
  explained as "some leaves are still reflective". At this stage it is more
  likely a mix of real heterogeneous-workload costs: more helper traffic, more
  branching, more allocations, more UTF-8 checks, and more nested container
  bookkeeping than the pure `Msg` or pure `Vec<u32>` cases.

## Open Questions

- How much UTF-8 validation belongs in generated code versus helpers?
- Should the IR interpreter replace some reflective paths even when JIT is
  disabled?
- What is the smallest useful initial opaque whitelist beyond `Vec<u8>` and
  `String`?
- How much helper surface is acceptable before the JIT stops paying for itself?
- Is there any shape worth specializing for encode before decode is mature?

## Recommendation

Proceed with a staged design:

- keep translation plans as the semantic layer
- introduce a compact execution IR
- keep the reflective interpreter as fallback
- use Cranelift for steady-state decode hot paths
- calibrate a very small whitelist of opaque std types per process
- avoid persistence entirely

The intended outcome is not a new schema system. It is the same schema-aware
runtime, with reflection removed from the hot path.
