# Deserializing Styx into facet_value::Value

## Overview

Enable `facet-styx` to deserialize Styx format directly into `facet_value::Value`, creating a unified dynamic value representation for Styx data. This allows applications to work with Styx data as dynamic values before deserializing into strongly-typed Rust structs.

## Motivation

- **Dynamic data handling**: Process Styx without pre-defining types
- **Interactive/exploratory workflows**: Query and inspect Styx structures at runtime
- **Interoperability**: Bridge between Styx and other systems that work with facet-value
- **Intermediate representation**: Use Value as a stepping stone for validation, transformation, or routing before deserializing to specific types
- **Debugging/inspection**: Easier inspection of Styx data in dynamic form

## Current State

### facet-styx
- Parses Styx using `StyxParser` (implements `FormatParser` trait)
- Deserializes directly into Rust types via `FormatDeserializer`
- Supports Styx syntax: scalars, sequences, objects, tagged values, unit values

### facet-value
- Dynamic pointer-sized value type (8 bytes on 64-bit)
- Supports 8 primary types: Null, Bool, Number, String, Bytes, Array, Object, DateTime
- Extensible via "Other" tag for custom types (currently QName, UUID)
- Zero-copy in many cases via inline encoding

## Implementation Approach

### High-Level Architecture

```
Styx source string
        ↓
    StyxParser (FormatParser trait implementation)
        ↓
    ParseEvent stream (Null, Bool, Scalar, BeginObject, Key, EndObject, etc.)
        ↓
    ValueBuilder (consumes events → Value)
        ↓
    facet_value::Value
```

### Concrete Implementation Steps

#### 1. Create ValueBuilder in facet-styx

A builder that implements the event-consumer pattern:

```rust
pub struct ValueBuilder {
    stack: Vec<ValueBuilderState>,
}

enum ValueBuilderState {
    Root,
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),
}

impl ValueBuilder {
    pub fn new() -> Self { /* ... */ }

    pub fn push_event(&mut self, event: ParseEvent) -> Result<(), Error> {
        match event {
            ParseEvent::Null => { /* build null */ },
            ParseEvent::Bool(b) => { /* build bool */ },
            ParseEvent::Scalar(s) => { /* parse scalar, infer type */ },
            ParseEvent::BeginSequence => { /* start array */ },
            ParseEvent::EndSequence => { /* finish array */ },
            ParseEvent::BeginObject => { /* start object */ },
            ParseEvent::Key(k) => { /* set current key */ },
            ParseEvent::EndObject => { /* finish object */ },
            // ... other events
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Value, Error> {
        // Return the root Value
    }
}
```

#### 2. High-Level Deserializer Function

In `facet_styx` crate, add:

```rust
/// Deserialize Styx format into a facet_value::Value
pub fn from_str_to_value(input: &str) -> Result<Value, StyxError> {
    let mut parser = StyxParser::new(input);
    let mut builder = ValueBuilder::new();

    while let Some(event) = parser.next()? {
        builder.push_event(event)?;
    }

    builder.finish().map_err(|e| StyxError::from(e))
}
```

#### 3. Type Inference for Styx Scalars

Styx scalars need intelligent conversion to Value types:

```
Styx scalar "true"/"false"  → Value::Bool
Styx scalar "null"          → Value::Null
Styx scalar /^-?\d+$/       → Value::Number (integer)
Styx scalar /^-?\d+\.\d+$/  → Value::Number (float)
Styx scalar (bytes literal) → Value::Bytes
Styx scalar (anything else) → Value::String
```

The parser already distinguishes some of these; we reuse that info.

#### 4. Handling Styx Tags

Styx supports tagged values: `@tag(content)` or `@tag { key value ... }`

**Initial approach (Option A - Conservative):**
- Store tagged values in Value as objects with magic key like `"@tag"`
- Example: `@mytype(42)` → `{ "@tag": "mytype", "@value": 42 }`
- Simple, doesn't require extending facet-value

**Future approach (Option B - Native Support):**
- Extend facet-value with a new type: `Value::Tagged { tag: String, value: Box<Value> }`
- Or use the "Other" extensible type mechanism
- More elegant but requires facet-value changes

**Recommendation:** Start with Option A (conservative). If tag support becomes critical, extend facet-value.

#### 5. Handling Unit Values

Styx supports bare `@` (unit type):

```rust
// The `@` represents unit
// Options:
// - Convert to Value::Null
// - Convert to empty object Value::Object({})
// - Convert to Value::String("") (zero-length string as sentinel)
// - Extend facet-value with Unit type
```

**Recommendation:** Start with `Value::Null`. Document that `@` and `null` both deserialize to the same value.

#### 6. Sequence/Array Handling

Styx sequences: `(a b c)` → `Value::Array([a, b, c])`

Standard case covered by event stream (BeginSequence, elements, EndSequence).

#### 7. Object Handling

