# Can Facet Have ONE Serializer and ONE Deserializer Trait Like Serde?

> **Status (Dec 2025):** This document is primarily a proposal/rationale and includes “end-state” sketches.
> For the current design + implementation status of the `facet-format*` work (including what is and isn’t implemented yet),
> see `drafts/facet-format.md`.

**The Central Question:** Can we design ONE `FacetSerializer` trait and ONE `FacetDeserializer` trait that works for ALL formats (JSON, YAML, XML, KDL, TOML, etc.) while maintaining:
- Full flexibility of format-specific annotations
- Solver integration for untagged enums and flatten
- Runtime field name resolution
- Self-describing vs non-self-describing format support

**Answer: Yes, but it requires ONE critical addition to serde's model: Evidence-based deserialization.**

---

## PART 1: Understanding Why Serde Works

### Serde's Genius: The 29-Type Data Model

Serde works because it has a **data model** that sits between Rust types and format bytes:

```
Rust Type ←→ Serde Data Model ←→ Format Bytes
   (app)     (abstraction layer)     (wire)
```

The data model has ~29 types:
- Primitives: bool, i8-i128, u8-u128, f32, f64, char, string
- Composites: unit, option, sequence, tuple, map, struct, enum

**Why this works:**
- Every Rust type maps to exactly ONE method call on the Serializer
- Every format decides how to represent those 29 types
- The abstraction is clean and bidirectional

### What Serde Doesn't Do (That Facet Needs)

1. **Solver-based disambiguation** - Serde requires you to know the exact type upfront
2. **Flatten with complex paths** - Serde's flatten is simple field merging
3. **Format-specific field semantics** - XML attributes vs elements, KDL arguments vs properties
4. **Runtime field name resolution** - Serde field names are compile-time constants
5. **Evidence-based type selection** - Serde propagates constraints downward, Facet needs evidence upward

---

## PART 2: All Problems We Need To Solve

### Problem Categories

#### A. Format Diversity
- **Self-describing** (JSON, YAML): Types are in the data (`"123"` is string, `123` is number)
- **Schema-driven** (XML, KDL): Everything is text, types come from schema
- **Binary** (MessagePack, Postcard): Fully typed at byte level

#### B. Structural Differences
- **JSON/YAML/TOML**: Objects as maps, arrays as sequences
- **XML**: Elements, attributes, text content, namespaces
- **KDL**: Nodes with arguments, properties, children
- **CSV**: Row-based, flat only

#### C. Annotation Requirements
- **JSON/YAML/TOML**: Minimal - just `#[facet(rename)]`, `#[facet(flatten)]`, etc.
- **XML/KDL**: **EVERY field MUST be annotated** - attribute vs element vs property vs argument vs child

#### D. Solver Integration
- **Untagged enums**: Need to scan all fields before selecting variant
- **Flatten**: Need to resolve which nested struct owns each field
- **Two-pass**: Scan → solve → deserialize

#### E. Runtime Capabilities
- **Field name resolution**: Six-level priority chain (crate defaults, container attributes, field attributes)
- **Evidence propagation**: "I see field X, so this must be variant Y"
- **Checkpoint/rewind**: Ability to scan then re-parse

---

## PART 3: All Differences Between Formats

### Structural Mapping

| Concept | JSON | XML | KDL |
|---------|------|-----|-----|
| **Struct** | Object `{k: v}` | Element with children or attributes | Node with properties/children |
| **Field** | Key-value pair | Element `<field>` OR attribute `attr=` | Property `prop=` OR argument OR child node |
| **Array** | Array `[...]` | Repeated elements | Multiple child nodes OR arguments |
| **Enum** | `{"Variant": data}` | Element name = variant | Node name = variant |
| **Scalar** | Typed: string/number/bool/null | Text content (always string) | Typed: string/number/bool/null |
| **Namespace** | N/A | `xmlns`, prefixes | N/A |

### Critical Differences

1. **Field Routing** - XML/KDL need to know WHERE to put data:
   - XML: `#[facet(xml::attribute)]` vs `#[facet(xml::element)]` vs `#[facet(xml::text)]`
   - KDL: `#[facet(kdl::property)]` vs `#[facet(kdl::argument)]` vs `#[facet(kdl::child)]`

2. **Self-Description** - JSON/YAML can peek at value types, XML/KDL cannot

3. **Rewind** - Solver needs to scan then re-parse:
   - JSON: Offset-based
   - XML/YAML: Event index-based
   - KDL: Tree-based (already parsed)
   - TOML: Deferred mode (no rewind needed)

