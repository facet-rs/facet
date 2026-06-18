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
├── (Enum)             variants: *const VariantLayout, variant_count
└── (Opaque)           opaque_handle: u32 (into the calibration registry)

FieldLayout            // one struct field, or one piece of a variant payload
├── name: LayoutBytes (utf8 ptr+len)
├── offset             // absolute, within the enclosing value
└── layout: *const ValueLayout (recursive)

VariantLayout
├── name
├── match_pattern      // bytes that must match for this variant to apply
├── store_pattern      // bytes to store when constructing this variant
└── fields, field_count
```

Variable-length backing storage (variant arrays, field arrays, name
bytes, nested layouts, match/store patterns) is owned by a
`LayoutArena` and freed when the arena is dropped. For process-wide
layouts you leak the arena.

### Why variants carry patterns, not a shared tag location

A first sketch had a single `tag_offset` + `tag_width` on the enum
plus a per-variant `tag_value`. That works for explicitly-tagged enums
(`#[repr(u8)] enum E { … }`) but **falls apart** the moment niche
optimization is in play:

- `Option<Box<T>>`, `Option<&T>`, `Option<NonZeroU32>`: rustc encodes
  `None` as a payload-bit pattern that's invalid for `Some`. There is
  no separate discriminant byte; the "tag" is "are bytes 0..8 of the
  payload all zero?".
- `Option<Option<NonZeroU32>>`: chained niche-filling. Multiple
  variants share overlapping bit ranges.
- Swift's spare-bit tagging: the discriminant lives in unused bits of
  the payload pointer / value, not in a separate field.
- `Option<bool>`: rustc may pick the niche `2` for `None` since `bool`
  only uses `0`/`1`. The discriminant byte *is* the payload byte.

So each variant needs to carry, on its own:

- `match_pattern` — a list of `(offset, expected_byte, mask)` triples
  that must all hold for this variant to be matched. For an
  explicitly-tagged enum this is a single triple
  `(tag_offset, tag_value, 0xFF)`. For a niche-filled `None` it's the
  bytes the payload must be entirely zero across. The last variant in
  an enum may carry an empty pattern (default / catch-all) to handle
  "anything that didn't match the niches above."
- `store_pattern` — a list of `(offset, value, mask)` triples to write
  when constructing this variant. For tagged enums this writes the
  discriminant. For niche-filled variants this might be empty (the
  payload write does the work) or might zero out a niche region.

A regular `#[repr(u8)] enum { Ok = 0, Err = 1 }` becomes:
- `Ok`: match `[(tag_offset, 0, 0xFF)]`, store `[(tag_offset, 0, 0xFF)]`
- `Err`: match `[(tag_offset, 1, 0xFF)]`, store `[(tag_offset, 1, 0xFF)]`

`Option<Box<T>>` (8-byte pointer payload, niche = all-zero) becomes:
- `None`: match every byte in `0..8` is zero — eight `(i, 0, 0xFF)`
  triples. Store: zero those eight bytes.
- `Some`: empty `match_pattern` (default; it's whatever isn't `None`).
  Empty `store_pattern` (the payload write places a non-null pointer
  there, which is what makes it `Some`).

The codegen reads the patterns and emits tests / stores. A dispatcher
on decode is "for each variant in order, check `match_pattern`; first
one whose pattern holds wins" — the codegen lowers it to a sequence of
loads-and-compares (or a single load-and-table-lookup when the
patterns are all single-byte at the same offset).

The match/store patterns are produced by the calibrator from real
samples; they don't have to be inferred by hand. The calibrator
enumerates the variants, asks the runtime to construct each, and
records every byte that's set / cleared / equal-across-samples.

For Swift, the calibration uses the same byte-comparison scheme: have
`inject` build canonical samples, diff them, fill in the patterns.
Niche-filled Swift enums are no harder to calibrate than
niche-filled Rust enums; the difference is only in *how* the samples
get built.

## Calibration

Calibration **produces** a `ValueLayout` from a backend-specific source
of truth. It runs once per type and is not on any hot path.

### Rust calibration

For each variant, the calibrator constructs canonical samples and
records what's the same / what differs / what's set / what's cleared.

For `Result<u64, ()>`:
- Build `Ok(0)`, `Ok(0xDEAD…)`, `Err(())` directly.
- Bytes that differ between `Ok(0)` and `Ok(0xDEAD…)` mark the Ok
  payload range — that gives the Ok payload offset.
- Bytes outside that range that differ between `Ok(_)` and `Err(_)`
  give us the discriminant location *for this enum*. For an
  explicitly-tagged enum that's a single byte; the value seen at that
  byte in each sample becomes that variant's `match_pattern` entry, and
  the same byte becomes its `store_pattern` entry.

For `Option<Box<T>>` (niche-filled):
- Build `None` and `Some(Box::new(…))`.
- The 8 bytes of `None` are all zero; the 8 bytes of `Some(_)` have at
  least one non-zero byte (the heap pointer).
- `None`'s `match_pattern` is "all 8 bytes are zero." Its
  `store_pattern` is "write 8 zero bytes."
- `Some`'s `match_pattern` is empty (default / catch-all). Its
  `store_pattern` is empty (the payload's pointer write produces the
  non-null bytes that make it `Some`).

