# Codec architecture

How the vox encoders / decoders are supposed to work, written down so I
stop reproducing the wrong thing.

## Thesis

A codec is **calibrated layout data + native codegen**, not **dispatch
through helper functions**.

For each value type (`Result<u64, ()>`, a Swift `enum Foo`, a struct, a
primitive — anything), we produce one piece of data — a `ValueLayout` —
that describes:

- size and alignment
- field offsets (for structs and enum-variant payloads)
- discriminant offset, width, and per-variant tag values (for enums)
- the layout of nested types, recursively

Each backend's codegen reads that data and **emits direct byte writes**
for its own values. The Rust JIT emits Cranelift code that stores into
Rust memory. The Swift JIT (when it exists) emits whatever Swift's
equivalent mechanism is, against Swift memory. Neither one calls a
helper function to "init an Ok variant" or "write a discriminant" on the
hot path. They store bytes.

That's it. There is no "interpreter that Swift calls into Rust" or "JIT
that calls into per-type Rust shims." Each language's hot path is
self-contained and native to that language.

## Why this exists

The Rust JIT already does the right thing in places: opaque containers
(`Vec`, `String`, `Box<T>`, `Box<[T]>`) go through `vox-jit-cal`, which
probes them once for the (`ptr`, `len`, `cap`) slot offsets and the
empty-constructor bytes, and the JIT then reads/writes those slots with
direct loads/stores at calibrated offsets. That is the pattern the
whole codec should use — calibration produces numbers, codegen turns
the numbers into instructions, no per-shape function calls.

Where it goes wrong is enums, options, and results. Those reach for
facet's vtable functions (`init_ok`, `init_err`, `is_some`,
`get_value`, …) and the JIT ends up emitting `call_indirect` to a
non-inlineable helper for every variant manipulation. That isn't real
codegen, it's dispatch elimination — the work is still done inside the
helper that the JIT cannot see into. The special-cased IR ops
(`DecodeResult`, `DecodeOption`, `DecodeResultInit`, `EncodeOption`,
`EncodeResult`) exist only because each helper needed somewhere to be
called from; once the calibration gives us tag offsets / widths /
values directly, those ops collapse into one general `DecodeEnum` /
`EncodeEnum`.

The fix, then, isn't "switch to a different architecture." It's
"extend the calibrate-and-emit-stores pattern to enums, options, and
results, the same way it already covers Vec/String/Box." The Swift
backend should adopt that pattern from the start instead of inheriting
the helper-call shortcut.

## ValueLayout

Lives in `vox-jit-cal::value_layout`. `#[repr(C)]`, FFI-stable, no
Rust-only types in its surface. The same struct graph is built by
the Rust calibrator and the Swift calibrator, and consumed by the
Rust JIT codegen and (eventually) the Swift JIT codegen.

```
ValueLayout            // tagged-struct: kind ∈ {Primitive, Struct, Enum, Opaque}
├── size, align
├── (Primitive)        primitive_kind: PrimitiveKind
├── (Struct)           fields: *const FieldLayout, field_count
├── (Enum)             tag_offset, tag_width, variants: *const VariantLayout
└── (Opaque)           opaque_handle: u32 (into the calibration registry)

FieldLayout            // one struct field, or one piece of a variant payload
├── name: LayoutBytes (utf8 ptr+len)
├── offset             // absolute, within the enclosing value
└── layout: *const ValueLayout (recursive)

VariantLayout
├── name
├── tag_value          // numeric value to write at tag_offset
└── fields, field_count
```

Variable-length backing storage (variant arrays, field arrays, name
bytes, nested layouts) is owned by a `LayoutArena` and freed when the
arena is dropped. For process-wide layouts you leak the arena.

## Calibration

Calibration **produces** a `ValueLayout` from a backend-specific source
of truth. It runs once per type and is not on any hot path.

### Rust calibration

For `Result<u64, ()>`:
- Construct `Ok(0)`, `Ok(0xDEAD…)`, `Err(())` directly.
- Read the bytes of each.
- Bytes that differ between `Ok(0)` and `Ok(0xDEAD…)` mark the Ok payload
  range — that gives the Ok payload offset.
