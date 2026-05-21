# vox-wire: one binary format

Written down so the cbor/postcard split stops being re-litigated, and
so the `SkipValue` cliff (found below) gets fixed at the root instead
of papered over.

## Thesis

vox uses two binary formats today — `facet-cbor` for the
self-describing parts (handshake, schema payloads) and `vox-postcard`
for everything else. That split is not load-bearing. It should
collapse into **one format, `vox-wire`, with two framing modes** that
share a single value codec:

- **self-describing mode** — structural tags inline, decodable with
  zero prior agreement. Used for the handshake and for transmitting
  schemas. This is the bootstrap fixpoint.
- **compact mode** — a value encoded against a schema both peers
  already hold. No tags. The steady-state RPC path.

Same leaf encoding in both. The only difference is whether the tag
stream is interleaved. One parser, one serializer, one IR, one error
type, one fuzz target — for one format.

And `vox-wire` is **fixed-width little-endian**, with a **native
facet data model**, designed so schema evolution stays on the
compiled fast path instead of falling off it.

## What we have today

- `facet-cbor` (`rust/facet-cbor/`) — self-describing. Used by the
  handshake (`vox-core/src/handshake.rs:54-78`) and for schema
  payloads embedded in messages (`CborPayload`, `vox-schema`
  `to_cbor`/`from_cbor`).
- `vox-postcard` (`rust/vox-postcard/`) — non-self-describing. Used
  for every RPC message. It has `serialize`, `deserialize`, `plan`
  (translation plans), `ir` (4358 LOC), `raw`, `scatter`, plus a JIT
  backend in `vox-jit`.

`vox-postcard` is **already not postcard**. Translation plans, an IR,
JIT codegen, opaque payload passthrough — none of that is in the
postcard spec. We pay the *constraints* of the postcard wire spec
(varint encoding, the ~29 serde data types) for a format we have
already forked. That is the worst trade: spec limitations, none of
the spec benefits — there is no interop, no shared tooling, nobody
else reads our frames.

Where the split hurts:

- **Two codecs, two of everything.** Two serializers, two
  deserializers, two error types, two test/fuzz surfaces. Every
  facet datatype edge case gets litigated twice.
- **The format boundary is a physical seam.** A `Message` is
  postcard, but `RequestCall.schemas` inside it is a `CborPayload` —
  opaque bytes decoded in a *second pass* with a *different codec*.
  Decoding one logical message touches two formats.
- **Two unrelated answers to one question.** "How do I decode
  something whose schema I don't fully share?" CBOR answers it by
  being self-describing forever; postcard answers it with a
  `TranslationPlan` built from handshake-exchanged schemas. Same
  problem, two machines.

## The cliff: SkipValue falls off the JIT

This is the finding that triggered the pivot. `vox-jit/src/codegen.rs:1364`:

```rust
DecodeOp::SkipValue { .. } => {
    return Err(CodegenError::UnsupportedOp(
        "SkipValue — fall back to IR interpreter for skip ops".into(),
    ));
}
```

Unconditional bail. One `SkipValue` op anywhere in a decode program
makes `compile_decode` return `Err`, and the **whole message** then
decodes on the IR interpreter — not just that field.

The asymmetry is the dangerous part. Look at what emits each op:

- `SkipValue` (`vox-postcard/src/ir.rs:1333`) — emitted for a
  *remote* field with no local counterpart. "The peer is newer than
  me."
- `WriteDefault` (`vox-postcard/src/ir.rs:1346`) — emitted for a
  *local* field with no remote counterpart. "I am newer than the
  peer." `WriteDefault` **is** JIT-compilable; it is not in the
  unsupported list.

So evolution is asymmetric on the fast path. If *you* added a field,
you JIT fine. If the *peer* added a field, every message from them
falls off the JIT onto the interpreter. In a rolling deploy the
lagging side silently interprets every message from every upgraded
peer — invisible except as latency. Pure field reorder is fine
(`FieldOp::Read` just retargets indices); it is specifically
remote-only fields that detonate.