Both shapes drop out of the same byte-diffing procedure. The
calibrator doesn't need to know about niche optimization specifically;
it just records what changes when each variant is constructed.

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

**Init `Result<u64, _>::Ok(31)` (explicit tag):**
1. For each `(offset, value, mask)` in `Ok.store_pattern`, emit a
   masked byte store at `dst + offset`. For an `#[repr(u8)]` Result
   that's one store of the discriminant byte.
2. For each field in `Ok.fields`, emit a store at `dst + field.offset`
   with the payload value (here, a u64 store of `31`).

That's two `mov`s for `Result<u64, ()>`. No `call_indirect`, no
`init_ok_fn`, no `inject`.

**Init `Some(Box::new(value))` (niche-filled Option):**
1. `Some.store_pattern` is empty — nothing to emit for the discriminant.
2. Emit a store of the box pointer at `Some.fields[0].offset`. The
   non-null pointer write *is* what makes it `Some`.

**Init `None: Option<Box<T>>` (niche-filled Option):**
1. `None.store_pattern` is "8 zero bytes at offsets 0..8" — emit a
   memset (or a single 8-byte zero store) to clear the payload region.
2. No fields to emit.

**Decode an enum from postcard:**
1. Read a varint discriminant from the input (this is the *wire*
   discriminant, not the in-memory one).
2. Map the wire discriminant to a local variant index (translation
   plan, computed once per shape pair).
3. Branch to per-variant decode block.
4. In each block, emit `match_pattern.store_pattern`'s stores (writes
   any in-memory discriminant bytes that this variant requires) and
   then emit each field's decode at the field's calibrated `offset`.

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

## Scope: Swift first, Rust later

The Rust JIT works today and has a meaningful test suite covering it.
Refactoring its IR (`vox-postcard::ir`) and Cranelift codegen to drop
the helper-call ops in favor of layout-driven ops is a substantial
undertaking — touching ~10k lines, several dozen tests, and the
`facet`-coupled lowering — and there's no reason to do it before the
new architecture is proven somewhere.

So we develop the new architecture **on the Swift side**, where there
is no existing implementation to be careful with, and we leave the
Rust JIT alone until the architecture has settled. Once Swift's codec
encodes/decodes the realistic shapes (enums with niches, structs,
nested types, opaque containers, recursive types, …) using only
calibrated layouts and direct stores, **then** we open a separate
effort to migrate the Rust JIT off its helper-call IR onto the same
machinery.

Concretely:

- `vox-postcard::ir`, `vox-jit::codegen`, the `Decode*`/`Encode*` ops
  that special-case Option/Result/etc. — left untouched.
- `vox-jit-cal::value_layout` — primary development target. Grows the
  per-variant match/store patterns, niche calibration, struct/nested
  support.
- `vox-swift-abi` — primary consumer. Calibration entry points, layout
  registry. No hot-path helpers.
- Swift side (`vox-runtime`) — primary user. Builds layouts via the
  calibration entry points, then encodes/decodes Swift values from
  those layouts directly in Swift code.

The Rust JIT remains a useful sanity check: when our calibrator
produces a `ValueLayout` for `Result<u64, ()>`, we can cross-check
that the layout's offsets/match-patterns match what `rustc` actually
laid out, by reading bytes of native Rust values. That doesn't require
modifying the JIT — just reading bytes.

## First milestone

Concrete, small, and actually validated:

> Given a `ValueLayout` for `Result<u64, _>`, write a fresh `Ok(31)`
> with two byte stores (one for the discriminant, one for the u64),
> using only the integers in the layout.

Done for Rust in `vox-jit-cal/src/value_layout.rs::tests`. **Note**:
the current code uses the over-simplified `tag_offset` / `tag_width` /
`tag_value` fields described in the *first sketch* above. That works
for the simple-tag case; it needs to be replaced with the per-variant
`match_pattern` / `store_pattern` representation before it can handle
RustNPO, niche-filled `Option<Box<T>>` / `Option<NonZero>`, Swift's
spare-bit-tagged enums, etc. Treat the existing code as a load-bearing
proof-of-concept, not the final shape.

The next steps, in order:

1. Swift parallel of the simple-tag case: define `enum Foo { case
   ok(UInt64); case err }`, calibrate via `inject`, get a
   `ValueLayout`, and have **Swift code** (not a Rust helper called
   from Swift) write the bytes for `.ok(31)` from the layout's
   integers.
2. Replace `tag_offset`/`tag_width`/`tag_value` with per-variant
   `match_pattern` / `store_pattern`. Re-run the simple-tag tests to
   confirm equivalence.
3. Extend the Rust calibrator to produce niche-filled patterns
   (`Option<Box<T>>`, `Option<&T>`, `Option<NonZero>`); add tests.
4. Same for Swift.
5. Replace the helper-call IR ops (`DecodeOption`, `DecodeResult`,
   `DecodeResultInit`, `EncodeOption`, `EncodeResult`) with one
   `DecodeEnum` / `EncodeEnum` driven by the new layout. Cranelift
   codegen emits the patterns directly; the special-cased ops go
   away.
6. Same exercise for structs (calibrate offsets, emit field stores)
   and nested types — both already most of the way there.