Styx objects (explicit): `{ a 1, b 2 }` or implicit root object with multiple keys.

```rust
// Explicit objects → Value::Object
// Implicit root object → Value::Object at the top level
// Keys must be strings; values are recursively built
```

### Error Handling

Extend `StyxError` or create `ValueConversionError`:

```rust
pub enum StyxToValueError {
    ParserError(StyxErrorKind),
    InvalidScalarType { value: String, expected: &'static str },
    DuplicateObjectKey { key: String },
    UnexpectedEof,
    // ... others
}
```

## Potential Extensions to facet-value

### Option 1: Native Tagged Values Type

If we want first-class support for Styx tags:

```rust
// In facet-value
pub enum OtherValue {
    Tagged { tag: String, value: Box<Value> },
    // existing: QName, UUID
}

// Usage:
// let tagged = Value::other(OtherValue::Tagged {
//     tag: "mytype".to_string(),
//     value: Box::new(Value::from(42)),
// });
```

**Pros:** Clean, matches Styx semantics
**Cons:** Requires facet-value changes, uses "Other" extensible slot

### Option 2: Native Unit Type

Currently no unit/void type in Value. Options:

- Keep using `Value::Null` (zero-cost, clear semantics)
- Add `Value::Unit` as 9th type (requires tag slot management)

**Recommendation:** Use `Value::Null` for now. If semantic distinction matters, add later.

### Option 3: String Union Type (Advanced Future)

Some Styx scenarios might benefit from union types in Value. This is more speculative and can be deferred.

## API Design

### Public API in facet-styx

```rust
// Primary function
pub fn from_str_to_value(input: &str) -> Result<Value, StyxError>;

// Extended API
pub fn from_slice_to_value(input: &[u8]) -> Result<Value, StyxError>;
pub fn from_reader_to_value<R: std::io::Read>(reader: R) -> Result<Value, StyxError>;

// Round-trip serialization
pub fn value_to_string(value: &Value) -> Result<String, StyxError>;
```

### Integration with facet-value

Add to `facet-styx/Cargo.toml`:

```toml
facet-value = { path = "../../facet/facet-value" }
```

## Testing Strategy

1. **Unit tests** for ValueBuilder state machine
2. **Round-trip tests**: Styx → Value → Styx (check equivalence)
3. **Type inference tests**: Verify scalars infer correct types
4. **Nested structure tests**: Complex objects, arrays, mixed
5. **Tag handling tests**: Verify tagged values serialize/deserialize correctly
6. **Error handling tests**: Malformed input, edge cases

Example test:

```rust
#[test]
fn test_simple_object_to_value() {
    let styx = "name alice\nage 30";
    let value = from_str_to_value(styx).unwrap();

    assert!(value.is_object());
    let obj = value.as_object().unwrap();
    assert_eq!(obj.get("name").unwrap().as_string().unwrap(), "alice");
    assert_eq!(obj.get("age").unwrap().as_number().unwrap().as_i64(), Some(30));
}
```

## Migration Path

1. **Phase 1 (MVP):**
   - Implement `ValueBuilder` consuming `ParseEvent`
   - Add `from_str_to_value()` function
   - Support null, bool, number, string, bytes, array, object
   - Handle tagged values as magic objects (Option A)
   - Handle unit as null

2. **Phase 2 (Polish):**
   - Add `value_to_string()` serialization
   - Comprehensive round-trip tests
   - Performance optimization

3. **Phase 3 (Optional Extensions):**
   - If tags become critical: extend facet-value with native `Tagged` type
   - If unit semantics matter: add distinct `Value::Unit`
   - Alternative: Add union types if use cases emerge

## Benefits & Tradeoffs

### Benefits
- ✅ Process Styx dynamically without pre-defined types
- ✅ Leverage facet-value's efficient pointer-sized representation
- ✅ Enable exploratory/debugging workflows
- ✅ Bridge to other systems using facet-value
- ✅ Zero-copy in many cases (inline encoding)

### Tradeoffs
- ⚠️ Tags stored as nested objects (not ideal semantically, acceptable pragmatically)
- ⚠️ Unit values indistinguishable from null (document clearly)
- ⚠️ Type inference for scalars is heuristic-based (but matches Styx intent)
- ⚠️ Potential future need to extend facet-value if native tag support desired

## Questions & Decisions Needed

1. **Tag representation**: Option A (nested object) or Option B (extend facet-value)?
2. **Unit type**: Use Null, or extend facet-value with Unit variant?
3. **Performance requirements**: Any specific benchmarks to target?
4. **Serialization round-trip**: Is `value_to_string()` essential in Phase 1, or nice-to-have?

## References

- **facet-styx**: `crates/facet-styx/` — Parser, events, serialization
- **facet-value**: `~/bearcove/facet/facet-value/` — Dynamic value type
- **FormatParser trait**: `styx-format` or facet-format crate
- **ParseEvent types**: Event stream produced by StyxParser
