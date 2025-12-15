# Handoff: JIT Deserialization Bug for XML

## Current State
The JIT deserializer works correctly for JSON but produces garbage values for XML, despite both parsers producing identical event streams.

## What Works
- JSON JIT tests pass (4/4)
- Memory layout tests pass
- Events are correctly produced: `Scalar(I64(42))` for the `id` field
- The `into_raw_parts` fix for owned strings works (string content shows correctly in debug)

## The Bug
For XML input `<root><id>42</id><name>test</name></root>`:
- Events show: `Scalar(I64(42))` ✓
- Result shows: `id: 4308501496` ✗ (garbage, looks like a pointer)
- String field causes UB panic when accessed

## Likely Culprit
The JIT-compiled code is reading from wrong memory locations. The value `4308501496` (0x100D2AFF8) looks like a memory address, suggesting the JIT is loading a pointer instead of the actual i64 value.

## Files to Focus On
- `facet-format/src/jit/compiler.rs` - Cranelift codegen
- `facet-format/src/jit/helpers.rs` - FFI helpers, RawEvent layout
- `facet-format-xml/tests/jit_test.rs` - Failing test

## Debug Output Added
```rust
// In helpers.rs next_event_wrapper:
eprintln!("[JIT] next_event: Scalar(I64({})) -> writing to {:p}", value, out);

// In helpers.rs jit_write_u64:
eprintln!("[JIT] write_u64: value={} to {:p}+{}", value, out, offset);
```

## To Run on Linux
```bash
# Run the failing test
cargo test -p facet-format-xml --test jit_test test_jit_xml_deserialize -- --nocapture

# With valgrind
cargo test -p facet-format-xml --test jit_test test_jit_xml_deserialize --no-run
valgrind --track-origins=yes ./target/debug/deps/jit_test-* test_jit_xml_deserialize --nocapture
```

## Key Question
Why does JSON work but XML doesn't when:
1. Both produce identical ParseEvent streams
2. Both go through the same JIT-compiled code path
3. The RawEvent is correctly populated (verified by debug output)

The difference must be in how the vtable/parser pointer interacts with the JIT, or something about the timing of when values are read from the stack-allocated RawEvent.

## Recent Changes
- Added `capacity` field to `StringPayload` for proper ownership transfer
- Implemented `string_into_raw_parts()` and `vec_into_raw_parts()` to safely decompose owned strings/vecs
- Updated `convert_event_to_raw()` to use `into_raw_parts` for owned data
- Updated `jit_write_string()` signature to accept capacity
- Updated compiler to pass capacity to write_string helper
- Changed from `TypeId::of::<P>()` to `ConstTypeId::of::<P>()` to remove `'static` bound on parsers
