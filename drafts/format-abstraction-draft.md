## Draft: Format Abstraction

### Motivation
- JSON serializes and deserializes with a highly sophisticated renaming/tagging engine (`facet-json/src/serialize.rs` and `facet-json/src/deserialize.rs`), and every other format crate (YAML, TOML, XML, KDL) duplicates large parts of that logic to answer the same questions: “what name do I emit or consume?”, “when should I skip or flatten a field?”, “how do I pick a variant layout?”, “how do proxies and transparent wrappers alter the payload?”  
- Instead of re-implementing rename rules, tag/content attributes, `skip*`, `flatten`, proxies, and enum tagging for each syntax, we can extract a format-agnostic core that answers those questions and let each format crate focus solely on surface syntax (braces vs. elements, attributes vs. children, quoting rules, pretty/compact, etc.).

### Landscape summary
| Format | Direction | I/O style | Rename/tag support | Field metadata consumed | Enum layouts handled | Special features | Proxy/flatten support | Streaming notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| JSON (`facet-json`) | serialize/deserialize | string, `JsonWrite`, streaming buffer | `rename`, `rename_all`, `tag`, `content`, `untagged`, `flatten` | rename/alias, skip/skip_serializing_if/skip_deserializing, flatten, metadata, `skip_unless_truthy` | external/internal/adjacent/untagged + flattened variant names | pretty/compact options, `RawJson`, byte-array heuristics, custom proxies | `facet-solver` for flatten deserialization, `Partial` setters for streaming and proxy, transparent types | streaming parser, borrowable strings (borrow mode), solver-based flatten |
| YAML (`facet-yaml`) | serialize/deserialize | string, `Write`, streaming events | similar rename/tag coverage plus `serde::rename` attribute | same Field metadata; `xml`-like children handled via field attrs | external/tuple/struct variants, single listener for `tag`/`content` | mixed content (anchors/aliases), `from_str_borrowed`, event-based streaming | `Partial` + `Solver` for flatten; `skip_unless_truthy` not yet, but format-agnostic helpers would cover it | event stream via saphyr-parser, supports owned/borrowed variants |
| TOML (`facet-toml`) | serialize/deserialize | string | rename/rename_all events | field rename, skip, flatten (array-of-tables) | struct-like + arrays of tables; less enum coverage today | root struct requirement, array-of-tables layout | struct fields via `fields_for_serialize`, `skip` enforced | string parsing only |
| XML (`facet-xml`) | serialize/deserialize | string, `Write`, streaming parser | rename (incl. namespace prefixes via `prefix:localname`), XML annotations | attributes/text/child metadata per field, `skip_serializing`, `skip_deserializing`, `xml::child/elements/text` | element-based variant resolution (uses variant names) | namespaces, attributes vs. children, text nodes, custom float formatters | proxy-aware, uses FieldFlags + annotations for `skip`, solver for flatten/child text interplay | event-based parser, supports `Partial` + `Solver` for flatten and streaming |
| KDL, XML-adjacent formats | serialize/deserialize | string | similar rename handling (kebab-case, xml-like) | nodes/attributes/child ordering metadata | element/node-based | child ordering, attributes, mixed text | flatten and child nodes handled via metadata | custom parsers; writer needs to interleave text/children |

The table will be expanded as we learn more about each crate’s capabilities (e.g., does YAML support alias attributes, does XML emit lists as elements vs. text, etc.).  

