# facet-format: Format-Specific JIT (Reference Design)

## Status and scope

This document specifies a concrete design for adding **format-specific JIT deserialization** to `facet-format`, while keeping the existing `ParseEvent` architecture intact for non-JIT and streaming use-cases.

Initial scope:
- **Supported inputs:** complete, in-memory byte slices (`&[u8]`) with a cursor position.
- **Primary target:** `facet-format-json` slice parser (`JsonParser<'de>`).
- **Initial shapes:** `Vec<T>` for scalar `T` (starting with `bool`) and simple structs with scalar fields. (Broader coverage can follow.)
- **Streaming:** explicitly out of scope for the first iteration; Tier 2 will be disabled when a parser can’t provide a complete slice.

Non-goals for the first iteration:
- JIT for `AsyncRead` / partial buffers.
- Supporting borrowed output types (`&str`, `Cow<'de, str>`) in JIT output.
- Perfect error reporting parity with the reflection path (we will design for good errors, but optimize for correctness + speed first).

## Terminology

- **Tier 1 / Shape JIT:** existing `facet-format` Cranelift JIT that consumes `ParseEvent` via `FormatParser` vtable calls (`jit_next_event`, `jit_peek_event`).
- **Tier 2 / Format JIT:** new path where generated code parses bytes directly via a format-provided Cranelift “emitter”.
- **Emitter / JitFormat:** a Rust trait implemented by each format crate to generate format-specific parsing IR (array/map protocols, scalar parsing, whitespace/comments).
- **Cursor:** `(input_ptr, len, pos)` passed to the compiled function; `pos` is mutated during parsing.

## High-level architecture

### Two-tier JIT dispatch

Because associated type defaults are unstable on stable Rust, Tier 2 cannot be expressed as “an optional associated type on every `FormatParser`”. Instead, Tier 2 is enabled by implementing a **separate opt-in trait** for parsers that can expose a complete input slice and cursor.

Concretely:

1. `facet_format::jit::try_deserialize::<T, P>(parser: &mut P)` remains the Tier-1 entry point (shape JIT over `ParseEvent`), usable by any `P: FormatParser`.
2. `facet_format::jit::try_deserialize_format::<T, P>(parser: &mut P)` is the Tier-2 entry point and requires `P: FormatJitParser<'de>`.
3. Format crates that want “try Tier 2 then fall back” provide (or use) a wrapper that attempts Tier 2 and then Tier 1 / reflection.

Tier selection is strictly a performance choice; semantics must match (within the supported subset).

### Responsibility split

`facet-format` (shared core) owns:
- Shape traversal and semantics (struct fields, options, vec/list, nested, enums later).
- Memory layout and writes (field offsets, initializing/pushing vectors).
- Caching and compilation lifecycle.

Format crates own:
- Surface syntax parsing rules (delimiters, separators, whitespace/comments, scalar tokenization).
- Any helper functions needed for complex parsing (strings with escapes, float parsing, etc.).

## API changes

### 1) New opt-in trait: `FormatJitParser<'de>` (behind `feature = "jit"`)

We add a new trait in `facet-format` that is implemented only by parsers that support Tier 2.

This avoids needing associated type defaults on `FormatParser`, and it makes the capability boundary explicit.

```rust
// facet-format/src/jit/format_parser.rs (or similar)
pub trait FormatJitParser<'de>: crate::FormatParser<'de> {
    /// Format-specific emitter used during Tier-2 codegen.
    type Format: crate::jit::JitFormat;

    /// Return the full input slice. Tier 2 is only valid for complete inputs.
    ///
    /// Returning `None` disables Tier 2 for this parser instance (e.g. streaming mode).
    fn jit_input(&self) -> Option<&'de [u8]>;

    /// Return the current absolute byte offset (cursor position).
    ///
    /// Returning `None` disables Tier 2 for this parser instance (e.g. if the
    /// parser has buffered state such as a cached `peek_event`).
    fn jit_pos(&self) -> Option<usize>;

    /// Commit a new cursor position after Tier-2 execution succeeds.
    ///
    /// Must also invalidate/reset any internal scanning/tokenizer state so that
    /// subsequent parsing continues from `pos` consistently.
    fn jit_set_pos(&mut self, pos: usize);

    /// Return a format emitter instance (usually a ZST).
    fn jit_format(&self) -> Self::Format;

    /// Convert a Tier-2 error (code + position) into `Self::Error`.
    ///
    /// This is only called on the slow error path.
    fn jit_error(&self, input: &'de [u8], error_pos: usize, error_code: i32) -> Self::Error;
}
```