- Bytes outside that range that differ between `Ok(_)` and `Err(_)` mark
  the discriminant location — that gives `tag_offset`. The values at
  `tag_offset` give `tag_value` for each variant.
- Everything else (size, align, field layout for `u64`) is known from
  the type system / facet metadata.

Facet's vtable functions (`init_ok`, `init_err`, `is_ok`, `get_ok`, …)
exist and are correct, but **we use them only at calibration time**, if
at all, to construct sample variants for probing. They are never called
on the hot path. Once the probe is done, we have integers; the codegen
emits stores.

For struct types, no probing is needed: facet already knows field
offsets. Just translate them into `FieldLayout`s.

For opaque containers (`Vec<T>`, `String`, `Box<T>`), calibration finds
slot offsets (`ptr_offset`, `len_offset`, `cap_offset`) and the empty
constructor bytes. Emitted code reads/writes those slots directly.

### Swift calibration

Swift hides enum layout behind value-witness functions. Each Swift enum
type has three relevant witnesses:

- `tag(value)` — return the variant index of an existing value.
- `project(value, idx, visitor)` — call the visitor with field pointers.
- `inject(dst, idx, fields, count)` — write a fresh enum value into
  `dst` for variant `idx` with the given payload pointers.

The Swift calibrator uses `inject` exactly the way the Rust calibrator
uses construction:
- Build two samples of variant A (`inject(dst, 0, [&u64_zero], 1)` and
  `inject(dst, 0, [&u64_max], 1)`).
- Build a sample of variant B.
- Byte-compare to find tag offset, tag values, payload offsets.

After calibration, the Swift codegen never calls `inject` again. It
stores bytes directly.

Niche-filled / spare-bit Swift enums (where there is no separate tag
region) won't fall out of byte comparison cleanly. Those need a
calibration variant that records "the discriminant is encoded in these
specific payload bits"; they're rare and an extension of the same data
structure.

## Codegen

A backend codegen takes a `ValueLayout` and produces native code that
reads or writes values of that layout. Examples:

**Init `Result<u64, _>::Ok(31)`:**
1. Read the layout's `tag_offset`, `tag_width`, and the `tag_value` for
   the `Ok` variant.
2. Emit a store of width `tag_width` to `dst + tag_offset` with the
   `tag_value` constant.
3. Read the `Ok` variant's first field's `offset`. Emit a u64 store to
   `dst + offset` with the payload value.

That's two `mov`s. No `call_indirect`. No `init_ok_fn`. No `inject`.

**Decode an enum from postcard:**
1. Read a varint discriminant from the input.
2. Map the wire discriminant to a local variant index (translation
   plan, computed once per shape pair).
3. Branch to per-variant decode block.
4. In each block, store the local `tag_value` at `tag_offset` and decode
   each field at its calibrated `offset`.

Same shape on the Rust side and the Swift side. The Rust JIT emits this
as Cranelift; the Swift backend emits its Swift equivalent. Neither one
calls into the other for the work.

## What crosses FFI

The `vox-swift-abi` cdylib exposes:

- **Layout description data** — `ValueLayout` graphs (`#[repr(C)]`),
  arena handles. Swift can read these directly via mirror structs.
- **Calibration setup** — a probe entrypoint that takes pre-injected
  sample buffers (Swift side built them via `inject`) and returns a
  calibrated `ValueLayout`. Calibration-time only.
- **Codec lifecycle** — prepare/release for a method's codec, which
  bundles the layout(s) for that method's request/response with the
  remote postcard schema.

It does **not** expose:

- "Write a discriminant byte" — that's a single store, emitted by
  whoever has the layout and the destination pointer. The Swift side
  has both; it stores the byte itself.
- "Write a payload field" — same.
- "Init Some / init Ok / init this enum variant" — those are just (tag
  store + each-field store) sequences that the codegen emits inline.
- Anything else that's a one-instruction operation hiding behind a
  function call.