Why skip is hard to JIT today: skipping a *postcard* value means
recursively parsing it, because varint lengths are unknowable ahead
of time. Emitting that recursive walk as Cranelift IR is essentially
reimplementing the interpreter inside codegen, so it was punted — a
reasonable punt, with an invisible cost.

`vox-wire` removes the punt's cause. See "Length-prefixed
aggregates" below.

## Self-describing is the fixpoint

A correction to an earlier wrong turn: you cannot drop
self-description and bootstrap everything from a compiled-in
constant. vox does **meta-schema evolution** — the `Schema` type
itself evolves — so peers can disagree on what `Schema` even *is*.
`TRANSPORT_VERSION` in `vox-core/src/transport_prologue.rs`
hard-rejecting on mismatch defeats that entirely and must go.

So self-describing mode is load-bearing: it is the one layer that
decodes with **zero** prior agreement. It is a genuine fixpoint, no
regress, because self-describing mode needs no schema *by
construction* — every value carries its own structural tag.

The only thing frozen-forever is the **tag vocabulary**: a small
fixed table of major types (scalar widths, list, map, struct,
enum-variant, option, …) sized to facet's `Def` / `ScalarType`.
~20 entries. That table is the eternal contract. Everything else —
`Schema`, `HandshakeMessage`, every user type — evolves.

"What format is the schema sent in?" — the self-describing mode of
`vox-wire`. That is the whole answer.

Meta-schema evolution then falls out: a peer sends its schemas in
self-describing mode → the receiver decodes them to a generic
`Value` tree with *no* schema → then tolerantly deserializes that
`Value` into its *own* `Schema` type (extra fields ignored, missing
ones defaulted — facet already does this). The same machinery
reconciles `HandshakeMessage`. No version byte gates anything; the
handshake is itself evolvable.

This keeps the self-describing / compact split that cbor/postcard
draw today. We just stop paying for it with two codecs.

## Wire choices

### Fixed-width little-endian, no varint

Postcard varint-encodes integers and lengths. Varints are right for
embedded; vox does not target embedded. For vox they are *actively*
wrong on three axes:

- **Zero-copy.** A varint length cannot be read without decoding it;
  you cannot point into the buffer first. Fixed-width LE means a
  length is a single load. vox has `r[impl zerocopy.framing.*]`
  rules everywhere.
- **JIT quality.** A varint is a branchy loop; a fixed-width integer
  is one load. Straight-line codegen instead of emitted loops.
- **Compression.** Varints pack bits → high entropy per byte →
  compressors hate them. Fixed-width LE integers are full of zero
  bytes → compressors love them. Varint is *anti-synergistic* with
  the compression we want to add (below).

So `vox-wire` integers and lengths are fixed-width little-endian.
Enum discriminants too.

### Native facet data model

Postcard's wire spec was designed around serde's ~29 data types. A
custom format is not bound by that. `vox-wire` encodes facet's `Def`
/ `ScalarType` directly, with canonical encodings for things serde
has no slot for: `Decimal128`, `Date` / `Time` / `DateTime` /
`Duration` as first-class scalars, n-dimensional arrays with a shape
header, `Set` distinct from `List`, `DynamicValue`, maps with
arbitrary (non-string) keys. No lossy mapping.

### Length-prefixed aggregates

This is the fix for the `SkipValue` cliff. Give every struct and
enum *field* a fixed-width length prefix. Then `SkipValue` stops
being a recursive structural walk and becomes `ptr += read_u32()` —
two instructions, trivially JIT-able. With that, the JIT compiles
skip ops, and schema evolution never falls off the fast path.

Scoped: scalars skip free (fixed-width, known size); strings and
lists skip free (length is already inherent); only struct and enum
fields need the *added* prefix. ~4 bytes each — free on localhost,
near-zero after compression on the wire (length prefixes are highly
repetitive).

