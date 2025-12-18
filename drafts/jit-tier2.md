# Tier‑2 Format JIT (Format‑Directed JIT) — Specification

This document is the **normative contract** for Tier‑2 (“format JIT”) deserialization in `facet-format`.

Tier‑2 exists to let format crates (e.g. `facet-format-json`) generate Cranelift IR that **parses bytes directly**, bypassing the `ParseEvent` / token / scanner layers used by Tier‑1 and reflection. The goal is serde‑competitive performance without making `facet-format` format‑specific.

Related (non‑normative) documents in `drafts/`:
- `drafts/facet-format-format-jit-motivation.md`
- `drafts/facet-format-format-jit-reference-design.md`
- `drafts/facet-format-format-jit-implementation-plan.md`

## Status / Scope

**In scope (Tier‑2 v1):**
- Input is a complete in‑memory byte slice (`&[u8]`) and a cursor (`pos`).
- `facet-format` owns *shape traversal* and memory layout.
- Format crates own *surface syntax / binary encoding parsing*.
- Explicit fallback path: Tier‑2 → Tier‑1 → reflection.

**Out of scope (Tier‑2 v1):**
- Streaming / partial buffers / `Read` / `AsyncRead`.
- Guaranteed parity of error message text with the reflection path (but error *positions* must be accurate).
- Full support for every `Facet` shape (start with a small supported subset and expand).

## Terminology (format‑agnostic)

This spec intentionally avoids JSON‑isms like “whitespace” as a global concept.

- **Cursor**: `(input_ptr, len, pos)` where `pos` is the current byte offset.
- **Value**: the semantic item being deserialized (scalar, sequence, map, etc.).
- **Container**: a sequence (“list/array”) or map (“object/dict”).
- **Trivia**: bytes that may appear between meaningful tokens *for a particular format* and do not change semantics.  
  Examples: JSON whitespace, YAML comments, TOML comments.  
  **Some formats have no trivia** (e.g. postcard/binary formats).
- **Value boundary**:
  - **Value‑start**: a cursor position at which it is valid to parse a value in the current context.
  - **Value‑end**: a cursor position immediately after consuming exactly one value.
- **Unsupported**: a capability mismatch (“this operation/type/format combination cannot be handled by Tier‑2”), *not* a parse error.

## Key design choice: Model A (“compiler aligns”)

Tier‑2 must support formats that do not have the notion of whitespace/trivia (e.g. postcard). Therefore, the generic Tier‑2 compiler cannot embed assumptions like “pos points to a non‑whitespace byte”.

This spec uses **Model A**:

> The Tier‑2 shape compiler is responsible for requesting “alignment to the next value boundary”, and the format is responsible for implementing that alignment according to its rules (possibly as a no‑op).

This is expressed as a required format operation:

- **`emit_align_to_value`** (aka “align”): advance `cursor.pos` such that the cursor is at a valid **value boundary** for the current context (or at EOF), without changing semantics.

Notes:
- A JSON implementation will typically skip whitespace/comments.
- A postcard implementation will typically be a no‑op.
- An XML implementation may skip ignorable trivia (implementation‑defined; XML Tier‑2 is explicitly not a v1 target).

If the codebase currently calls this operation `emit_skip_ws` or similar, it should be treated as the same conceptual hook; the name is not normative, the **contract** is.

## Trait‑level contract

Tier‑2 is split into:
- `FormatJitParser`: an **opt‑in** extension trait implemented by parsers that can expose a stable slice + cursor.
- `JitFormat`: a trait implemented by format crates providing Tier‑2 IR emission.

### `FormatJitParser` (parser capability boundary)

`FormatJitParser` implementations MUST satisfy:

1. **Full input slice availability**
   - `jit_input()` returns the full slice used by Tier‑2.
   - If the parser instance cannot provide the complete slice, it MUST disable Tier‑2 (by returning `None` from `jit_pos()` or a similar Tier‑2 entry guard, depending on the API).

2. **Stable cursor availability**
   - `jit_pos()` returns an absolute cursor position into `jit_input()`.
   - If the parser has buffered/peeked state that makes the “current position” ambiguous, `jit_pos()` MUST return `None` (Tier‑2 unavailable).