Key rule: `jit_set_pos` must bring the parser back into a coherent state. For `facet-format-json::JsonParser`, this implies:
- format JIT is disabled if `peek_event` has cached an event (or we define a “clear peek” policy).
- on success, set `current_offset = new_pos` and reset/reinitialize `SliceAdapter` to start scanning from `new_pos` while preserving absolute span semantics.

### 1.1) Wrapper function for “Tier 2 then Tier 1”

Because Tier 2 uses a separate trait, the “attempt Tier 2 first” policy lives in a wrapper with an extra bound:

```rust
pub fn try_deserialize_with_format_jit<'de, T, P>(
    parser: &mut P,
) -> Option<Result<T, crate::DeserializeError<P::Error>>>
where
    T: facet_core::Facet<'de>,
    P: FormatJitParser<'de>,
{
    // try Tier 2 (format jit), else try Tier 1 (shape jit)
    // return None if neither jit applies
}
```

### 2) New `JitFormat` trait: format-specific IR building blocks

`JitFormat` is defined in `facet-format/src/jit/format.rs` and implemented by format crates.

The core insight: formats must provide **container protocols**, not just scalar parsing.

#### Cursor model

All Tier-2 compiled functions operate over:
- `input_ptr: *const u8`
- `len: usize`
- `pos: usize` (mutable)

Within codegen, we model this as:

```rust
// facet-format/src/jit/format.rs
pub struct JitCursor {
    pub input_ptr: cranelift::prelude::Value,
    pub len: cranelift::prelude::Value,
    pub pos: cranelift::prelude::Variable,
    pub ptr_type: cranelift::prelude::Type,
}
```

`facet-format` provides helper routines (in Rust, during codegen) for:
- loading a byte at `pos` with bounds checks
- bumping `pos`
- creating “error blocks” and branching patterns

This prevents every format from re-implementing basic “safe cursor” IR patterns.

#### String representation for handoff

Tier 2 initially targets **owned outputs** (`String`, not `&str`/`Cow`), but we still want to avoid allocations in the parser loop when possible.

We represent strings as:

```rust
pub struct JitStringValue {
    pub ptr: Value,   // *const u8 (or *mut u8 if owned)
    pub len: Value,   // usize
    pub cap: Value,   // usize (only meaningful when owned)
    pub owned: Value, // i8: 1 => ptr/len/cap are String raw parts; 0 => ptr/len borrowed
}
```

This matches existing `jit_write_string(out, offset, ptr, len, cap, owned)` semantics.

