# Unified Deserialization Abstraction for Facet

## Executive Summary

This document proposes a **unified backend abstraction** that can work for all facet format deserializers, whether they're event-streaming (JSON, YAML), DOM-based (XML, KDL), or already semantic (Value).

The key insight: **Partial is already the abstraction**. We just need to add a thin backend layer that abstracts over parsing strategies.

## The Three Layers

```
┌─────────────────────────────────────────┐
│  Layer 3: Type Construction             │  ← Identical across all formats
│  (Partial API: begin_field, set, end)   │
├─────────────────────────────────────────┤
│  Layer 2: Semantic Backend              │  ← NEW: Unified abstraction
│  (DeserializerBackend trait)            │
├─────────────────────────────────────────┤
│  Layer 1: Format Parsing                │  ← Format-specific
│  (Tokens/Events/DOM/Enum)               │
└─────────────────────────────────────────┘
```

### Layer 1: Format Parsing (Format-Specific)

Each format has its own parsing mechanism:

- **JSON**: Token stream from `Scanner` → `Token<'input>`
- **YAML**: Event stream from `saphyr_parser` → `OwnedEvent`
- **XML**: Event stream from `quick_xml` → `OwnedEvent`
- **KDL**: DOM tree from `kdl` crate → `KdlDocument` (will eventually be event-based)
- **Value**: Already semantic → `Value` enum

### Layer 2: Semantic Backend (NEW - Unified)

This layer provides a **format-agnostic view** of the data:

```rust
pub trait DeserializerBackend {
    type Position: Clone;

    /// Peek at current element without consuming
    fn peek(&self) -> Result<Element, Self::Error>;

    /// Advance to next element
    fn advance(&mut self) -> Result<(), Self::Error>;

    /// Enter a structure (object/array/element)
    /// Returns expected element count if known
    fn enter_structure(&mut self) -> Result<Option<usize>, Self::Error>;

    /// Exit current structure
    fn exit_structure(&mut self) -> Result<(), Self::Error>;

    /// Get current field/key name
    fn get_field_name(&mut self) -> Result<String, Self::Error>;

    /// Get scalar value
    fn get_scalar(&mut self) -> Result<Scalar, Self::Error>;

    /// Get current position (for error reporting)
    fn position(&self) -> Self::Position;

    /// Save position for rewinding (solver support)
    fn save_checkpoint(&self) -> Self::Position;

    /// Rewind to saved position
    fn rewind_to(&mut self, pos: Self::Position) -> Result<(), Self::Error>;
}
```

### Layer 3: Type Construction (Identical)

All formats use the same `Partial` API:
- `begin_field(name)` / `begin_nth_field(idx)`
- `set(value)` / `parse_from_str(s)`
- `set_default()` / `set_nth_field_to_default(idx)`
- `end()`
- `build()` / `materialize()`

## The Element Enumeration

The backend exposes a simple element model:

```rust
pub enum Element {
    /// Start of object/array/element
    StructureStart(StructureKind),

    /// Field name in object / key in map
    FieldName,

    /// Scalar value
    Scalar,

    /// Null/None value
    Null,

    /// End of object/array/element
    StructureEnd,

    /// End of input
    Eof,
}

pub enum StructureKind {
    Object,     // JSON object, YAML mapping, XML element with children
    Array,      // JSON array, YAML sequence
    Tuple,      // Fixed-size array
}

pub struct Scalar {
    /// Raw content
    pub content: String,

    /// Inferred type (for self-describing formats)
    /// None for XML/KDL where everything is text
    pub inferred_type: Option<ValueType>,

    /// Source location
    pub span: SourceSpan,
}

pub enum ValueType {
    String,
    Number,
    Boolean,
    // Self-describing formats can distinguish these
}
```

## Self-Describing vs Non-Self-Describing

### Self-Describing Formats (JSON, YAML, Value)

Type information is in the format:
- `123` is a number
- `"123"` is a string
- `true` is a boolean
- `null` is null

**Advantage**: Better untagged enum disambiguation because we can see value types before committing.

**Backend behavior**: `get_scalar()` returns `Scalar { content: "123", inferred_type: Some(Number) }`

### Non-Self-Describing Formats (XML, KDL)

Everything is text or nodes:
- `<count>123</count>` is just text "123"
- `node 123` is just text "123"

Types are determined by schema alone.

**Backend behavior**: `get_scalar()` returns `Scalar { content: "123", inferred_type: None }`

## The Universal Deserialization Loop

With this abstraction, ALL formats can use the same high-level deserialization logic:

```rust
pub fn deserialize_struct<B: DeserializerBackend>(
    mut partial: Partial,
    backend: &mut B,
    struct_def: &StructType,
) -> Result<Partial> {
    // Enter object/element
    backend.expect(Element::StructureStart(StructureKind::Object))?;
    backend.enter_structure()?;

    let mut fields_set = vec![false; struct_def.fields.len()];

    // Process fields
    while backend.peek()? != Element::StructureEnd {
        // Get field name
        backend.expect(Element::FieldName)?;
        let field_name = backend.get_field_name()?;
        backend.advance()?;

        // Find matching field
        let (idx, field) = find_field_by_key(
            struct_def.fields,
            &field_name,
            &resolve_field_name,
        ).ok_or_else(|| unknown_field_error(&field_name))?;

        // Skip if needed
        if field.should_skip_deserializing() {
            skip_value(backend)?;
            continue;
        }

        // Deserialize field value
        partial = partial.begin_field(field.name)?;
        partial = deserialize_value(partial, backend, field.shape())?;
        partial = partial.end()?;

        fields_set[idx] = true;
    }

    backend.exit_structure()?;

    // Apply defaults
    partial = apply_defaults(partial, struct_def, &fields_set)?;

    Ok(partial)
}
```

**This same code works for JSON, YAML, XML, KDL, and TOML** because the backend abstracts the parsing.

## Backend Implementations

### JSON Backend (Token-Based)

```rust
struct JsonBackend<'input> {
    adapter: SliceAdapter<'input>,
    peeked: Option<Token<'input>>,
}

impl DeserializerBackend for JsonBackend<'_> {
    type Position = usize; // Byte offset

    fn peek(&self) -> Result<Element> {
        match self.peek_token()? {
            Token::ObjectStart => Ok(Element::StructureStart(StructureKind::Object)),
            Token::ArrayStart => Ok(Element::StructureStart(StructureKind::Array)),
            Token::String(_) if in_object_key_position => Ok(Element::FieldName),
            Token::String(_) | Token::U64(_) | Token::I64(_) | ... => Ok(Element::Scalar),
            Token::Null => Ok(Element::Null),
            Token::ObjectEnd | Token::ArrayEnd => Ok(Element::StructureEnd),
            Token::Eof => Ok(Element::Eof),
        }
    }

    fn get_scalar(&mut self) -> Result<Scalar> {
        let token = self.next_token()?;
        Ok(Scalar {
            content: token.as_string(),
            inferred_type: Some(match token {
                Token::String(_) => ValueType::String,
                Token::U64(_) | Token::I64(_) | Token::F64(_) => ValueType::Number,
                Token::True | Token::False => ValueType::Boolean,
                _ => return Err(unexpected_token_error(token)),
            }),
            span: token.span,
        })
    }

    fn save_checkpoint(&self) -> usize {
        self.adapter.current_offset()
    }

    fn rewind_to(&mut self, offset: usize) {
        self.adapter.seek_to(offset);
        self.peeked = None;
    }
}
```

### XML Backend (Event-Based)

```rust
struct XmlBackend {
    events: Vec<OwnedEvent>,
    pos: usize,
}

impl DeserializerBackend for XmlBackend {
    type Position = usize; // Event index

    fn peek(&self) -> Result<Element> {
        match &self.events[self.pos] {
            OwnedEvent::Start { .. } | OwnedEvent::Empty { .. } => {
                Ok(Element::StructureStart(StructureKind::Object))
            }
            OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                Ok(Element::Scalar)
            }
            OwnedEvent::End { .. } => Ok(Element::StructureEnd),
            OwnedEvent::Eof => Ok(Element::Eof),
        }
    }

    fn get_scalar(&mut self) -> Result<Scalar> {
        match &self.events[self.pos] {
            OwnedEvent::Text { content, span } | OwnedEvent::CData { content, span } => {
                self.pos += 1;
                Ok(Scalar {
                    content: content.clone(),
                    inferred_type: None, // XML is not self-describing
                    span: *span,
                })
            }
            _ => Err(unexpected_event_error(&self.events[self.pos])),
        }
    }

    fn save_checkpoint(&self) -> usize {
        self.pos
    }

    fn rewind_to(&mut self, pos: usize) {
        self.pos = pos;
    }
}
```

### KDL Backend (DOM-Based, for now)