Length-prefixing is therefore not a framing-robustness nicety. It is
the mechanism that keeps schema evolution compiled. That is its
justification.

## Schema evolution

Evolution stays plan-based, **by field name**, exactly as today:

- **No protobuf-style field ids. No `#[facet(id = N)]`.** Firm. Field
  identity is the field *name*, carried in the schema. The schema is
  exchanged once; the `TranslationPlan` reconciles remote↔local by
  name. This is what `vox-postcard/src/plan.rs` already does and it
  is right.
- **The plan never survives to decode time.** On native it is
  consumed when the decoder is built — `prepare_decoder`
  (`vox-jit/src/lib.rs:93`) lowers `plan → IR → machine code` once at
  conduit construction. Every `recv` after is a bare fn-ptr call.
  Keep it that way; never interpret a plan per message on native.
- **The JIT must compile every evolution op** — skip, default-fill,
  reorder. With fixed-width + length-prefixed framing all three are
  cheap to emit. Then there is no "fall back to interpreter" path
  for evolved schemas, and no perf cliff.

memcpy of a struct from wire to memory is *not* a headline benefit:
it only exists when remote schema == local schema and the struct is
all fixed-width POD. It is just the degenerate code the compiler
emits in that case. Do not design around it. Design around "every
(remote, local) pair compiles to straight-line code"; memcpy is
what that happens to be at the identity end of the range.

## One IR, two executors

There are three decode engines today, gated by `CodecMode` in
`vox-jit/src/lib.rs:281` (`try_decode_owned`):

- **Jit** — `plan → IR → Cranelift → machine code`. Native default.
- **Interp** — `from_slice_ir`, `plan → IR → interpreted`. Shares
  lowering with the JIT.
- **Reflect** — `from_slice_with_plan`
  (`vox-postcard/src/deserialize.rs:21`), a reflective plan-walker,
  no IR.

The IR *type* lives in `vox-postcard/src/ir.rs`, but the lowering and
`from_slice_ir` live in `vox-jit`, which does not build for wasm
(Cranelift). So **wasm cannot reach the IR at all** — its decode path
runs `from_slice_with_plan`, the reflective walker. Native and wasm
run different implementations of the same wire semantics, which can
silently diverge. Encode has the same split (`serialize.rs`
reflective vs. JIT).

Target for `vox-wire`:

1. **One IR.** Move IR lowering and the IR interpreter out of
   `vox-jit` into the format crate so they build for wasm.
2. **Two executors of that IR.** The portable IR interpreter (wasm,
   plus native's can't-compile fallback) and the Cranelift JIT
   (native accelerator). Same lowering, same semantics, everywhere.
3. **Demote the reflective `from_slice_with_plan` to a CI-only
   oracle.** It is valuable *as* an oracle precisely because it does
   not share lowering with the JIT/interpreter — a lowering bug
   cannot fool all three. It should not be a shipped code path.

## Compression

None today. Add it, negotiated:

- Negotiated at the transport prologue (there is already a `mode`
  byte, currently only `Bare`) or in `connection_settings`.
- Local / in-process / loopback → **off**. Bandwidth there is
  effectively infinite; compression is pure CPU cost.
- Remote → **on**. Streaming zstd (or brotli) over the link byte
  stream, *below* framing — not per-message. RPC messages are small;
  per-message zstd-q1 is mostly overhead, but a streaming compressor
  learns the recurring structure (method ids, field layouts, common
  strings) fast. Optionally seed a dictionary from the schema
  exchanged at handshake.
- Fixed-width framing + streaming compression is synergistic: the
  uncompressed local path is fast, and the compressed remote path
  compresses *better* than varint postcard would have.