---

## PART 4: The Unified Trait Design

### The Key Insight From Serde

Serde has TWO roles:
1. **`Serialize` trait** (on data) - "Here's how to serialize me"
2. **`Serializer` trait** (on format) - "Here's how I represent the data model"

Facet needs the SAME separation, but with **one addition**:

3. **Evidence collection** - Formats must answer "What fields are present?" not just "Give me this field"

### The Critical Insight: Synthesis Layer

Before showing the trait, we need to understand **field synthesis**:

**Problem:** Enum tagging strategies create synthetic fields that don't exist in memory:

```rust
#[facet(tag = "type")]
enum Message {
    Text { content: String }
}

// Memory: variant="Text", field="content"
// Serialized: {"type": "Text", "content": "..."}
//              ^^^^^^^^^^^^^ synthetic field!
```

**Solution:** The serializer should ONLY see a struct with fields. We synthesize the tag fields BEFORE serialization.

```
Enum Variant (in memory)
         ↓
   [Synthesis Layer] ← Insert synthetic fields based on tag strategy
         ↓
Virtual Struct { tag: "Variant", ...real_fields }
         ↓
   [Serializer] ← Sees uniform struct, writes fields
         ↓
    JSON/XML/KDL output
```

### The Facet Serializer Trait

```rust
pub trait FacetSerializer {
    type Ok;
    type Error;

    // === PRIMITIVES ===
    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error>;
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error>;
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error>;
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error>;
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error>;
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error>;

    // === SPECIAL ===
    fn serialize_none(self) -> Result<Self::Ok, Self::Error>;
    fn serialize_some<T: ?Sized + Facet>(self, value: &T) -> Result<Self::Ok, Self::Error>;
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error>;

    // === SEQUENCES ===
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error>;

    // === MAPS ===
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error>;

    // === STRUCTS (INCLUDING SYNTHESIZED ENUMS!) ===
    // NOTE: Enums are synthesized into structs before this is called
    fn serialize_struct(
        self,
        name: &'static str,
        shape: &'static Shape,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error>;

    // === ASSOCIATED TYPES FOR COMPOUND SERIALIZATION ===
    type SerializeSeq: SerializeSeq<Ok = Self::Ok, Error = Self::Error>;
    type SerializeMap: SerializeMap<Ok = Self::Ok, Error = Self::Error>;
    type SerializeStruct: SerializeStruct<Ok = Self::Ok, Error = Self::Error>;
}

// NOTE: No serialize_variant or SerializeVariant!
// Enums are synthesized into structs, so serializer only sees struct serialization

// Compound serialization helpers
pub trait SerializeStruct {
    type Ok;
    type Error;

    fn serialize_field<T: ?Sized + Facet>(
        &mut self,
        key: &'static str,
        field: &Field,           // Field metadata (may be synthetic!)
        value: &T,
    ) -> Result<(), Self::Error>;

    fn end(self) -> Result<Self::Ok, Self::Error>;
}
```

### Field Synthesis Details

When serializing an enum, the framework creates a **virtual struct view** with synthesized fields:

```rust
/// Represents a field that may or may not exist in memory
pub enum FieldSource<'facet> {
    /// Real field from the variant
    Real {
        field: &'facet Field,
        peek: Peek<'facet>,
    },
    /// Synthetic field (tag or content wrapper)
    Synthetic {
        kind: SyntheticKind,
    },
}

pub enum SyntheticKind {
    /// Contains variant discriminant as string
    Tag { value: &'static str },
    /// Wraps variant data
    Content,
}

/// Iterator over all fields (real + synthetic) for a variant
pub struct SynthesizedFields<'facet> {
    peek_enum: PeekEnum<'facet>,
    state: FieldIterState,
}

impl<'facet> Iterator for SynthesizedFields<'facet> {
    type Item = (&'static str, FieldSource<'facet>);

    fn next(&mut self) -> Option<Self::Item> {
        let shape = self.peek_enum.shape();

        match (shape.get_tag_attr(), shape.get_content_attr()) {
            // Internally tagged: yield tag, then real fields
            (Some(tag_name), None) => {
                if self.state == FieldIterState::Initial {
                    self.state = FieldIterState::RealFields(0);
                    return Some((tag_name, FieldSource::Synthetic {
                        kind: SyntheticKind::Tag {
                            value: self.peek_enum.active_variant().name
                        }
                    }));
                }
                // Then yield real fields...
            }

            // Adjacently tagged: yield tag, then content wrapper
            (Some(tag_name), Some(content_name)) => {
                match self.state {
                    FieldIterState::Initial => {
                        self.state = FieldIterState::SyntheticContent;
                        Some((tag_name, FieldSource::Synthetic {
                            kind: SyntheticKind::Tag {
                                value: self.peek_enum.active_variant().name
                            }
                        }))
                    }
                    FieldIterState::SyntheticContent => {
                        self.state = FieldIterState::Done;
                        Some((content_name, FieldSource::Synthetic {
                            kind: SyntheticKind::Content
                        }))
                    }
                    _ => None
                }
            }

            // Untagged or externally tagged: just real fields
            _ => {
                // Yield real fields from peek_enum.fields_for_serialize()
            }
        }
    }
}
```

