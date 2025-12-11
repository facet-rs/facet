## Facet Format Codex – Deserializer API

> **Note (Dec 2025):** This is an older “end-state” sketch. For the current design + implementation status,
> see `drafts/facet-format.md`.

Goal: provide one deserializer interface every format implements, while shared logic handles solver integration, flatten, runtime rename, and markup routing. This design uses *evidence collection* (per `drafts/facet-format-proposal.md`) so untagged enums/flatten behave like today.

### 1. Evidence-aware parser trait

```rust
pub trait FormatParser<'de> {
    type Error;

    fn next_event(&mut self) -> ParseEvent<'de>;
    fn peek_event(&mut self) -> ParseEvent<'de>;
    fn skip_value(&mut self) -> Result<(), Self::Error>;

    /// Start a solver probe. The parser remains at its current position while
    /// the returned cursor yields evidence lazily (field names, locations, type
    /// hints). As soon as the solver has enough evidence it can drop the cursor,
    /// at which point the parser automatically rewinds to the original position.
    fn begin_probe(&mut self) -> Result<ProbeCursor<'_, 'de, Self>, Self::Error>
    where
        Self: Sized;
}
```

`ParseEvent` is format-agnostic: `StructStart`, `StructEnd`, `FieldKey(&str)`, `Scalar(ScalarValue)`, `SequenceStart`, etc. DOM-based formats (XML/KDL) synthesize events from their node trees; streaming formats (JSON/TOML/YAML) emit events straight from scanners.

`FieldEvidence` carries `name`, optional value type hint, and location hint (attribute/child/text), enabling solver decisions without consuming the data.

### 2. Shared deserializer

```rust
pub struct FormatDeserializer<'de, P> {
    parser: P,
}

impl<'de, P: FormatParser<'de>> FormatDeserializer<'de, P> {
    pub fn deserialize_value(
        &mut self,
        target_shape: &'static Shape,
    ) -> Result<HeapValue<'de>, P::Error> {
        // Steps:
        // 1. Run evidence pass using parser.probe_fields() when needed (untagged enums, flatten).
        // 2. Build solver (Schema::build_auto + Solver::see_key).
        // 3. parser.rewind(checkpoint).
        // 4. Walk events, building Partial/HeapValue via shared visitors.
        shared_deserialize(self.parser_mut(), target_shape)
    }
}
```

`shared_deserialize` (analogous to existing `deserialize_value` in format crates) handles structs/enums/options/seq/map/scalar via `Peek` builders and `Partial`.

### 3. ParseEvent & FieldEvidence

```rust
pub enum ParseEvent<'de> {
    StructStart,
    StructEnd,
    FieldKey(&'de str, FieldLocationHint),
    SequenceStart,
    SequenceEnd,
    Scalar(ScalarValue<'de>),
    VariantTag(&'de str),
    // Format-specific: e.g., XML attribute sentinel
}

pub struct FieldEvidence<'de> {
    pub name: &'de str,
    pub location: FieldLocationHint,
    pub value_type: Option<ValueTypeHint>,
}

/// Lazily produced evidence iterator. Formats implement this by buffering only
/// as much state as they need (e.g., JSON reads one key at a time from the
/// scanner; XML walks attributes until solver stops). Dropping the cursor rewinds.
pub struct ProbeCursor<'a, 'de, P: FormatParser<'de>> {
    parser: &'a mut P,
    // parser-specific checkpoint lives here so dropping the cursor rewinds.
}

impl<'a, 'de, P: FormatParser<'de>> ProbeCursor<'a, 'de, P> {
    pub fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, P::Error> {
        // format-specific implementation: return next key or Ok(None) if exhausted
        unimplemented!()
    }
}
```

- JSON/YAML fill `value_type` (`Null`, `Bool`, `Number`, `String`, `Seq`, `Map`).  
- XML/KDL set `value_type = None` but specify `location` (Attribute/Text/Child/Property/Argument).  
- TOML set `value_type` using parsed type (int/float/string/inline table).  
- Formats using DOM (XML, KDL) call `probe_fields` by walking existing nodes; streaming formats run a non-consuming scan (like JSON’s `SliceAdapter` pass or TOML’s event buffer).

### 4. How formats implement `FormatParser`
- **JSON**: `next_event` reads scanner tokens, `probe_fields` uses SliceAdapter to collect field names and value types, `skip_value` rewinds based on offsets.  
- **YAML**: `next_event` wraps saphyr events, `probe_fields` replays mapping start to gather keys, `skip_value` drains until mapping end.  
- **XML**: `next_event` wraps element/attribute/text nodes, `probe_fields` enumerates attributes + child elements using DOM tree before consuming, locations filled via annotations.  
- **TOML**: `next_event` uses streaming parser events, `probe_fields` uses current key path list (per `navigate_and_deserialize_direct`), value type hints from tokens.

### 5. Shared struct/enum visitor algorithm

Pseudo-flow for `shared_deserialize_struct`:

1. Request evidence if struct has flatten or `deny_unknown_fields`.  
2. Use runtime rename resolver to map serialized keys to `FieldMetadata`.  
3. For each field event:
   - Determine `FieldLocation` to match attributes/children/properties.
   - If field is flattened enum/struct, delegate to solver-supplied path.  
   - Use `Partial::begin_field` + `Partial::end` to populate values, running default injection after the loop.
4. Track missing fields → apply defaults or error per shape metadata.

Enum flow:
1. Evidence determines variant (tag field, variant name, solver).  
2. Build synthetic struct layout (same as serialization) so variants are processed through `shared_deserialize_struct`.  
3. For externally tagged, treat variant as map key.  
4. Untagged uses solver resolution to determine variant fields before recursion.

### 6. Evidence & solver integration

```rust
fn solve_variant<'de, P: FormatParser<'de>>(
    parser: &mut P,
    shape: &'static Shape,
) -> Result<VariantResolution, P::Error> {
    let mut cursor = parser.begin_probe()?;
    let schema = Schema::build_auto(shape)?;
    let mut solver = Solver::new(&schema);

    while let Some(field) = cursor.next()? {
        solver.see_key(field.name);
        if let Some(value_type) = field.value_type {
            solver.see_value_type(field.name, value_type);
        }
        if solver.is_satisfied() {
            break;
        }
    }

    solver.finish()
}
```

### 7. Why this matches real deserializers
- Captures per-format complexity: JSON’s streaming scanner, YAML’s events, XML’s DOM + attributes, TOML’s nested tables.  
- `FieldEvidence` precisely models runtime rename chain: names come resolved before entering shared visitor.  
- `FieldLocationHint` supports XML attributes, KDL properties/arguments, etc., matching annotations described in `facet-format-proposal`.  
- `begin_probe`/`ProbeCursor` mirror the solver usage in `facet-json` and `facet-toml`: formats feed evidence lazily and stop as soon as the solver is satisfied, after which the cursor rewinds automatically.  
- `Partial`/`HeapValue` usage is unchanged; this API simply standardizes how data reaches them.

### 8. Summary
- Formats implement `FormatParser`: handling tokens/events, evidence scanning, checkpoint/rewind.  
- Shared code implements actual struct/enum/sequence/scalar deserialization once, consuming `ParseEvent`s and metadata.  
- All complexity from the draft document (annotations, solver, runtime rename) lives in shared logic; formats only adapt their input representation to `ParseEvent`/`FieldEvidence`.
