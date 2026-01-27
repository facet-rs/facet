# Plan: Remove corosensei from facet-format

## Goal

Remove the `corosensei` dependency from `facet-format` to enable WASM compilation. Use dynamic dispatch via `DynParser` instead of coroutines to achieve the same monomorphization reduction.

## Background

### Why corosensei was used

The coroutine-based approach in `coro.rs` was designed to reduce monomorphization. Without it, every inner deserialization function would be compiled separately for each parser type (JSON, YAML, TOML, MsgPack, etc.), causing code bloat.

The coroutine approach:
1. Inner functions (like `deserialize_enum_externally_tagged_inner`) take a `DeserializeYielder`
2. They yield `DeserializeRequest` variants when needing parser operations
3. A driver loop (`run_deserialize_coro`) handles requests by calling the parser
4. Inner functions are compiled once, driver is compiled per parser type

### The DynParser alternative

PR #1939 added `impl FormatParser<'de> for &mut dyn DynParser<'de>`. This means:
- We can use `&mut dyn DynParser<'de>` as a generic `P: FormatParser<'de>`
- `FormatDeserializer<'input, BORROW, &mut dyn DynParser<'input>>` works
- Inner functions taking this type are compiled **once** (for the dyn trait object)
- Achieves the same monomorphization reduction without coroutines

## Implementation Plan

### Phase 1: Convert inner functions to use DynParser directly

Transform inner functions from:

```rust
fn deserialize_enum_variant_content_inner<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    let event = request_event(yielder, "value")?;
    wip = request_deserialize_into(yielder, wip)?;
    // ...
}
```

To:

```rust
fn deserialize_enum_variant_content_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn DynParser<'input>>,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError<DynParserError>> {
    let event = deser.expect_event("value")?;
    wip = deser.deserialize_into(wip)?;
    // ...
}
```

Request-to-method mapping:
| Old (coroutine) | New (direct call) |
|-----------------|-------------------|
| `request_event(yielder, exp)?` | `deser.expect_event(exp)?` |
| `request_peek(yielder, exp)?` | `deser.expect_peek(exp)?` |
| `request_peek_raw(yielder)?` | `deser.parser.peek_event()?` |
| `request_skip(yielder)?` | `deser.parser.skip_value()?` |
| `request_span(yielder)` | `deser.last_span` |
| `request_deserialize_into(yielder, wip)?` | `deser.deserialize_into(wip)?` |
| `request_collect_evidence(yielder)?` | `deser.collect_evidence()?` |
| `request_set_string_value(yielder, wip, s)?` | `deser.set_string_value(wip, s)?` |
| `request_hint_enum(yielder, hints)?` | `deser.parser.hint_enum(&hints)` |
| `request_solve_variant(yielder, shape)?` | `crate::solve_variant(shape, &mut deser.parser)?` |

### Phase 2: Update wrapper methods

Transform wrapper methods from:

```rust
pub fn deserialize_enum_variant_content(
    &mut self,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
    run_deserialize_coro(
        self,
        Box::new(move |yielder| deserialize_enum_variant_content_inner(yielder, wip)),
    )
}
```

To:

```rust
pub fn deserialize_enum_variant_content(
    &mut self,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
    // Create a dyn-dispatched deserializer
    let dyn_parser: &mut dyn DynParser<'input> = &mut self.parser;
    let mut dyn_deser = FormatDeserializer::<BORROW, _>::new(dyn_parser);
    dyn_deser.last_span = self.last_span;
    dyn_deser.current_path = self.current_path.clone();

    let result = deserialize_enum_variant_content_inner(&mut dyn_deser, wip)
        .map_err(|e| /* convert DynParserError back if needed */)?;

    self.last_span = dyn_deser.last_span;
    Ok(result)
}
```

### Phase 3: Delete coro.rs and update dependencies

1. Delete `facet-format/src/deserializer/coro.rs`
2. Remove `mod coro;` from `deserializer.rs`
3. Remove from `facet-format/Cargo.toml`:
   ```toml
   corosensei = { version = "0.3", features = ["default-stack", "unwind"] }
   ```

### Phase 4: Keep facet-json streaming unchanged

The streaming feature uses corosensei for a different purpose (incremental parsing). Keep it as-is:
- `facet-json/Cargo.toml` already has `corosensei` as optional
- Streaming will work on native platforms, just not WASM
- Future work could replace it with a state machine if needed

## Files to Modify

| File | Changes |
|------|---------|
| `facet-format/Cargo.toml` | Remove corosensei dependency |
| `facet-format/src/deserializer.rs` | Remove `mod coro;` |
| `facet-format/src/deserializer/eenum.rs` | Convert 9 inner functions to use DynParser |
| `facet-format/src/deserializer/struct_with_flatten.rs` | Convert inner functions |
| `facet-format/src/deserializer/dynamic.rs` | Convert inner functions |

## Files to Delete

- `facet-format/src/deserializer/coro.rs`

## Files Unchanged

- `facet-json/Cargo.toml` - corosensei already optional
- `facet-json/src/streaming.rs` - keep using corosensei
- `facet-json/src/streaming_adapter.rs` - keep using corosensei

## Testing

1. All existing tests should pass (behavior unchanged)
2. WASM compilation test:
   ```bash
   cargo build -p facet-format --target wasm32-unknown-unknown
   ```
3. Streaming still works on native platforms

## Order of Implementation

1. Start with one inner function as proof of concept
2. Convert remaining functions in `eenum.rs` (9 functions)
3. Convert functions in `struct_with_flatten.rs`
4. Convert function in `dynamic.rs`
5. Delete `coro.rs`
6. Remove corosensei from Cargo.toml
7. Run tests
8. Test WASM compilation