- Browser caveat: `CompressionStream` gives gzip/deflate natively and
  Brotli decode in recent browsers, but Brotli *encode* in wasm is
  heavy. A small symmetric zstd-wasm is probably the pragmatic pick.
  Decide this early — it is the one external constraint.

## What dies, what stays

**Dies:**

- The `facet-cbor` crate.
- `CborPayload` and the two-pass decode.
- `to_cbor` / `from_cbor` in `vox-schema`.
- `from_slice_with_plan` as a *shipped* path (kept as a CI oracle).
- `TRANSPORT_VERSION` hard-reject — replaced by schema-reconciliation
  negotiation.
- The per-wire-field "which format goes here?" decision.

**Stays / ported:**

- The meta-schema bootstrap — becomes load-bearing.
- `TranslationPlan` / `build_plan` — format-agnostic, reconciles by
  name. Consumed only at decoder-build time.
- The IR (`vox-postcard/src/ir.rs`) — moves to the format crate,
  retargeted to fixed-width.
- The JIT — retargets to the new IR, gets simpler and faster
  (fixed-width = straight-line codegen).
- `vox-schema`'s `Schema` model — basis for the self-describing tag
  set and the bootstrap.
- The opaque `Payload` mechanism.

It is a wire break — a coordinated protocol bump. That is acceptable;
vox values small code over backwards compatibility, and the browser
cannot introspect the current wire anyway, so nothing observable is
lost. It is evolution of `vox-postcard` into `vox-wire` (drop the
postcard-spec constraints, go fixed-width, fold in self-describing
mode, delete `facet-cbor`), not a from-scratch codec.

## Anti-patterns

- **Protobuf-style field ids / `#[facet(id = N)]`.** Field identity
  is the name, in the schema. Do not reintroduce numeric ids in any
  disguise, including name-hashes.
- **Interpreting a `TranslationPlan` per message on native.** The
  plan is a decoder-build-time input. If it reaches the hot path,
  that is the bug.
- **Shipping two divergent codec implementations** (reflective on
  wasm, IR/JIT on native). One IR, two executors of it.
- **A `TRANSPORT_VERSION` that hard-rejects on mismatch.** The
  handshake must be evolvable; negotiate via schema reconciliation.
- **Keeping varint "to save bytes."** Remote traffic is compressed
  (varint compresses *worse*); local traffic does not care. Varint
  only costs us — zero-copy, JIT quality, compression ratio.
- **A separate self-describing codec.** Self-describing is a *mode*
  of `vox-wire`, sharing its leaf encoding — not another crate.

## Open questions

- Length-prefix width — `u32` is simplest; `u16` is enough for most
  aggregates but caps them. Probably `u32`, uniform.
- Prefix *all* struct/enum fields, or only those that could ever be
  skip targets? The encoder cannot know at encode time which fields a
  future peer will lack, so in practice: prefix all struct/enum
  fields.
- Compression library for the browser side — zstd-wasm vs. relying on
  `CompressionStream`. Drives the native choice too (keep it
  symmetric).
- Meta-schema evolution corner: a remote `Schema` carrying a field
  whose *type* the local build does not understand. The generic
  `Value` decode handles it structurally; confirm the tolerant
  deserialize into local `Schema` degrades cleanly.

## Relationship to codec-architecture.md

`notes/codec-architecture.md` is the *codegen* half: codecs are
calibrated layout data + direct stores, no helper-call dispatch,
"`SlowPath` should not exist — the IR is the full path." This note is
the *wire-format* half. They are complementary:

- `SkipValue → UnsupportedOp` is exactly the kind of fallback that
  note wants gone. `vox-wire`'s length-prefixed framing is the
  wire-format change that makes the skip op mechanically compilable,
  so "the IR is the full path" can actually hold.
- Both point at the same IR (`vox-postcard::ir`) and want it
  retargeted: that note to be layout-driven instead of helper-driven,
  this note to be fixed-width instead of varint, and to live in a
  crate wasm can build.