### How Serialization Works With Synthesis

**Internally Tagged Example:**

```rust
#[derive(Facet)]
#[facet(tag = "type")]
enum Message {
    Text { content: String }
}

let msg = Message::Text { content: "Hello".into() };
```

**Step 1: Framework creates synthesized field iterator:**

```rust
let peek_enum = Peek::new(&msg).into_enum();
let fields = peek_enum.synthesized_fields();

// fields yields:
// 1. ("type", Synthetic(Tag("Text")))
// 2. ("content", Real(field, peek))
```

**Step 2: Shared serialize_enum_impl calls serializer:**

```rust
pub fn serialize_enum_impl<S: FacetSerializer>(
    peek_enum: PeekEnum,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let fields: Vec<_> = peek_enum.synthesized_fields().collect();

    let mut state = serializer.serialize_struct(
        peek_enum.shape().type_identifier,
        peek_enum.shape(),
        fields.len(),
    )?;

    for (field_name, field_source) in fields {
        match field_source {
            FieldSource::Real { field, peek } => {
                state.serialize_field(field_name, field, peek)?;
            }
            FieldSource::Synthetic { kind: SyntheticKind::Tag { value } } => {
                state.serialize_field(
                    field_name,
                    &SYNTHETIC_TAG_FIELD,  // Dummy field metadata
                    value,                   // Variant name as string
                )?;
            }
            FieldSource::Synthetic { kind: SyntheticKind::Content } => {
                // Recursively serialize variant content
                state.serialize_field(
                    field_name,
                    &SYNTHETIC_CONTENT_FIELD,
                    peek_enum.variant_content(),
                )?;
            }
        }
    }

    state.end()
}
```

**Step 3: Format sees uniform struct:**

```rust
impl FacetSerializer for JsonSerializer {
    // ...

    fn serialize_struct(...) -> Result<Self::SerializeStruct, ...> {
        // Just open object: {
        Ok(JsonStructSerializer { writer, first: true })
    }
}

impl SerializeStruct for JsonStructSerializer {
    fn serialize_field(&mut self, key: &str, field: &Field, value: &T) {
        if !self.first { self.writer.write(b","); }
        self.first = false;

        // Write key (could be "type", "content", or real field name)
        write_json_string(&mut self.writer, key);
        self.writer.write(b":");

        // Serialize value (recursively)
        value.serialize(&mut JsonSerializer::new(self.writer))?;
    }

    fn end(self) {
        // Close object: }
        self.writer.write(b"}");
    }
}
```

**Result:** `{"type":"Text","content":"Hello"}`

The serializer never knew it was an enum! It just saw a struct with two fields.

### Benefits of Synthesis

1. **Serializers are simpler** - No enum-specific logic, just struct serialization
2. **Works uniformly** - JSON, XML, KDL all serialize the same synthetic struct
3. **Tagging is centralized** - Logic lives in `facet-reflect`, not in each format
4. **Extensible** - New tagging strategies just change the synthesis, not serializers

### Externally Tagged: The Exception

Externally tagged enums create a wrapper structure:

```rust
// Externally tagged: {"Variant": content}
// This is actually a MAP with one entry, not a struct
```

For externally tagged, we use `serialize_map` instead:

```rust
pub fn serialize_enum_impl<S: FacetSerializer>(
    peek_enum: PeekEnum,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if peek_enum.shape().is_externally_tagged() {
        // Use map serialization for external tagging
        let variant = peek_enum.active_variant();
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(variant.name, peek_enum.variant_content())?;
        map.end()
    } else {
        // Use struct synthesis for internal/adjacent tagging
        serialize_synthesized_struct(peek_enum, serializer)
    }
}
```

**What's different from serde:**

