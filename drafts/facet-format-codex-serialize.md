## Facet Format Codex – Serializer API

> **Note (Dec 2025):** This is an older “end-state” sketch. For the current design + implementation status,
> see `drafts/facet-format.md`.

This proposal specifies the *single serializer touch point* every format crate implements. Everything else (field ordering, rename/tag synthesis, solver hooks) lives in shared helpers so no format re-implements the rules.

### 1. Core Trait

```rust
pub trait FormatSerializer {
    type Ok;
    type Error;

    /// JSON/TOML/YAML use this default (it calls `shared_serialize` immediately).
    /// XML/KDL override it only when they must perform format-wide bookkeeping
    /// before dispatch—for example, XML needs to establish the root element and
    /// prime its namespace/attribute buffers before the shared helper starts
    /// visiting fields—but even then they call back into `shared_serialize`
    /// afterwards so struct/map/seq/scalar dispatch remains centralized.
    fn serialize_value(&mut self, value: Peek<'_, '_>) -> Result<Self::Ok, Self::Error> {
        shared_serialize(value, self)
    }

    fn serialize_struct(&mut self, layout: SynthesizedStruct<'_>) -> Result<Self::Ok, Self::Error>;
    fn serialize_map(&mut self, entries: SynthesizedMap<'_>) -> Result<Self::Ok, Self::Error>;
    fn serialize_seq(&mut self, entries: SynthesizedSeq<'_>) -> Result<Self::Ok, Self::Error>;
    fn serialize_scalar(&mut self, scalar: SynthesizedScalar<'_>) -> Result<Self::Ok, Self::Error>;
}
```

Formats only implement `serialize_value` by delegating to the shared helpers that construct synthesized layouts and call the struct/map/seq/scalar methods. A JSON/TOML/YAML serializer never sees enums or tagging logic; XML/KDL see `SynthesizedStruct` whose `FieldMetadata` includes their annotations (attribute/element/property/text).

### 2. Synthesized layouts

```rust
pub struct SynthesizedStruct<'a> {
    pub shape: &'static Shape,
    pub fields: &'a [SynthesizedField<'a>],
    /// Formats that need to reorder fields (XML attributes before children, etc.)
    /// can scan this slice, partition references however they need, and then emit
    /// directly from the original `Peek`s—no intermediate allocation required.
}

pub enum SynthesizedField<'a> {
    Real {
        key: &'static str,
        field: &'static Field,
        value: Peek<'a, 'a>,
        location: FieldLocation,
    },
    SyntheticTag { key: &'static str, value: &'static str },
    SyntheticContent { key: &'static str, value: Peek<'a, 'a> },
}
```

- JSON just iterates and writes every field as `"key": value`.
- XML uses `location` (`Attribute`, `Text`, `Child`, etc.) to emit fields exactly as annotations dictate (per `drafts/facet-format-proposal.md` Section 5).
- Enum tagging is already synthesized; the serializer only knows about struct fields.

Similarly for sequences/maps:

```rust
pub struct SynthesizedSeq<'a> {
    pub iter: facet_reflect::PeekListLikeIter<'a, 'a>,
}

pub struct SynthesizedMap<'a> {
    pub iter: facet_reflect::PeekMapIter<'a, 'a>,
}
```

### 3. JSON implementation sketch

```rust
impl FormatSerializer for JsonSerializer<'_, W> {
    type Ok = ();
    type Error = SerializeError;

    fn serialize_value(&mut self, value: Peek<'_, '_>) -> Result<(), SerializeError> {
        // shared helper peels proxies, transparent types, RawJson, etc.,
        // handing back a Synthesized* structure
        shared_serialize(value, self)
    }

    fn serialize_struct(&mut self, layout: SynthesizedStruct<'_>) -> Result<(), SerializeError> {
        self.writer.write(b"{");
        let mut first = true;
        for field in layout.fields {
            if matches!(field, SynthesizedField::Real { .. } | SynthesizedField::SyntheticTag { .. } | SynthesizedField::SyntheticContent { .. }) {
                if !first { self.writer.write(b","); }
                first = false;
                match field {
                    SynthesizedField::Real { key, value, .. } => {
                        write_json_string(&mut self.writer, key);
                        self.writer.write(b":");
                        shared_serialize(value, self)?;
                    }
                    SynthesizedField::SyntheticTag { key, value } => {
                        write_json_string(&mut self.writer, key);
                        self.writer.write(b":");
                        write_json_string(&mut self.writer, value);
                    }
                    SynthesizedField::SyntheticContent { key, value } => {
                        write_json_string(&mut self.writer, key);
                        self.writer.write(b":");
                        shared_serialize(value, self)?;
                    }
                }
            }
        }
        self.writer.write(b"}");
        Ok(())
    }

    fn serialize_map(&mut self, entries: SynthesizedMap<'_>) -> Result<(), SerializeError> { ... }

    fn serialize_seq(&mut self, entries: SynthesizedSeq<'_>) -> Result<(), SerializeError> { ... }

    fn serialize_scalar(&mut self, scalar: SynthesizedScalar<'_>) -> Result<(), SerializeError> { ... }
}
```

### 4. XML/KDL implementation differences
- XML inspects `FieldLocation` in `SynthesizedField::Real` to emit attributes before child nodes. Synthetic tag/content fields respect `FieldLocation` (tag always attribute, content always child/text depending on variant).  
- KDL uses `FieldLocation::Property` vs `FieldLocation::Argument` vs `FieldLocation::Child` to render nodes exactly like the existing serializers.  
- Both rely on the shared runtime rename resolver (issue #1127) to ensure `key` holds the final serialized name.

### 5. Shared helper responsibilities
- Evaluate skip rules (`skip_serializing_if`, `skip_unless_truthy`), flatten, proxy, transparent types.
- Build `SynthesizedStruct` for structs/enums, `SynthesizedSeq` for sequences, `SynthesizedMap` for maps.  
- Provide `SynthesizedScalar` with typed representation (string/number/bool bytes) so formats can choose how to output (e.g., XML always stringifies, JSON writes typed).  
- Satisfy solver requirements by deriving flattened field lists before calling format implementations.

### 6. Performance considerations
- Shared helper hands formats the *existing* iterators (`PeekListLikeIter`, `PeekMapIter`) instead of collecting into new buffers, so sequences/maps stream entries exactly the way today’s serializers do; the only synthesized work is for tag/content fields on enums.  
- Format writes remain direct (no dynamic dispatch inside loops beyond trait object chosen at compile time).  
- JSON’s RawJson bypass stays intact (shared helper detects `RawJson` and writes directly).  
- Pretty-printers manipulate `FormatSerializer` to add whitespace; this design doesn’t interfere.

### 7. coverage vs requirements
- Field annotations (XML/KDL) handled via `FieldLocation` and `FieldHints`.  
- Runtime rename chain applied before building `SynthesizedField`.  
- Enum tagging synthesized entirely (adjacent/internal/external/untagged).  
- Flatten + solver (issue #1127) happen before calling format serializer, so writer only sees the final layout.  
- Byte arrays, proxies, metadata fields are filtered by shared helper, so format-specific code stays simple.

This doc focuses on serialization; the companion deserialization doc defines the evidence-aware parser trait.