3. **State coherence on commit**
   - `jit_set_pos(pos)` commits the new cursor position and MUST reset/invalidate internal parser state such that subsequent non‑Tier‑2 parsing continues correctly from `pos`.

4. **Error mapping**
   - `jit_error(input, error_pos, error_code)` maps Tier‑2 errors into the parser’s native error type.
   - This is allowed to be slow; it is on the cold path.

### `JitFormat` (format IR emission contract)

A format’s `JitFormat` implementation MUST:

- Produce Cranelift IR that reads from `cursor.input_ptr` within bounds `[0, cursor.len]`.
- Update `cursor.pos` precisely on success.
- Return `error_code == 0` on success.
- Return a non‑zero `error_code` on error.
- Reserve one specific error code (see **Unsupported**) to mean “Tier‑2 unsupported”, not “parse error”.

The format MAY:
- Emit all parsing logic inline as IR.
- Or use helper functions (Rust functions registered into the JIT module) and emit calls to them.

The Tier‑2 compiler MUST NOT require helper symbols if the relevant operations are implemented purely inline.

## Error model

Tier‑2 uses a single error channel with a reserved code:

- `0`: success
- `FACET_T2_UNSUPPORTED` (reserved): unsupported/capability mismatch → **trigger fallback**, not a parse error
- any other non‑zero value: parse error (format‑defined codes)

### Reserved constant value

To prevent accidental collisions with “normal” parse errors, `FACET_T2_UNSUPPORTED` MUST use a value that format implementations are unlikely to pick by accident.

Normative choice for the host (`facet-format`):

```rust
pub const FACET_T2_UNSUPPORTED: i32 = i32::MIN;
```

### `FACET_T2_UNSUPPORTED` requirements

- The reserved unsupported code MUST be defined by `facet-format` (so that every format uses the same sentinel).
- Formats MUST NOT use this code for parse errors.
- The Tier‑2 wrapper MUST treat this code as: “return `None` from `try_deserialize_format` so callers can fall back to Tier‑1/reflection.”

### Error position (`error_pos`)

When Tier‑2 reports an error, it MUST report a best‑effort `error_pos`:
- `error_pos` MUST be an absolute byte offset into `jit_input()`.
- On error, `error_pos` SHOULD point to the first byte where the parser can prove the input does not match the format expectation.
- For EOF errors, `error_pos` SHOULD be `len` (or the earliest position where EOF became observable).

## Cursor alignment (the core neutrality rule)

The Tier‑2 compiler MUST NOT embed format semantics such as “skip whitespace”, “handle commas”, etc.

Instead, the compiler establishes the following invariant by calling format hooks:

> **Invariant A (Aligned‑cursor invariant):** whenever the compiler calls a format operation that expects a value boundary, `cursor.pos` is aligned per `emit_align_to_value`.

### `emit_align_to_value(cursor) -> error`

**Meaning:** advance `cursor.pos` to a position where parsing the next value/container check is valid for the format.

**MUST:**
- Not move backwards.
- Not change semantic meaning (i.e. only skip trivia in the format’s definition).
- Return `0` on success.
- Return parse errors for malformed trivia sequences if the format defines them (e.g. unterminated comment).
- Return `FACET_T2_UNSUPPORTED` if the format can’t implement alignment in Tier‑2 (rare; typically it can).

**SHOULD:**
- Be a no‑op in formats where there is no trivia.

## Sequence (list/array) protocol

Tier‑2 uses a 3‑operation protocol for sequences:
- `seq_begin`: consume container start
- `seq_is_end`: check for end (and, if end, consume it)
- `seq_next`: advance after an element (consume separator if present; leave cursor ready for the next `seq_is_end` / element parse)

### `emit_seq_begin(cursor, state_ptr) -> error`

**Preconditions:**
- Compiler has called `emit_align_to_value` before this operation.
- `cursor.pos` is at the start of a sequence encoding (format‑defined).

**Postconditions on success (error = 0):**
- Initializes any sequence state in `state_ptr` (format‑defined).
- Advances `cursor.pos` past the sequence begin marker (if any).
- Calls/ensures alignment such that immediately after `seq_begin`, the cursor is:
  - at the sequence end marker (empty sequence), OR
  - at the value‑start of the first element.