1. **`serialize_struct` and `serialize_variant` get metadata** - The `Shape` and `Variant` provide field annotations
2. **`serialize_field` gets `Field` metadata** - Format can check `field.is_xml_attribute()` vs `field.is_xml_element()`
3. **Field names resolved at runtime** - The `key` parameter is computed using the six-level priority chain

### The Facet Deserializer Trait

This is where the major innovation happens:

```rust
pub trait FacetDeserializer<'de> {
    type Error;

    // === PRIMITIVES (same as serde) ===
    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    // ... (same for all primitives)

    // === HINT-DRIVEN (same as serde) ===
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        shape: &'static Shape,
        fields: &'static [Field],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        shape: &'static Shape,
        variants: &'static [Variant],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    // === SELF-DESCRIBING (same as serde) ===
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>;

    // === NEW: EVIDENCE COLLECTION (THIS IS THE KEY!) ===

    /// Probe available fields without consuming input
    /// Used by solver to determine which variant matches
    fn probe_fields(&mut self) -> Result<Vec<&'de str>, Self::Error>;

    /// Check if a specific field exists
    fn has_field(&mut self, field_name: &str) -> Result<bool, Self::Error>;

    /// Get field value type hint (for self-describing formats)
    /// Returns None for schema-driven formats (XML, KDL)
    fn peek_field_type(&mut self, field_name: &str) -> Result<Option<ValueType>, Self::Error>;

    /// Save current position for rewinding
    fn checkpoint(&self) -> Self::Checkpoint;

    /// Rewind to saved position (for solver two-pass)
    fn rewind(&mut self, checkpoint: Self::Checkpoint) -> Result<(), Self::Error>;

    type Checkpoint: Clone;
}

/// Value type hints for self-describing formats
#[derive(Debug, Clone, Copy)]
pub enum ValueType {
    Null,
    Bool,
    Number,
    String,
    Bytes,
    Sequence,
    Map,
}
```

**The critical addition:**

The `probe_fields()`, `has_field()`, and `peek_field_type()` methods enable **evidence-based disambiguation**.

This is what serde doesn't have and what facet NEEDS for solver integration.

---

## PART 5: How This Solves All The Problems

### Problem 1: Format-Specific Annotations (XML/KDL)

**In serialization:**

```rust
impl SerializeStruct for XmlSerializer {
    fn serialize_field<T: Facet>(
        &mut self,
        key: &'static str,
        field: &'static Field,
        value: &T,
    ) -> Result<(), Self::Error> {
        // Check field annotation
        if field.has_attr(Some("xml"), "attribute") {
            // Serialize as XML attribute
            self.write_attribute(key, value)?;
        } else if field.has_attr(Some("xml"), "element") {
            // Serialize as XML element
            self.write_element(key, value)?;
        } else if field.has_attr(Some("xml"), "text") {
            // Serialize as text content
            self.write_text(value)?;
        } else {
            return Err("XML requires explicit field annotation".into());
        }
        Ok(())
    }
}
```

**In deserialization:**

```rust
impl<'de> FacetDeserializer<'de> for XmlDeserializer {
    fn deserialize_struct<V>(
        self,
        name: &'static str,
        shape: &'static Shape,
        fields: &'static [Field],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        // Separate fields by annotation
        let attr_fields: Vec<_> = fields.iter()
            .filter(|f| f.has_attr(Some("xml"), "attribute"))
            .collect();

        let element_fields: Vec<_> = fields.iter()
            .filter(|f| f.has_attr(Some("xml"), "element"))
            .collect();

        // Deserialize attributes first
        for field in attr_fields {
            // ... match against XML attributes
        }

        // Then deserialize elements
        for field in element_fields {
            // ... match against XML elements
        }

        visitor.visit_map(StructMapAccess::new(fields, values))
    }
}
```

**The key:** Format implementations can inspect `Field` metadata to route data correctly.

### Problem 2: Solver Integration (Untagged Enums & Flatten)

**The two-pass pattern:**

```rust
pub fn deserialize_with_solver<'de, D>(
    deserializer: D,
    shape: &'static Shape,
) -> Result<Value, D::Error>
where
    D: FacetDeserializer<'de>,
{
    // PASS 1: Collect evidence
    let checkpoint = deserializer.checkpoint();
    let available_fields = deserializer.probe_fields()?;

    // Build solver and feed evidence
    let schema = Schema::build_auto(shape)?;
    let mut solver = Solver::new(&schema);

    for field_name in &available_fields {
        solver.see_key(field_name);

        // For self-describing formats, also check value type
        if let Some(value_type) = deserializer.peek_field_type(field_name)? {
            // Use value type to further constrain
            // e.g., if field is Number, rule out String variants
        }
    }

    let resolution = solver.finish()?;

    // PASS 2: Deserialize with resolution
    deserializer.rewind(checkpoint)?;

    // Now deserialize knowing which variant/paths are correct
    deserialize_with_resolution(deserializer, shape, &resolution)
}
```