If a function on the FFI surface starts to look like "do this small
operation on a Swift value at runtime," it's wrong. The layout has the
numbers; the code has the pointer; the store is one instruction. Don't
add a function for it.

## IR

The shared IR (currently `vox-postcard::ir::DecodeOp` /
`vox-postcard::ir::EncodeOp`) needs to be rewritten so its operations
are layout-driven, not helper-driven. The current shape has special
cases like `DecodeResult`, `DecodeResultInit`, `DecodeOption`,
`EncodeOption`, `EncodeResult` because the helper-call approach forced
type-specific dispatch. Once the codegen stores bytes directly:

- `DecodeOption`, `DecodeResult`, `DecodeResultInit` → one
  `DecodeEnum { layout, body_blocks }` op. The op's body is a per-variant
  basic block that stores the local tag value, then decodes each field.
- `EncodeOption`, `EncodeResult` → one `EncodeEnum { layout, body_blocks }`
  op that reads the variant index from the source value (using
  `tag_offset`/`tag_width` from the layout, no `is_some_fn`/`is_ok_fn`
  call), branches, and writes the variant.
- `WriteDefault` (currently calls a facet vtable) → either materialize
  default bytes calibrated at lowering time, or emit per-field default
  stores from the layout.
- `ReadOpaque` and the various `WriteShape`/`WriteOpaque`/`WriteProxy` →
  driven by an `OpaqueDescriptor` (the existing calibrated container
  data) plus `ValueLayout` for the element type, not by `&'static Shape`
  + facet vtables.
- `SlowPath` — should not exist after this refactor. The IR is the
  full path; if a shape can't be expressed in the IR, the calibrator
  rejects it before codegen runs.

`&'static Shape` should not appear in any IR op. That parameter exists
only because the helpers need shape lookups; remove the helpers and
the shape references go too.

## Anti-patterns

These keep being tempting; they are wrong:

- **Adding a Rust function `vox_swift_*` that performs a one-instruction
  operation on Swift values.** The Swift code can do it itself, given
  the layout. Adding a function for it reproduces the helper-call JIT
  we're trying to replace.

- **Turning the IR generic over a "backend trait" with associated
  types for `DefaultInitFn`, `OptionIsSomeFn`, etc.** That assumes the
  IR will keep calling those functions, which is the thing we're
  removing. Layout data + direct stores doesn't need a trait; it needs
  more numbers in the data structure.

- **Type-erasing facet's vtable functions to `*const c_void` and
  threading them through the IR.** Same trap, dressed in raw pointers.
  The functions shouldn't be in the IR at all.

- **Per-shape IR ops that exist to dispatch to a per-shape helper.**
  `DecodeResult` / `DecodeResultInit` / `DecodeOption` / `EncodeResult`
  / `EncodeOption` are in the IR today only because each one had a
  facet vtable to call. Replace them with one general `DecodeEnum` /
  `EncodeEnum` driven by the calibrated `EnumLayout`. (For contrast:
  `ReadString` / `ReadFixedVec` / `AllocBacking` are calibrated already
  — they read/write Vec/String slots at offsets `OpaqueDescriptor`
  recorded — so they're examples of the pattern we want, not problems
  to remove.)

- **Cross-FFI hot-path calls between Rust and Swift codegen.** Each
  language compiles its own paths against its own values. The only
  thing they share is the layout description.

## First milestone

Concrete, small, and actually validated:

> Given a `ValueLayout` for `Result<u64, _>`, write a fresh `Ok(31)`
> with two byte stores (one for the discriminant, one for the u64),
> using only the integers in the layout.

Done for Rust in `vox-jit-cal/src/value_layout.rs::tests`. The next
step is the Swift parallel: define `enum Foo { case ok(UInt64); case
err }`, calibrate via `inject`, get back the same `ValueLayout` shape,
and have **Swift code** (not a Rust helper called from Swift) write
the bytes for `.ok(31)` from the layout.

After that, the same pattern extends outward — multi-byte discriminants,
non-ZST payload, niche-filled enums, structs, nested types, opaque
containers — by adding more numbers to the data structure, not more
helper functions.