### `emit_seq_is_end(cursor, state_ptr) -> (is_end: i8, error: i32)`

**Preconditions:**
- Compiler has called `emit_align_to_value` since the last cursor‑moving operation.
- Cursor is positioned either:
  - at an end marker, OR
  - at the value‑start of the next element.

**Postconditions on success:**
- If `is_end == 0`:
  - `cursor.pos` MUST remain unchanged.
  - Cursor remains at the value‑start of the next element.
- If `is_end == 1`:
  - The format MUST consume the end marker/terminator and advance `cursor.pos` to after the container.
  - The format MUST align such that after the call, cursor is at a value boundary for the *enclosing* context.

This “consume end on `is_end == 1`” rule prevents the generic compiler from needing a separate “end” operation and avoids leaking delimiter rules.

### `emit_seq_next(cursor, state_ptr) -> error`

Called after parsing one element successfully.

**Preconditions:**
- Cursor is at the value‑end (immediately after the element bytes).

**Postconditions on success:**
- Consumes any required element separator(s), if the format has them.
- MUST NOT consume the sequence end marker. (The next loop iteration will call `seq_is_end`, which consumes it if present.)
- MUST align such that, after returning, cursor is positioned so that a subsequent `emit_align_to_value` + `emit_seq_is_end` behaves correctly.

Notes:
- For delimiter formats (JSON), this means consuming `,` and aligning to the next element/end marker.
- For counted formats (postcard), this might mean decrementing a remaining‑count in `state_ptr` and doing no byte‑level work.

## Map (object/dict) protocol

Maps are similar, but include key parsing and key/value separation:

- `map_begin`: consume map start
- `map_is_end`: consume map end when present
- `map_read_key`: parse the next key
- `map_kv_sep`: consume key/value separator
- `map_next`: advance after a value (consume entry separator if present)

All map operations MUST follow the same alignment rules as sequences: the compiler aligns via `emit_align_to_value` and never assumes delimiter details.

### Key representation: `JitStringValue`

Keys are represented as a `JitStringValue`:
- May be **borrowed** (points into input slice) or **owned** (points to heap allocation).
- `owned == 0` MUST mean borrowed and does not require dropping.
- `owned == 1` MUST mean owned and must be dropped on all paths once no longer needed.

Tier‑2 v1 MAY restrict support to “borrowed keys only” and treat owned keys as unsupported until drop glue is implemented robustly in Tier‑2.

### `emit_map_read_key(cursor, state_ptr) -> (key: JitStringValue, error)`

**Preconditions:**
- Cursor is aligned and at the key position of the next entry.

**Postconditions on success:**
- Advances cursor to just after the parsed key.
- Leaves cursor positioned such that `emit_map_kv_sep` can be called next (after a compiler `emit_align_to_value`).

### `emit_key_normalize(key)`

Optional hook allowing formats that want normalization (case folding, unescaping, etc.) to do so.
This MUST NOT read beyond `key.ptr..key.ptr+key.len` when `owned==0`.

## Null / Option protocol

Optionals require the format to:
- detect whether the next value is “null / absent” in the format,
- and consume it if present.

`emit_peek_null` MUST NOT consume bytes (it only checks).
`emit_consume_null` consumes the null token.

Both MUST follow alignment rules.

## Scalar parsing protocol

Scalar parse methods (`emit_parse_bool`, `emit_parse_i64`, `emit_parse_u64`, `emit_parse_f64`, `emit_parse_string`) MUST:

**Preconditions:**
- Compiler has called `emit_align_to_value`.
- Cursor is at the scalar’s value‑start.

**Postconditions on success:**
- Cursor advances to the scalar’s value‑end.
- Returned `error == 0`.

**On error:**
- Return parse error code (non‑zero, not `FACET_T2_UNSUPPORTED`).
- Record `error_pos` accurately.
- Cursor position after error is unspecified (caller must treat as invalid and bail out).

## State memory (`state_ptr`)

Formats may need per‑container state (e.g. counted sequences, nesting flags, etc.).

The `JitFormat` trait provides:
- `SEQ_STATE_SIZE` / `SEQ_STATE_ALIGN`
- `MAP_STATE_SIZE` / `MAP_STATE_ALIGN`