**Why this works:**
- `probe_fields()` - Scan without consuming (evidence collection)
- `checkpoint()` / `rewind()` - Two-pass support
- `peek_field_type()` - Self-describing format advantage

### Problem 3: Runtime Field Name Resolution

**During serialization:**

```rust
// In the generated Facet impl
fn serialize<S: FacetSerializer>(
    &self,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut state = serializer.serialize_struct("MyStruct", SHAPE, 3)?;

    // Resolve field name at runtime
    let field_name = resolve_field_name(
        &SHAPE.fields[0],      // Field metadata
        SHAPE,                  // Container metadata
        crate_config(),         // Crate-level config
        serializer.format(),    // Format identifier
    );

    state.serialize_field(field_name, &SHAPE.fields[0], &self.my_field)?;
    state.end()
}
```

**The six-level priority chain is applied BEFORE calling `serialize_field()`.**

### Problem 4: Self-Describing vs Schema-Driven

**Self-describing formats (JSON, YAML):**

```rust
impl<'de> FacetDeserializer<'de> for JsonDeserializer {
    fn peek_field_type(&mut self, field_name: &str) -> Result<Option<ValueType>, Self::Error> {
        // JSON can look at the token type
        match self.peek_token()? {
            Token::Null => Ok(Some(ValueType::Null)),
            Token::True | Token::False => Ok(Some(ValueType::Bool)),
            Token::String(_) => Ok(Some(ValueType::String)),
            Token::U64(_) | Token::I64(_) | Token::F64(_) => Ok(Some(ValueType::Number)),
            Token::ArrayStart => Ok(Some(ValueType::Sequence)),
            Token::ObjectStart => Ok(Some(ValueType::Map)),
            _ => Ok(None),
        }
    }
}
```

**Schema-driven formats (XML, KDL):**

```rust
impl<'de> FacetDeserializer<'de> for XmlDeserializer {
    fn peek_field_type(&mut self, field_name: &str) -> Result<Option<ValueType>, Self::Error> {
        // XML has no type information
        Ok(None)
    }
}
```

**The solver can use this:**
- For JSON: "Field `count` is a Number, so it can't be variant with `count: String`"
- For XML: "Can't use value type, must rely on field presence alone"

### Problem 5: Markup Languages (XML, KDL, potentially others)

**Lift common markup abstractions to `facet-core`:**

```rust
// In facet-core
#[derive(Debug, Clone, Copy)]
pub enum FieldLocation {
    /// Field is a key-value pair (JSON, YAML, TOML)
    KeyValue,

    /// Field is an attribute on the element/node
    Attribute,

    /// Field is a child element/node
    Child,

    /// Field is multiple child elements/nodes
    Children,

    /// Field is positional argument
    Argument,

    /// Field is named property
    Property,

    /// Field is text content
    Text,
}

// Extension trait for all formats
pub trait FieldLocationExt {
    fn field_location(&self) -> FieldLocation;
}

impl FieldLocationExt for Field {
    fn field_location(&self) -> FieldLocation {
        // Check for XML annotations
        if self.has_attr(Some("xml"), "attribute") {
            return FieldLocation::Attribute;
        }
        if self.has_attr(Some("xml"), "element") {
            return FieldLocation::Child;
        }
        if self.has_attr(Some("xml"), "elements") {
            return FieldLocation::Children;
        }
        if self.has_attr(Some("xml"), "text") {
            return FieldLocation::Text;
        }

        // Check for KDL annotations
        if self.has_attr(Some("kdl"), "property") {
            return FieldLocation::Property;
        }
        if self.has_attr(Some("kdl"), "argument") {
            return FieldLocation::Argument;
        }
        if self.has_attr(Some("kdl"), "child") {
            return FieldLocation::Child;
        }
        if self.has_attr(Some("kdl"), "children") {
            return FieldLocation::Children;
        }

        // Default for JSON/YAML/TOML
        FieldLocation::KeyValue
    }
}
```

**Now BOTH XML and KDL can use this:**