```rust
struct KdlBackend<'doc> {
    nodes: &'doc [KdlNode],
    current: usize,
    // Current node state for properties/arguments
    current_node: Option<&'doc KdlNode>,
    property_idx: usize,
}

impl DeserializerBackend for KdlBackend<'_> {
    type Position = (usize, usize); // (node index, property index)

    fn peek(&self) -> Result<Element> {
        if let Some(node) = self.current_node {
            // Processing properties within a node
            if self.property_idx < node.entries().len() {
                return Ok(Element::FieldName);
            }
        }

        if self.current < self.nodes.len() {
            Ok(Element::StructureStart(StructureKind::Object))
        } else {
            Ok(Element::Eof)
        }
    }

    fn get_scalar(&mut self) -> Result<Scalar> {
        let node = self.current_node.ok_or(no_current_node_error())?;
        let entry = &node.entries()[self.property_idx];

        self.property_idx += 1;

        Ok(Scalar {
            content: entry.value().to_string(),
            inferred_type: None, // KDL is not self-describing
            span: entry.span(),
        })
    }

    fn save_checkpoint(&self) -> (usize, usize) {
        (self.current, self.property_idx)
    }

    fn rewind_to(&mut self, (node_idx, prop_idx): (usize, usize)) {
        self.current = node_idx;
        self.property_idx = prop_idx;
        // Restore current_node
    }
}
```

**Note**: When KDL moves to event-based parsing, the backend implementation changes but the trait stays the same.

### Value Backend (Already Semantic)

```rust
struct ValueBackend<'v> {
    value: &'v Value,
    // Stack for navigating nested structures
}

impl DeserializerBackend for ValueBackend<'_> {
    type Position = ValuePath; // Path into Value tree

    fn peek(&self) -> Result<Element> {
        match self.value {
            Value::Object(_) => Ok(Element::StructureStart(StructureKind::Object)),
            Value::Array(_) => Ok(Element::StructureStart(StructureKind::Array)),
            Value::String(_) | Value::Number(_) | Value::Bool(_) => Ok(Element::Scalar),
            Value::Null => Ok(Element::Null),
        }
    }

    fn get_scalar(&mut self) -> Result<Scalar> {
        Ok(Scalar {
            content: self.value.to_string(),
            inferred_type: Some(match self.value {
                Value::String(_) => ValueType::String,
                Value::Number(_) => ValueType::Number,
                Value::Bool(_) => ValueType::Boolean,
                _ => return Err(not_scalar_error()),
            }),
            span: SourceSpan::default(),
        })
    }

    // Value backend doesn't need rewind - just clone
    fn save_checkpoint(&self) -> ValuePath {
        self.current_path.clone()
    }

    fn rewind_to(&mut self, path: ValuePath) {
        self.current_path = path;
    }
}
```

## Handling Format-Specific Features

### XML: Attributes vs Elements

XML backend needs to track attribute mode:

```rust
struct XmlBackend {
    events: Vec<OwnedEvent>,
    pos: usize,
    attributes: Vec<(String, String)>, // Extracted from Start event
    attribute_mode: bool, // True when processing attributes
}

impl XmlBackend {
    fn enter_attribute_mode(&mut self) {
        // Extract attributes from current Start event
        self.attribute_mode = true;
    }

    fn exit_attribute_mode(&mut self) {
        self.attribute_mode = false;
        self.attributes.clear();
    }
}
```

Format-specific routing happens in the struct deserializer:

```rust
// In deserialize_struct for XML
for field in struct_def.fields {
    if field.is_xml_attribute() {
        backend.enter_attribute_mode();
        // Process attributes
        backend.exit_attribute_mode();
    } else if field.is_xml_element() {
        // Process as child elements
    }
}
```

### KDL: Properties vs Children vs Arguments

Similar pattern - the backend tracks what mode it's in:

```rust
enum KdlMode {
    Properties,  // key=value pairs
    Arguments,   // positional values
    Children,    // child nodes
}

struct KdlBackend<'doc> {
    mode: KdlMode,
    // ...
}
```

## Solver Integration

The two-pass pattern works naturally with the backend abstraction:

```rust
pub fn deserialize_with_solver<B: DeserializerBackend>(
    mut partial: Partial,
    backend: &mut B,
) -> Result<Partial> {
    // Pass 1: Scan keys
    let checkpoint = backend.save_checkpoint();
    let mut keys = Vec::new();

    backend.enter_structure()?;
    while backend.peek()? != Element::StructureEnd {
        backend.expect(Element::FieldName)?;
        keys.push(backend.get_field_name()?);
        backend.advance()?;
        skip_value(backend)?;
    }
    backend.exit_structure()?;

    // Build solver and resolve
    let schema = Schema::build_auto(partial.shape())?;
    let mut solver = Solver::new(&schema);
    for key in &keys {
        solver.see_key(key);
    }
    let resolution = solver.finish()?;

    // Pass 2: Rewind and deserialize with resolution
    backend.rewind_to(checkpoint)?;
    partial = deserialize_with_resolution(partial, backend, &resolution)?;

    Ok(partial)
}
```

**This same code works for JSON, YAML, XML, KDL** because rewinding is abstracted.

## Flatten Support

Path navigation with the backend:

```rust
pub fn deserialize_flattened_field<B: DeserializerBackend>(
    mut partial: Partial,
    backend: &mut B,
    field_info: &FieldInfo,
) -> Result<Partial> {
    // Navigate to field using path
    for segment in field_info.path.segments() {
        partial = partial.begin_field(segment.field_name)?;
        if let Some(variant_name) = segment.variant_name {
            partial = partial.select_variant_named(variant_name)?;
        }
    }

    // Deserialize value at this path
    partial = deserialize_value(partial, backend, field_info.value_shape)?;

    // Unwind path
    for _ in field_info.path.segments() {
        partial = partial.end()?;
    }

    Ok(partial)
}
```

## Benefits of This Abstraction

### 1. **Code Reuse**

Instead of 5 separate implementations of:
- Struct deserialization
- Enum deserialization
- Field matching
- Default application
- Solver integration
- Flatten support

We have **ONE implementation** that works for all formats via the backend trait.

### 2. **Format-Specific Logic Isolated**

Each format only needs to implement:
- Token/event/DOM traversal (Layer 1)
- Backend trait methods (Layer 2)

Everything else is shared (Layer 3).

### 3. **Future-Proof**

When KDL moves to event-based parsing:
- Only `KdlBackend` implementation changes
- All high-level logic stays the same
- No changes to other formats

### 4. **Testing**

We can test the high-level deserialization logic once with a mock backend:

```rust
struct MockBackend {
    elements: Vec<Element>,
    scalars: HashMap<usize, Scalar>,
    // ...
}

#[test]
fn test_struct_deserialization() {
    let backend = MockBackend::new(vec![
        Element::StructureStart(StructureKind::Object),
        Element::FieldName,
        Element::Scalar,
        Element::FieldName,
        Element::Scalar,
        Element::StructureEnd,
    ]);

    // Test deserialize_struct logic without format-specific code
}
```

### 5. **New Format Support**

Adding a new format requires:
1. Implement `DeserializerBackend` trait (~200 lines)
2. All deserialization logic works automatically

## What About Serialization?

Similar abstraction possible:

```rust
pub trait SerializerBackend {
    fn begin_structure(&mut self, kind: StructureKind) -> Result<()>;
    fn end_structure(&mut self) -> Result<()>;
    fn write_field_name(&mut self, name: &str) -> Result<()>;
    fn write_scalar(&mut self, value: &str, hint: Option<ValueType>) -> Result<()>;
    fn write_null(&mut self) -> Result<()>;
}
```

The serialize logic becomes:

```rust
pub fn serialize_struct<B: SerializerBackend>(
    peek: Peek,
    backend: &mut B,
) -> Result<()> {
    backend.begin_structure(StructureKind::Object)?;

    for (field_name, field_peek) in peek.fields_with_names(&resolve_field_name) {
        backend.write_field_name(field_name)?;
        serialize_value(field_peek, backend)?;
    }

    backend.end_structure()?;
    Ok(())
}
```

## Migration Path

### Phase 1: Prototype the Backend Trait

Create `facet-format-core` with:
- `DeserializerBackend` trait definition
- `Element`, `Scalar`, `StructureKind` enums
- Shared deserialization functions

### Phase 2: Implement One Backend

Pick JSON (most mature) and implement `JsonBackend`.

Prove that the unified `deserialize_struct`, `deserialize_enum`, etc. work.

### Phase 3: Migrate Other Formats

One by one:
- Implement backend for YAML
- Implement backend for XML
- Implement backend for KDL
- Implement backend for Value

Each migration removes ~1000-2000 lines of duplicated logic.

### Phase 4: Serialization

Apply same pattern to serialization.

## Open Questions

1. **How to handle format-specific errors?**
   - Backend returns `Result<T, Self::Error>`
   - Shared code converts to generic error
   - Or: Use unified error type with format-specific variants

2. **Performance impact?**
   - Trait dispatch overhead (can be monomorphized away)
   - Extra allocations for `Scalar` struct
   - Benchmark and optimize

3. **How to handle attributes (XML) vs properties (KDL)?**
   - Backend has mode switching
   - Or: Add `enter_attribute_mode()` / `enter_property_mode()` to trait

4. **Streaming vs buffering?**
   - Checkpoint/rewind requires buffering for true streaming
   - But: Most formats already buffer (events, tokens)
   - KDL/Value: No streaming anyway

## Conclusion

The unified abstraction is:

**A backend trait that provides format-agnostic element traversal, with shared high-level deserialization logic built on top.**

This is NOT:
- A visitor pattern (we're not traversing with callbacks)
- A pure event model (DOM-based formats are fine)
- A replacement for Partial (Partial is the abstraction we build on)

This IS:
- A thin layer between format parsing and type construction
- A way to share 80% of deserialization logic
- Future-proof for when XML/KDL move to streaming
- Testable independently of format parsing

The key insight: **Partial already separates type construction from parsing. We just need to standardize the parsing interface.**