### Issue #1127: runtime field name resolution
We read [RFC #1127](https://github.com/facet-rs/facet/issues/1127) in full. It proposes storing the original Rust names in `FieldDef`, adding `rename_all`/per-format overrides to `StructDef`, and threading a per-crate `CrateConfig` (exposed via `facet::crate_config!`) through every `Shape`. Serializers/deserializers would then resolve names at runtime using the six-level override chain described in the issue (Rust name → crate defaults → per-format crate defaults → container-wide override → container per-format override → field-level rename). The RFC is clear that implementing the strategy today touches multiple crates: `facet-macros-impl`/derive (to emit the new metadata and inject `crate_config`), `facet-core`/`facet-reflect` shapes (to store the extra config and original names), `facet-json`/YAML/XML/TOML/KDL serializers/deserializers (to call the shared resolver instead of baking names), plus any format-specific glue (e.g., `facet-xml` field annotations). If we had a shared format abstraction, the resolver would live in one place and format crates would just call it, avoiding rewriting the override chain five or six times.

### Serialization landscape today
- `serialize_value` in `facet-json/src/serialize.rs` already peels proxies, transparent wrappers, `RawJson`, options, pointers, lists, maps, sets, structs, enums, dynamic values, and byte arrays before emitting JSON syntax.  
- Struct fields are iterated via `facet_reflect::fields_for_serialize` (`facet-reflect/src/peek/fields.rs`), so `skip`, `skip_serializing_if`, and `flatten` logic, plus effective names (`FieldItem` and `Field`) and proxy-aware iteration, are handled centrally for JSON.  
- `Shape` metadata (`facet-core/src/types/shape.rs`) stores the enum-tagging directives (`untagged`, `tag`, `content`) that decide whether an enum is externally, internally, adjacently tagged, or untagged and how variant content is emitted (`serialize_enum_content`).  
- Format crates besides JSON tend to duplicate this flow even though the decision tree is identical—only the writer (braces vs `<tag>`, attributes/text interleaving) differs.

### Deserialization landscape today
- JSON deserialization (`facet-json/src/deserialize.rs`) follows the same shape metadata: structs match input keys against `struct_def.fields` (already enriched with rename/alias), flattening uses `facet-solver::Schema` + `Solver`, the parser honors `#[facet(default)]`/`skip_deserializing`, handles proxies via `Partial::begin_custom_deserialization`, and uses the same `Shape::get_tag_attr()`/`content` helpers to select variants.  
- YAML deserialization (`facet-yaml/src/deserialize.rs`) mirrors the JSON paths: it matches event names against `get_serialized_name(field)` (falls back to `field.name` or `#[facet(serde::rename)]`), respects defaults/`skip`, reuses `Partial`/`Solver` for flattening, and uses the same `Facet` metadata for proxies, options, and tags.  
- XML deserialization (`facet-xml/src/deserialize.rs`) again rebuilds the rename/tag logic via `get_field_display_name`, `get_variant_display_name`, and `shape_accepts_element`, while `XmlAnnotationPhase`/`FieldFlags` enforce `skip_serializing` vs `skip_deserializing`, and text/attribute metadata guides matching.  
- Every format currently rebuilds the same lookup tables—matching field names, honoring flattened fields, deciding whether to error on unknown keys, selecting variants—because the format-independent metadata lives in the `Shape`/`Field` definitions.

### Shared abstraction candidates
1. **Field metadata “visitor” (iterator/visitor hybrid)**  
   - Instead of forcing formats into a single iterator, consider exposing a visitor-style API that yields field metadata plus payload delegates. Formats can either iterate (pull model) or register callbacks (visitor) depending on their needs. For XML, we still get metadata about attributes vs. child nodes, so the writer can emit all attributes first, then interleave text/child output using the visitor hooks. Deserializers can use the same visitor to lookup fields by name/key while still honoring rename/alias, skip flags, and proxies.  
   - This visitor would be backed by `fields_for_serialize` and `struct_def.fields`, ensuring `rename`, `alias`, `skip`, `flatten`, `default`, and proxy info travel with the field. The visitor can also include hints (“attribute”, “text node”, “child elements”) so XML/KDL writers know how to dispatch.  
2. **Enum tagging descriptor**  
  - Centralize the `Shape` logic (`is_untagged`, `get_tag_attr`, `get_content_attr`) into an `EnumTagging` descriptor offering helpers like `should_wrap_variant()`, `tag_key()`, `content_key()`, and `variant_name_for_output()`. Instead of just boolean flags, the descriptor synthesizes the final structure that should be visited/serialized—for adjacent tagging it will claim an object with both `tag` and `content` fields, for internal tagging it describes an object that mixes the tag field alongside the variant’s fields, and for external tagging it presents the variant name as the surrounding key whose value is the content. Formats simply traverse that synthesized tree, so they never have to re-implement the tag/content synthesis.  
   - Serialization and deserialization call this descriptor to decide whether to emit tag+content, to search for variants during parsing, and to integrate with flatten/mixed-content heuristics.  
3. **Value classifier (“peel”)**  
   - Factor the top-level `match (shape.def, shape.ty)` from `serialize_value` (handling proxies, transparent structs, bytes/arrays, lists, maps, sets, options, pointers, dynamic values) into a shared routine so every format can inspect the canonical payload before applying syntax-specific punctuation.  
   - A paired deserializer helper can reverse the process when expecting a value (e.g., know when to `begin_option`, `begin_custom_deserialization`, `select_variant`, or reuse proxies).  
4. **Enum content emitter + solver hook**  
   - Reuse `serialize_enum_content`’s struct/tuple/newtype branch (maybe by exposing a helper that traverses `variant.data.fields`).  
   - Provide a complementary parser helper that, given an `EnumTagging`, resolves the variant via `shape.accepts_input` and then uses `facet-solver` when flatten is involved.  
5. **Single trait surfaces**
  - Provide one `FormatSerializer` and one `FormatDeserializer` trait (built atop `FormatWriter`/`FormatParser` if needed) that each format implements to satisfy the shared core. `FormatSerializer::serialize` will be the single entry point the shared `FieldVisitor`/`ValueClassifier` calls, and `FormatDeserializer::deserialize` will be the single entry point for parsing logic. This mirrors Serde’s single trait per direction and keeps each format’s implementation localized to one struct/trait while the shared layer handles rename/tag/visitor logic.
6. **Cranelift / setter-aware deserializers**
  - The “cranelift” style (FacetJSON’s streaming but efficient setters) can be modeled as a specialized visitor: the shared deserializer helper builds `Partial`/`HeapValue` setters as field callbacks, allowing streaming adapters to jump straight to an efficient setter when they find a match. This ensures we keep performance for streaming buffers while still leveraging the shared metadata.  

### API sketches
Below are concrete sketches for the shared traits/helpers and how formats might call them.

```rust
pub struct FieldMetadata {
    pub item: FieldItem,
    pub hints: FieldHints,
}

pub struct FieldHints {
    pub is_flattened: bool,
    pub location: FieldLocation,
    pub valid_names: &'static [&'static str],
}

pub enum FieldLocation {
    Attribute,
    Text,
    Child,
    Any,
}

pub trait FieldVisitor<'a> {
    fn field(
        &mut self,
        meta: FieldMetadata,
        value: Peek<'a, 'facet>,
    ) -> Result<VisitOutcome, VisitorError>;
}

pub enum VisitOutcome {
    Emit,
    Skip,
    Expand,
}
```

For formats that just need a linear traversal, a default iterator can wrap this visitor with the field loop from `fields_for_serialize`, while XML/KDL writers can inspect `meta.hints.location` to emit attributes before children/text nodes.

```rust
pub enum EnumTagging {
    Externally { variant_key: &'static str },
    Internally { tag_key: &'static str },
    Adjacently { tag_key: &'static str, content_key: &'static str },
    Untagged,
}

impl EnumTagging {
    pub fn should_wrap(&self) -> bool {
        !matches!(self, EnumTagging::Untagged)
    }
}
```

`EnumTagging` also exposes a helper such as `layout_fields()` that returns a small virtual struct describing the synthesized fields (tag + content for adjacent, single variant-key field for external, etc.). The serializer/deserializer then walks that structure with the `FieldVisitor`, so the tag/content pair looks like just another pair of `FieldMetadata` records—even though they’re synthesized—allowing formats to treat them uniformly with real fields while still honoring `rename`/`skip`/`flatten`. 

```rust
pub enum ValueShape<'a> {
    Scalar(Peek<'a, 'facet>),
    Sequence(PeekListLike<'a, 'facet>),
    Struct(PeekStruct<'a, 'facet>),
    Enum(PeekEnum<'a, 'facet>),
    Option(Option<Peek<'a, 'facet>>),
}

pub trait ValueClassifier {
    fn classify(&self, peek: Peek<'_, '_>) -> ValueShape<'_>;
}
```

Serializer example:

```rust
fn shared_serialize<W: FormatWriter>(
    writer: &mut W,
    value: Peek<'_, '_>,
    visitor: &mut dyn FieldVisitor,
    classifier: &impl ValueClassifier,
) -> Result<(), SerializeError> {
    match classifier.classify(value) {
        ValueShape::Struct(struct_peek) => {
            writer.start_struct()?;
            for (field_item, field_value) in struct_peek.fields_for_serialize() {
                let meta = FieldMetadata {
                    item: field_item,
                    hints: FieldHints::from(field_item.field),
                };
                if matches!(visitor.field(meta, field_value)?, VisitOutcome::Emit) {
                    writer.write_key(field_item.name)?;
                    shared_serialize(writer, field_value, visitor, classifier)?;
                }
            }
            writer.end_struct()?;
        }
        ValueShape::Enum(enum_peek) => { /* use EnumTagging to decide wrap */ }
        ValueShape::Sequence(seq) => { writer.write_sequence(seq)?; }
        ValueShape::Scalar(_) => { writer.write_scalar(value)?; }
        ValueShape::Option(opt) => { writer.write_option(opt, visitor, classifier)?; }
    }
    Ok(())
}
```

Deserializer example:

```rust
pub trait FormatParser<'a> {
    fn next_token(&mut self) -> ParseToken;
    fn enter_struct(&mut self) -> Result<(), ParseError>;
    fn resolve_field(&self, key: &str) -> Option<FieldMetadata>;
    fn skip_value(&mut self);
    fn current_peek(&self) -> Peek<'a, 'facet>;
}

fn shared_deserialize<'a>(
    parser: &mut impl FormatParser<'a>,
    visitor: &mut dyn FieldVisitor<'a>,
) -> Result<Partial<'a, 'facet>, ParseError> {
    parser.enter_struct()?;
    loop {
        let token = parser.next_token();
        if token.is_struct_end() {
            break;
        }
        let field = parser.extract_key(&token)?;
        if let Some(meta) = parser.resolve_field(field) {
            let value = parser.current_peek();
            visitor.field(meta, value)?;
            parser.skip_value();
        } else {
            parser.skip_value();
        }
    }
    Ok(Partial::new())
}
```

`FormatWriter` / `FormatParser` implementations live closelier to each syntax (JSON/TOML/YAML/XML), reuse the shared `EnumTagging`, and call the `FieldVisitor`/`ValueClassifier` to honor issue #1127’s runtime rename rules instead of re-implementing the override chain.

### FormatParser contract
The parser trait abstracts away the lexing/tokenization differences between formats.

```rust
pub enum ParseToken<'a> {
    StructStart,
    StructEnd,
    Key(&'a str),
    Value(Peek<'a, 'facet>),
    VariantTag(&'a str),
    SequenceStart,
    SequenceEnd,
}

pub trait FormatParser<'a> {
    fn next_token(&mut self) -> ParseToken<'a>;
    fn enter_struct(&mut self);
    fn resolve_field(&self, key: &str) -> Option<FieldMetadata>;
    fn skip_value(&mut self);
}
```

Each format maps its own events into this contract: JSON returns `StructStart`/`StructEnd`/`Key`, YAML uses `MappingStart`/`MappingEnd`, XML translates element nodes and attributes into `Key`+`Value`, and the cranelift deserializer may produce the same tokens directly from the scanner’s state machine.

A realistic shared deserialization loop becomes:

```rust
fn shared_deserialize<'a>(
    parser: &mut impl FormatParser<'a>,
    visitor: &mut dyn FieldVisitor<'a>,
) -> Result<Partial<'a, 'facet>, ParseError> {
    parser.enter_struct();
    loop {
        match parser.next_token() {
            ParseToken::StructEnd => break,
            ParseToken::Key(key) => {
                match parser.resolve_field(key) {
                    Some(meta) => {
                        let value = parser.next_token();
                        let peek = match value {
                            ParseToken::Value(p) => p,
                            other => return Err(ParseError::UnexpectedToken(other)),
                        };
                        visitor.field(meta, peek)?;
                    }
                    None => parser.skip_value(),
                }
            }
            ParseToken::VariantTag(tag) => {
                let layout = EnumTagging::for_shape(parser.current_shape());
                visitor.field(layout.tag_metadata(tag), parser.current_peek())?;
            }
            other => parser.skip_value(), // catch-all for sequences, arrays, etc.
        }
    }
    Ok(Partial::new())
}
```

This captures the “is struct end? break” logic and illustrates how `extract_key`, `current_peek`, and `skip_value` behave differently per format: `extract_key` is `next_token` + match, `current_peek` gives the most recent `Peek`, and `skip_value` fast-forwards to the next token while consuming nested structures. Each format still implements the trait with its own tokenizer, but the shared loop is the same everywhere.

### Options and I/O variants
- Serialization formats may expose multiple outputs: string/`Write` (JSON/TOML/YAML/XML all support both), `JsonWrite` adapters, streaming “windowed” chunking, pretty vs. compact modes, custom float formatters (XML), etc. We should expose these as either options on the `FormatWriter` trait or as builder-style parameters that decorate the writer implementation (e.g., JSON can wrap pretty/indent around the shared writer).  
- Deserializers span another spectrum: borrowed inputs (`from_str_borrowed`, `SliceAdapter` for JSON), owned strings (`from_str`), streaming events (YAML/XML), and solver-driven flatten parsing. The shared core must be agnostic to borrowing semantics: the metadata/visitor layer operates on `Field`/`Shape` descriptors and delegates pointer management to whatever underlying `Partial`/`Adapter` is controlling the data flow.  
- We should document all existing variants (borrowed vs. owned, string vs. writer, streaming vs. buffer) so the shared API can provide the right hooks (e.g., `Adapter::at_offset`, `FormatParser::skip_value`) for each scenario.

### Visitor vs iterator discussion
- XML (and KDL) require arranging attributes before child elements and text nodes interleaved with elements. A pure iterator may be too rigid; a visitor allows the format implementation to request fields in the exact order it needs.  
- Proposal: expose both a `FieldVisitor` interface (formats can register callbacks or request batches like “give me attribute fields first”) and a default iterator built on top of it for serializers/deserializers that just want linear traversal.  
- The visitor would still supply rename/alias info, skip flags, flattened hints, and attribute/text metadata, so XML writers can emit attributes first and then process child/text nodes using the same shared metadata pipeline.  
- To make the sketches less abstract, add an appendix describing `FormatParser` implementations for JSON/YAML/XML.  
  * JSON’s `next_token` returns concrete scanners (`{`, `}`, string key, literal value). `enter_struct` consumes `{`, `resolve_field` uses `FieldMetadata::valid_names`, `current_peek` wraps the `Peek` from the JSON adapter, `skip_value` fast-forwards the scanner.  
  * YAML’s parser maps `saphyr` events to the same `ParseToken` enum (`MappingStart`, `ScalarKey`, `ScalarValue`, etc.), treats `enter_struct` as “consume `MappingStart`”, and `skip_value` rewinds/consumes until the matching `MappingEnd`.  
  * XML’s parser turns `<Element>`/`</Element>`/attributes/text into tokens: `enter_struct` begins the element, `resolve_field` matches attribute/child names via `FieldHints.location`, `current_peek` builds a temporary `Peek` from the attribute or child value, and `skip_value` consumes the remainder of the element.  
  * Each parser shares the same resolver and visitor logic so they never have to re-encode issue #1127’s override chain, even though each format’s `skip_value`/`current_peek` behave differently.

### Benefits
- One canonical spot for rename/alias, skip, flatten, enum tagging, defaults, and proxy handling eliminates divergence across formats.  
- New attributes such as `skip_unless_truthy`, proxy hooks, or additional node types (text/child) can be supported once in the shared visitor/iterator helper.  
- Format crates can focus on syntax-specific output/parsing (XML namespaces, mixed text, TOML tables, YAML quoting) while reusing the same metadata-driven decisions for field names, variant layout, and buffering.

### Next actions
1. Flesh out the format capability table with every format/crate, including borrowing semantics, streaming modes, and options (pretty, indent, attribute text handling).  
2. Define the shared API: a field visitor that exposes metadata and payload callbacks, an enum tagging descriptor, a value classifier/peeler, optional `FormatWriter`/`FormatParser` traits, and hooks for solver/cranelift streaming.  
3. Sketch integration for one format (e.g., YAML or XML) using the visitor/iterator to route fields into attribute/text/child buckets and calling the enum descriptor for tagged variants.  
4. List open questions: should renames from namespaces (e.g., `xml::element_name="ns:foo"`) live in the core, how to keep streaming setters (`Partial`) compatible with borrowed adapters, and how to expose options like pretty vs compact consistently.  
5. Once the API is more concrete, write an ABI proposal or RFC with example usage in JSON, YAML, and XML to compare against the other draft.  
6. Ensure the shared core addresses the priorities listed in GitHub issue #1127 (`https://github.com/facet-rs/facet/issues/1127`), incorporating that conversation once the issue can be reviewed in full.