Additionally, Tier 2 needs a way to drop temporary owned strings (e.g., map keys) if they were allocated but not moved into output.
We therefore add a shared helper in `facet-format`:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_drop_owned_string(ptr: *mut u8, len: usize, cap: usize) {
    drop(String::from_raw_parts(ptr, len, cap));
}
```

#### Container state

Some formats need per-container state:
- YAML block style: indentation level, “at line start” flags, etc.
- TOML: potentially key-path context / root mode.

To support this without embedding format-specific state into `facet-format`, the interface allows an **opaque state pointer** per container protocol.
The format tells the compiler what stack slot size/alignment is required.

For formats that don’t need it (JSON), state size is 0 and `state_ptr` is ignored.

#### The trait

```rust
pub trait JitFormat: Default + Copy + 'static {
    /// Called during compilation to register format helper symbols (optional).
    fn register_helpers(builder: &mut cranelift_jit::JITBuilder);

    /// Stack slot layout required for sequence state (0 means no state).
    const SEQ_STATE_SIZE: u32 = 0;
    const SEQ_STATE_ALIGN: u32 = 1;

    /// Stack slot layout required for map state (0 means no state).
    const MAP_STATE_SIZE: u32 = 0;
    const MAP_STATE_ALIGN: u32 = 1;

    // --- utility ---
    fn emit_skip_ws(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> Value /*err*/;
    fn emit_skip_value(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> Value /*err*/;

    // --- null / option ---
    fn emit_peek_null(&self, b: &mut FunctionBuilder, c: &mut JitCursor)
        -> (Value /*is_null i8*/, Value /*err*/);
    fn emit_consume_null(&self, b: &mut FunctionBuilder, c: &mut JitCursor) -> Value /*err*/;

    // --- scalars ---
    fn emit_parse_bool(&self, b: &mut FunctionBuilder, c: &mut JitCursor)
        -> (Value /*i8*/, Value /*err*/);
    fn emit_parse_i64(&self, b: &mut FunctionBuilder, c: &mut JitCursor)
        -> (Value /*i64*/, Value /*err*/);
    fn emit_parse_u64(&self, b: &mut FunctionBuilder, c: &mut JitCursor)
        -> (Value /*u64 as i64*/, Value /*err*/);
    fn emit_parse_f64(&self, b: &mut FunctionBuilder, c: &mut JitCursor)
        -> (Value /*f64*/, Value /*err*/);
    fn emit_parse_string(&self, b: &mut FunctionBuilder, c: &mut JitCursor)
        -> (JitStringValue, Value /*err*/);

    // --- sequences ---
    fn emit_seq_begin(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value /*err*/;

    fn emit_seq_is_end(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value /*is_end i8*/, Value /*err*/);

    /// Called after parsing an element, before the next loop iteration.
    /// Responsible for consuming the entry separator (comma/newline/etc) if present/required.
    fn emit_seq_next(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value /*err*/;

    // --- maps (object-like) ---
    fn emit_map_begin(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value /*err*/;

    fn emit_map_is_end(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (Value /*is_end i8*/, Value /*err*/);

    fn emit_map_read_key(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> (JitStringValue, Value /*err*/);

    fn emit_map_kv_sep(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value /*err*/;

    fn emit_map_next(
        &self,
        b: &mut FunctionBuilder,
        c: &mut JitCursor,
        state_ptr: Value,
    ) -> Value /*err*/;

    /// Optional: allow a format to canonicalize a parsed key before matching.
    /// Default is no-op; YAML/TOML may want case-folding or bareword normalization.
    fn emit_key_normalize(
        &self,
        _b: &mut FunctionBuilder,
        _key: &mut JitStringValue,
    ) {
    }
}
```

Error handling convention: methods return `err` as an `i32`-compatible `Value` where `0` means OK.
The Tier-2 compiler centralizes error branching:
- after each emitter call, compare `err == 0`, otherwise jump to a shared error block.

### 3) Tier-2 compiled function ABI

All Tier-2 compiled deserializers share a single ABI:

```rust
/// Returns:
/// - `>= 0`: new cursor position (success)
/// - `<  0`: failure; error details are written to `scratch`
type JitFormatFn =
    unsafe extern "C" fn(
        input_ptr: *const u8,
        len: usize,
        pos: usize,
        out: *mut u8,
        scratch: *mut JitScratch,
    ) -> isize;

#[repr(C)]
pub struct JitScratch {
    pub error_code: i32,
    pub error_pos: usize,
}
```

Notes:
- `out` is a `*mut T` cast to `*mut u8` on the Rust side (same as existing helpers).
- The compiled code must write `scratch.error_code/error_pos` before returning `< 0`.
- We keep the return value as the new position because that is required to update the parser.

Wrapper behavior:
- Call compiled fn with `(input_ptr, len, pos, out, &mut scratch)`.
- On success, `parser.jit_set_pos(new_pos)`.
- On failure, return `DeserializeError::Parser(parser.jit_error(input, scratch.error_pos, scratch.error_code))`.

## Tier-2 compiler in `facet-format`

We add a new compiler path in `facet-format/src/jit/format_compiler.rs`:

- Entry point: `try_compile_format::<T, P>() -> Option<CompiledFormatDeserializer<T, P>>`
- Uses the same caching style as Tier 1 (keyed by `(ConstTypeId(T), ConstTypeId(P), Tier)`).
- Invokes `<P::Format as JitFormat>::register_helpers(builder)` in addition to existing helper registration.

### Shape compatibility for Tier 2

Tier 2 and Tier 1 should not necessarily share the same compatibility predicate.

For the MVP, Tier 2 supports:
- `Vec<T>` where `T` is one of: `bool`, `i64`-family, `u64`-family, `f64`-family, `String`
- simple structs (no flatten, no untagged enums) with those scalar fields, plus nested supported lists/structs
- `Option<T>` where `T` is supported and the format implements null-like behavior (JSON: yes; TOML: likely `emit_peek_null` always false)

Unsupported shapes fall back to Tier 1 / reflection.

### Field set validation (UB avoidance)

The existing Tier-1 JIT has a known missing-field validation issue (it may `assume_init` without verifying all required fields were set).

Tier 2 must not introduce new UB; ideally we fix this in the shared logic and apply it to both tiers:
- compile a bitset on the stack
- set the bit whenever a non-Option field is written
- at function end, verify required bits are set; otherwise error

This is a correctness requirement independent of parsing speed.

## JSON implementation mapping (Tier 2)

`facet-format-json` implements Tier 2 by providing a `JsonJitFormat` that parses the JSON syntax directly from bytes:

### Whitespace
- `emit_skip_ws`: tight loop over `[ \t\r\n]`.

### Sequences (arrays)
- `emit_seq_begin`: expect `'['`, advance, then skip ws.
- `emit_seq_is_end`: after ws, check `']'` (without consuming it) and return `is_end`.
- `emit_seq_next`: after element parse, skip ws, then:
  - if next is `','`, consume and skip ws
  - else if next is `']'`, do nothing (next `seq_is_end` will end)
  - else error

### Maps (objects)
- `emit_map_begin`: expect `'{'`, advance, skip ws.
- `emit_map_is_end`: after ws, check `'}'`.
- `emit_map_read_key`: parse a JSON string (key), return `JitStringValue`:
  - fast path: no escapes ⇒ return borrowed slice (owned=0)
  - slow path: escapes ⇒ call a helper to decode into owned string (owned=1)
- `emit_map_kv_sep`: expect `':'`, consume, skip ws.
- `emit_map_next`: after value parse, skip ws, then consume `','` or accept `'}'` analogous to arrays.

### Scalars
- `emit_parse_bool`: inline `true`/`false` recognition (bounds checks + byte comparisons).
- numbers/strings: can start helper-based (call into `facet-format-json` helper functions) and be inlined later.

This mapping ensures that parsing + control flow are fused in the compiled function, eliminating the layered event pipeline for Tier 2.

## Extensibility notes (YAML/TOML)

The `JitFormat` interface is intentionally expressed in terms of:
- “sequence protocol”
- “map protocol”
- scalar parsing
- whitespace/comments

This is sufficient for:
- **TOML:** root is “map-like without braces” (map protocol can treat begin/end as start-of-doc/EOF), kv separator is `'='`, entry separator is newline; arrays are JSON-like.
- **YAML:** flow style maps/sequences (`{}`, `[]`) map cleanly; block style requires state (indentation), which is why state stack slots exist.

Formats may start with a limited Tier-2 subset:
- YAML Tier 2 supports flow style only initially; block style falls back to Tier 1.
- TOML Tier 2 supports arrays and inline tables first.

This is acceptable because Tier 2 is an optimization; unsupported constructs simply don’t take the fast path.

## Interaction with Tier 1 (shape JIT) and reflection

The tiers are strictly ordered:
1. Tier 2 if supported + slice available + compatible shape
2. Tier 1 if compatible shape (existing)
3. Reflection (`FormatDeserializer`) otherwise

Tier 2 is never attempted on streaming inputs unless the parser explicitly returns a complete slice.

## Open questions / follow-ups (not required for MVP)

- Add a Tier-1 “raw event” API to avoid `ParseEvent → RawEvent` conversion overhead (helps Tier 1).
- Share helper functions between `facet-json` and `facet-format-json` (or formally migrate `jitson_*` helpers into `facet-format-json`).
- Improve error reporting in Tier 2 (expected token kinds, spans, context).
- Support borrowed output types (`&str`, `Cow<str>`) by adding write helpers for them and tightening lifetimes.