```rust
impl SerializeStruct for XmlSerializer {
    fn serialize_field(...) {
        match field.field_location() {
            FieldLocation::Attribute => self.write_attribute(key, value),
            FieldLocation::Child => self.write_child_element(key, value),
            FieldLocation::Text => self.write_text_content(value),
            _ => Err("XML doesn't support this location".into()),
        }
    }
}

impl SerializeStruct for KdlSerializer {
    fn serialize_field(...) {
        match field.field_location() {
            FieldLocation::Property => self.write_property(key, value),
            FieldLocation::Argument => self.write_argument(value),
            FieldLocation::Child => self.write_child_node(key, value),
            _ => Err("KDL doesn't support this location".into()),
        }
    }
}
```

**Benefits:**
- Shared abstraction for markup languages
- Could add HTML, SVG, or other XML-like formats easily
- Future markup formats get the same annotations

---

## PART 6: What We DON'T Lose

### ✅ Full Flexibility Maintained

- Format-specific annotations still work (via `Field` metadata)
- Solver integration preserved (via evidence collection)
- Runtime field name resolution supported (resolved before serialization)
- All current features preserved

### ✅ Solver Integration Improved

- `probe_fields()` is cleaner than current scan-skip-rewind logic
- `peek_field_type()` enables better disambiguation for self-describing formats
- `checkpoint()` / `rewind()` is explicit in the trait

### ✅ Code Reuse Maximized

Instead of duplicated implementations:

```
// Current: Each format has its own implementation
facet-json:  deserialize_struct() - 200 lines
facet-yaml:  deserialize_struct() - 200 lines
facet-xml:   deserialize_struct() - 250 lines
facet-kdl:   deserialize_struct() - 220 lines
facet-toml:  deserialize_struct() - 230 lines
```

We get:

```
// With unified trait: ONE implementation
facet-core:  deserialize_struct_impl<D: FacetDeserializer>() - 200 lines

// Each format only implements the trait:
facet-json:  impl FacetDeserializer - 150 lines (trait methods only)
facet-yaml:  impl FacetDeserializer - 150 lines
facet-xml:   impl FacetDeserializer - 180 lines (attribute/element routing)
facet-kdl:   impl FacetDeserializer - 180 lines (property/argument routing)
facet-toml:  impl FacetDeserializer - 160 lines
```

**Total savings: ~1000-1500 lines of duplicated logic**

---

## PART 7: Implementation Phases

### Phase 1: Define The Traits

Create `facet-format-core` with:
- `FacetSerializer` trait
- `FacetDeserializer` trait
- Associated traits (`SerializeStruct`, `SerializeVariant`, etc.)
- `FieldLocation` enum and extension trait

### Phase 2: Shared Implementations

Write once, use everywhere:
- `deserialize_struct_impl()` - Works for all formats via the trait
- `deserialize_enum_impl()` - Works for all formats
- `serialize_struct_impl()` - Works for all formats
- `serialize_enum_impl()` - Works for all formats
- `apply_defaults()` - Shared default application
- `resolve_field_name()` - Runtime name resolution

### Phase 3: Format Implementations

Implement the trait for each format:
1. JSON (simplest, good proof of concept)
2. YAML (similar to JSON)
3. XML (tests attribute routing)
4. KDL (tests argument/property routing)
5. TOML (tests deferred mode)

### Phase 4: Migration

Gradually replace format-specific implementations with shared ones.

---

## CONCLUSION

**Yes, we can have ONE serializer trait and ONE deserializer trait like serde.**

**The key additions needed:**

### For Serialization:
- Pass `Shape` and `Field` metadata to serializers
- Allow formats to inspect field annotations

### For Deserialization:
- Add evidence collection methods: `probe_fields()`, `has_field()`, `peek_field_type()`
- Add checkpoint/rewind support: `checkpoint()`, `rewind()`
- Pass `Shape` metadata to deserializers

**What we gain:**
- ~1000-1500 lines of code removed (duplicated logic)
- Consistent behavior across formats
- Easier to add new formats
- Better solver integration
- Runtime field name resolution built-in

**What we keep:**
- Full flexibility of format-specific annotations
- All current features (flatten, untagged, etc.)
- Solver-based disambiguation
- Self-describing vs schema-driven format support

**The unified abstraction:**

```
Data Type ←→ Facet Traits ←→ Format Bytes
   (app)    (abstraction)     (wire)
```

Just like serde, but with evidence collection for solver integration and metadata passing for format-specific routing.