The Tier‑2 compiler MUST:
- Allocate stack slots of the requested size/alignment.
- Pass `state_ptr` to the corresponding container methods.

The format MUST:
- Treat `state_ptr` as pointing to uninitialized bytes at begin.
- Initialize as needed and not read uninitialized bytes.

## Safety and portability requirements

### Bounds safety

Tier‑2 generated code MUST NOT read out of bounds.

If IR uses “trusted” memory flags (or equivalent), the format MUST ensure it emitted bounds checks that prove those loads are safe.

### 32‑bit / pointer width

Tier‑2 MUST NOT depend on:
- packing booleans into the high bit of a 64‑bit integer,
- `usize == u64`,
- or other hard 64‑bit assumptions.

If an implementation temporarily gates Tier‑2 to 64‑bit for expediency, that is allowed as an implementation choice, but the **spec** remains pointer‑width‑agnostic. Expanding to 32‑bit MUST NOT require redesign of the protocol; it should only require removing architecture guards and avoiding 64‑bit packing tricks.

### Determinism / purity

Tier‑2 parsing MUST be deterministic with respect to `(input, pos)` and not depend on global mutable state.

Helper functions MUST be thread‑safe and re‑entrant.

## Compiler responsibilities (what `facet-format` may assume)

The Tier‑2 compiler MAY assume:
- It controls when `emit_align_to_value` is called.
- Format operations respect the pre/postconditions in this spec.
- `FACET_T2_UNSUPPORTED` triggers fallback.

The Tier‑2 compiler MUST NOT assume:
- specific trivia characters (spaces/tabs/etc.),
- delimiter bytes (`,`/`]`/`}`),
- any JSON‑specific tokenization rules,
- or that alignment is performed implicitly by some subset of operations.

If the compiler needs alignment at a point, it MUST call `emit_align_to_value`. This is the primary mechanism that prevents JSON semantics from bleeding into `facet-format`.

## Capability detection / compatibility

Tier‑2 has two layers of compatibility:

1. **Shape compatibility** (owned by `facet-format`): whether a given `Shape` can be expressed using the Tier‑2 protocol and the available format operations.
2. **Format compatibility** (owned by the format crate): whether the `JitFormat` supports the operations required for that `Shape`.

The Tier‑2 entrypoint MUST:
- reject types/shapes that are not shape‑compatible
- reject parser instances without stable `(input, pos)`
- reject formats that return `FACET_T2_UNSUPPORTED`

…and then fall back to Tier‑1/reflection.

## Example mappings (non‑normative)

### JSON (delimiter format)

- Trivia: spaces/tabs/newlines/comments (format‑defined).
- `emit_align_to_value`: skip trivia.
- `seq_begin`: consume `[` then align.
- `seq_is_end`: if next non‑trivia is `]`, consume it, align, return `is_end=1`; else return `is_end=0`.
- `seq_next`: align, then require `,` or `]`; if `,`, consume and align; if `]`, leave it (so next `seq_is_end` consumes it).

### Postcard / counted binary format (counted format)

- Trivia: none → `emit_align_to_value` is a no‑op.
- Sequence encoding: `u32` element count then elements back‑to‑back.
- `SEQ_STATE_SIZE`: enough for a remaining‑count `u32` (and possibly the initial count).
- `seq_begin`: read count into state; cursor advanced past count.
- `seq_is_end`: check remaining‑count; if 0, set `is_end=1` (and consume nothing, since the container ends implicitly); if >0, `is_end=0`.
  - To satisfy the “consume end on `is_end==1`” rule for implicit end, consumption is a no‑op; alignment remains a no‑op.
- `seq_next`: decrement remaining‑count; no bytes consumed.

This demonstrates why `facet-format` must not talk about whitespace/delimiters: the separator/end concepts are state‑driven, not byte‑driven.

## Conformance checklist

An implementation is Tier‑2 conforming when:
- The compiler never encodes format semantics (no delimiter/trivia logic).
- Every place the compiler needs a value boundary, it calls `emit_align_to_value`.
- Format operations implement the pre/postconditions above.
- Unsupported is a dedicated sentinel code that triggers fallback, not a parse error.
- Error positions are accurate byte offsets into the input slice.
