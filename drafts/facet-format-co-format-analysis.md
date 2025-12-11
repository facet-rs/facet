# Format-Co Approach: Format Compatibility Analysis

This document analyzes how the format-co (codex) deserialization approach translates to various data formats.

## Summary Table

| Format | Fit | Evidence Quality | Key Benefits | Key Challenges | Notes |
|--------|-----|------------------|--------------|----------------|-------|
| **XML** | ✅ Excellent | High | Attribute/Child distinction, tag names for solver | Mixed content, namespaces | Already started in POC |
| **KDL** | ✅ Excellent | High | Arg/Property/Child distinction, type annotations | Node name handling, multiple values per property | `FieldLocationHint` designed for this |
| **JSON** | ✅ Excellent | High | Self-describing, natural field names | Already works well with existing approach | POC complete |
| **YAML** | ✅ Excellent | High | Similar to JSON, anchors/aliases add complexity | Anchors, merge keys, complex types | Not yet implemented |
| **TOML** | ✅ Good | Medium-High | Table/inline table distinction, dotted keys | Table headers create nesting | Section-based structure |
| **args** | ⚠️ Moderate | Medium | Subcommand solving, unified abstraction | Help/completions, span tracking, flag chaining complexity | Hybrid approach recommended |
| **CSV** | ⚠️ Marginal | Low | Row structure clear | No nesting, no types, weak evidence | Too flat for solver benefit |
| **urlencoded** | ✅ Good | Medium-High | Nested objects, arrays, untagged enum solving | Array syntax variations, type inference from strings | Bracket notation enables rich structure |
| **XDR** | ⚠️ Marginal | Medium | Union discriminants | Schema-driven, limited untagged cases | Schema dictates everything |
| **Postcard (structs)** | 🔴 Poor | None | - | No field names = no evidence | Purely positional |
| **Postcard (maps)** | ✅ Good | Medium | Map keys provide field names | Only applies to HashMap/BTreeMap, not structs | Limited use case |
| **MessagePack** | ⚠️ Depends | Varies | Self-describing option exists | Can be positional or map-based depending on usage | Schema-less has field names |

## Priority Recommendations

### Tier 1: High Value (Implement These)
1. **XML** - Critical format, unique challenges (attributes vs elements)
2. **KDL** - Perfect match for `FieldLocationHint` design
3. **YAML** - Popular format, similar benefits to JSON
4. **TOML** - Config file format, natural fit

### Tier 2: Medium Value (Consider These)
5. **urlencoded** - Fills critical gaps (array support), enables untagged enums for APIs
6. **args** - Hybrid approach: shared deserializer, keep specialized features (help, completions)
7. **Postcard (maps only)** - Niche benefit, only for map collections

### Tier 3: Low Value (Keep Current Approach)
8. **CSV** - Too simple/flat for format-co to add value
9. **XDR** - Schema-driven nature limits solver utility
10. **Postcard (structs)** - Fundamentally incompatible (no field names)

## Key Insight

The format-co approach **thrives on field names**. Formats that:
- ✅ **Have field names on the wire** → Excellent fit (JSON, XML, KDL, YAML, TOML, urlencoded)
- ⚠️ **Have limited/optional field names** → Marginal fit (CSV, XDR)
- 🔴 **Lack field names entirely** → Poor fit (Postcard structs, binary positional formats)

The evidence-based solver is most powerful for **structured, named-field, self-describing formats** where untagged enum disambiguation and flatten support are valuable.

**Note**: URL-encoded was initially underestimated. The widespread bracket notation conventions (`field[nested]=value`, `field[]=array`) provide rich structural information, enabling nested objects, arrays, and untagged enum disambiguation. See [detailed urlencoded analysis](./facet-format-co-urlencoded-deep-dive.md).

---

## Detailed Format Analysis

### 1. XML ✅ Excellent

**Current Status**: POC implementation exists (`facet-format-xml`)

**Mapping Strategy**:
- `<element>` → `StructStart`
- `</element>` → `StructEnd`
- `attr="value"` → `FieldKey("attr", Attribute)`
- `<child>` → `FieldKey("child", Child)`
- Text content → `Scalar(Str(...))`

**Key Challenges**:
1. **Attributes vs Elements**: `FieldLocationHint::Attribute` vs `FieldLocationHint::Child` captures this perfectly
2. **Mixed content**: Text interleaved with elements (current impl handles this via text events)
3. **Evidence collection**: Must probe both attributes AND child element names (TODO in current impl)
4. **Namespace handling**: Not yet addressed
5. **Self-closing tags**: Already handled via queued events

**Evidence Collection Strategy**:
```rust
// For <person name="Alice" age="30"><address>...</address></person>
// Probe should yield:
[
  FieldEvidence { name: "name", location: Attribute, type_hint: Some(String) },
  FieldEvidence { name: "age", location: Attribute, type_hint: Some(String) },
  FieldEvidence { name: "address", location: Child, type_hint: Some(Map) }
]
```

**Fit Assessment**: **Excellent** - XML's structure maps naturally. The Attribute/Child distinction is critical and well-supported.

---

### 2. KDL ✅ Excellent

**Format Characteristics**:
- Node-based: `node arg1 arg2 key="value" { children }`
- Arguments (positional), Properties (named), Children (nested)
- Already has rich `FieldLocationHint` support: `Argument`, `Property`, `Child`, `Text`

**Mapping Strategy**:
```kdl
person "Alice" age=30 {
  address city="NYC"
}
```

Translates to:
```
StructStart
FieldKey("person", Text)     // node name as text content
FieldKey("0", Argument)      // positional arg
  Scalar(Str("Alice"))
FieldKey("age", Property)    // named property
  Scalar(I64(30))
FieldKey("address", Child)   // child node
  StructStart
  FieldKey("city", Property)
    Scalar(Str("NYC"))
  StructEnd
StructEnd
```

**Key Challenges**:
1. **Node names as field content**: Should emit `FieldKey("$node", Text)` or special handling?
2. **Positional arguments**: Index-based field keys ("0", "1", "2") or special event?
3. **Type annotations**: KDL supports `(type)value` - should inform `ValueTypeHint`
4. **Multiple values per property**: KDL allows `key=1 key=2` - sequences?

**Evidence Collection**:
- Must scan arguments, properties, AND child node names
- Type annotations provide strong hints for solver

**Fit Assessment**: **Excellent** - The three-way distinction (Argument/Property/Child) maps perfectly to KDL's model. This is exactly what `FieldLocationHint` was designed for.

---

### 3. JSON ✅ Excellent

**Current Status**: POC implementation complete (`facet-format-json`)

**Mapping Strategy**:
```json
{"name": "Alice", "age": 30}
```

Translates to:
```
StructStart
  FieldKey("name", KeyValue)
    Scalar(Str("Alice"))
  FieldKey("age", KeyValue)
    Scalar(I64(30))
StructEnd
```

**Key Challenges**:
1. **Incomplete JSON support** in current POC:
   - Only handles strings (no numbers, booleans, null)
   - No escape sequence handling in strings
   - No comma handling between array elements
   - No support for nested structures in evidence collection

2. **Error handling**:
   - Only two error types (`UnexpectedEof`, `UnexpectedToken`)
   - No error context (line/column numbers, what was expected)

3. **Event generation**:
   - `skip_value` just calls `next_event` - won't properly skip nested structures
   - String parsing conflates field keys and string values

4. **Probe implementation**:
   - Doesn't capture value type hints (`ValueTypeHint` is always `None`)
   - Doesn't handle nested objects during probing
   - Evidence collection stops at first level fields only

**Fit Assessment**: **Excellent** - Self-describing format with natural field names. Evidence collection enables untagged enum solving.

---

### 4. YAML ✅ Excellent

**Format Characteristics**:
- Similar to JSON but with indentation-based structure
- Anchors and aliases (`&anchor`, `*alias`)
- Merge keys (`<<: *defaults`)
- Multiple document support
- Complex type system (dates, binary, custom tags)

**Mapping Strategy**:
```yaml
name: Alice
age: 30
tags:
  - rust
  - programming
```

Translates to:
```
StructStart
  FieldKey("name", KeyValue)
    Scalar(Str("Alice"))
  FieldKey("age", KeyValue)
    Scalar(I64(30))
  FieldKey("tags", KeyValue)
    SequenceStart
      Scalar(Str("rust"))
      Scalar(Str("programming"))
    SequenceEnd
StructEnd
```

**Key Challenges**:
1. **Anchors and aliases**: Need to resolve references during evidence collection
2. **Merge keys**: `<<: *defaults` must be expanded before probing
3. **Complex types**: Custom tags, dates, binary - need type hint mapping
4. **Indentation tracking**: Parser state more complex than JSON
5. **Multiple documents**: Stream of documents vs single value

**Evidence Collection**:
- Must resolve anchors/aliases first
- Merge keys expand field set
- Tag information provides strong type hints

**Fit Assessment**: **Excellent** - Self-describing with field names. Anchors/merges add complexity but don't break the model.

---

### 5. TOML ✅ Good

**Format Characteristics**:
- Section-based: `[section]` defines context
- Dotted keys: `person.name = "Alice"`
- Inline tables: `person = { name = "Alice", age = 30 }`
- Arrays of tables: `[[servers]]`

**Mapping Strategy**:
```toml
[person]
name = "Alice"
age = 30

[[tags]]
value = "rust"

[[tags]]
value = "programming"
```

Translates to:
```
StructStart
  FieldKey("person", KeyValue)
    StructStart
      FieldKey("name", KeyValue)
        Scalar(Str("Alice"))
      FieldKey("age", KeyValue)
        Scalar(I64(30))
    StructEnd
  FieldKey("tags", KeyValue)
    SequenceStart
      StructStart
        FieldKey("value", KeyValue)
          Scalar(Str("rust"))
      StructEnd
      StructStart
        FieldKey("value", KeyValue)
          Scalar(Str("programming"))
      StructEnd
    SequenceEnd
StructEnd
```

**Key Challenges**:
1. **Section headers**: `[a.b.c]` creates nested structure
2. **Dotted keys**: `a.b.c = 1` also creates nesting - conflicts possible
3. **Table arrays**: `[[array]]` repeated creates sequence
4. **Order sensitivity**: Sections must be processed in order
5. **Inline vs expanded**: Same data, different syntax

**Evidence Collection**:
- Must track section context
- Dotted keys expand to nested fields
- Strong type hints from value syntax

**Fit Assessment**: **Good** - Field names present and nested structure clear. Section-based parsing adds state management complexity.

---

### 6. CSV ⚠️ Marginal

**Format Characteristics**:
- Row-oriented flat tabular data
- Header row defines field names
- No nesting, no types (everything is text)

**Mapping Strategy**:
```csv
name,age,city
Alice,30,NYC
Bob,25,SF
```

**Row-as-Struct approach** (most natural):
```
SequenceStart  // outer array of records
  StructStart
    FieldKey("name", KeyValue)
      Scalar(Str("Alice"))
    FieldKey("age", KeyValue)
      Scalar(Str("30"))  // note: string!
    FieldKey("city", KeyValue)
      Scalar(Str("NYC"))
  StructEnd
  StructStart  // next row
    ...
  StructEnd
SequenceEnd
```

**Key Challenges**:
1. **Type inference**: Everything is text. Solver must try parsing "30" as number
2. **No nesting**: Can't deserialize nested structs without schema tricks (e.g., `address.city` flattening)
3. **Missing fields**: Empty cells - are they None or empty string?
4. **Evidence collection**: Just column names, no type hints

**Evidence Collection**:
```rust
// Header row provides all evidence upfront
[
  FieldEvidence { name: "name", location: KeyValue, type_hint: None },
  FieldEvidence { name: "age", location: KeyValue, type_hint: None },
  FieldEvidence { name: "city", location: KeyValue, type_hint: None }
]
```

**Fit Assessment**: **Marginal** - Works for flat structures, but CSV's constraints (no nesting, no types) are fundamental. Evidence collection is weak. The format-co model doesn't add much value here beyond what facet-csv already does.

---

### 7. urlencoded ✅ Good

**Format Characteristics**:
- Flat key-value pairs: `name=Alice&age=30`
- Bracket notation for nested objects: `user[name]=Alice&user[age]=30`
- Bracket notation for arrays: `tags[]=a&tags[]=b` (Rails/PHP convention)
- Indexed arrays: `items[0]=apple&items[1]=banana`
- Deep nesting: `order[user][address][street]=123+Main+St`
- Arrays of objects: `users[0][name]=Alice&users[1][name]=Bob`

**Current facet-urlencoded Status**:
- ✅ Supports nested objects via bracket notation
- ✅ Supports deep nesting
- 🔴 **Does NOT support arrays** (critical missing feature!)
- 🔴 No untagged enum support

**Mapping Strategy**:

**Simple case**:
```
name=Alice&age=30
```
```
StructStart
  FieldKey("name", KeyValue)
    Scalar(Str("Alice"))
  FieldKey("age", KeyValue)
    Scalar(Str("30"))  // string! needs parsing
StructEnd
```

**Nested object case**:
```
user[name]=Alice&user[age]=30
```
```
StructStart
  FieldKey("user", KeyValue)
    StructStart
      FieldKey("name", KeyValue)
        Scalar(Str("Alice"))
      FieldKey("age", KeyValue)
        Scalar(Str("30"))
    StructEnd
StructEnd
```

**Array case** (empty bracket notation):
```
tags[]=rust&tags[]=web&tags[]=api
```
```
StructStart
  FieldKey("tags", KeyValue)
    SequenceStart
      Scalar(Str("rust"))
      Scalar(Str("web"))
      Scalar(Str("api"))
    SequenceEnd
StructEnd
```

**Array of objects**:
```
users[0][name]=Alice&users[0][age]=30&users[1][name]=Bob&users[1][age]=25
```
```
StructStart
  FieldKey("users", KeyValue)
    SequenceStart
      StructStart
        FieldKey("name", KeyValue)
          Scalar(Str("Alice"))
        FieldKey("age", KeyValue)
          Scalar(Str("30"))
      StructEnd
      StructStart
        FieldKey("name", KeyValue)
          Scalar(Str("Bob"))
        FieldKey("age", KeyValue)
          Scalar(Str("25"))
      StructEnd
    SequenceEnd
StructEnd
```

**Key Challenges**:
1. **Type inference**: Everything is text - must infer bool/number from string values
2. **Array syntax variations**: Empty brackets `[]`, indexed `[N]`, duplicate keys
3. **Two-phase parsing**: Must parse entire input to understand structure (can't stream)
4. **Sparse arrays**: How to handle `items[0]=a&items[5]=f` with gaps?
5. **Mixed notation**: Error on `field[0]=a&field[]=b` (ambiguous)

**Evidence Collection**:
```rust
// Example: Untagged enum disambiguation
// Input: username=alice&password=secret
[
  FieldEvidence { name: "username", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "password", location: KeyValue, type_hint: Some(String) }
]

// Solver can distinguish between:
// - Login { username, password } ✅
// - Logout { session_id } ❌
// - RefreshToken { refresh_token } ❌
```

**Evidence Quality**: **Medium-High**
- ✅ Field names always present
- ✅ Nesting structure clear from bracket notation
- ✅ Array detection explicit via `[]` or indexed `[N]`
- ⚠️ Type hints inferred from string values (not perfect but useful)

**Fit Assessment**: **Good** - Initially underestimated! The bracket notation conventions provide rich structural information. format-co enables:
1. **Array support** (currently missing in facet-urlencoded!)
2. **Untagged enum disambiguation** for API endpoints
3. **Type inference** from string values
4. **Consistent ParseEvent abstraction** across formats

**Real-World Value**: Web forms, REST APIs, search queries all benefit from array support and untagged enum solving.

**See detailed analysis**: [URL-encoded Deep Dive](./facet-format-co-urlencoded-deep-dive.md)

---

### 8. XDR (External Data Representation) ⚠️ Marginal

**Format Characteristics**:
- Binary, schema-driven (requires IDL)
- Fixed-size primitives, arrays, structs, discriminated unions
- Big-endian, 4-byte aligned
- Self-describing discriminated unions only

**Mapping Strategy**:
Similar to Postcard, but:
- Schema provides all type information
- Discriminated unions have explicit tags
- No varint encoding (fixed 4-byte integers)

**Key Challenges**:
1. **Schema dependency**: Needs XDR IDL to parse (unlike self-describing formats)
2. **Alignment**: Must track byte position for padding
3. **Fixed vs variable arrays**: Different wire representations
4. **No field names on wire**: Schema provides names

**Evidence Collection**:
- Must parse discriminated union tag to generate evidence
- For structs, schema provides field list upfront
- Limited value since schema dictates everything

**ParseEvent Strategy**:
```rust
// For discriminated union:
VariantTag("Some")    // read u32 discriminant, look up in schema
StructStart           // variant content
  ...
StructEnd
```

**Fit Assessment**: **Marginal** - XDR is so schema-driven that the evidence-based approach adds little value. The solver might help with untagged enums IF the schema allows them, but XDR itself doesn't have untagged unions. Better to keep existing schema-driven approach.

---

### 9. Postcard ⚠️ Mixed

#### Postcard with Plain Structs 🔴 Poor

**Format Characteristics**:
- Binary, positional encoding
- Varint-encoded integers
- No field names on wire (positional)
- Serde's `serialize_struct` is treated as `serialize_seq`

**Mapping Strategy**:
Postcard doesn't have field names for structs, so everything would be:
```
StructStart
  Scalar(Str("Alice"))  // first field (no name!)
  Scalar(I64(30))       // second field (no name!)
StructEnd
```

**Key Challenges**:
1. **No field names**: Can't emit `FieldKey` events - fields are positional
2. **No evidence collection**: Can't probe for field names since they don't exist
3. **Type ambiguity**: Can't distinguish `Vec<u8>` from string without schema
4. **Solver useless**: Without field names, can't disambiguate untagged enums

**Evidence Collection**:
```rust
// Completely empty - no field names to collect!
[]
```

**Fit Assessment**: **Poor** - Postcard's lack of field names for structs makes the format-co model nearly useless. The evidence-based solver approach relies on field names for disambiguation. Better to keep the existing reflection-driven positional approach in facet-postcard.

#### Postcard with Map Types ✅ Good

**Format Characteristics**:
- `HashMap<String, T>`, `BTreeMap<String, T>` DO serialize keys
- Wire format: `[length][key1][value1][key2][value2]...`

**Mapping Strategy**:
```rust
// For HashMap<String, User>:
ParseEvent sequence:
MapStart
  FieldKey("name", KeyValue)
    Scalar(Str("Alice"))
  FieldKey("age", KeyValue)
    Scalar(I64(30))
MapEnd
```

**Evidence Collection for Maps**:
```rust
// After reading map length and scanning keys:
[
  FieldEvidence { name: "name", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "age", location: KeyValue, type_hint: Some(String) }
]
```

**Fit Assessment**: **Good** - When using Map types, field names (keys) are present and evidence collection works. However, this is a niche use case.

**Overall Postcard Assessment**: Format-co only helps for map collections, not plain structs. Limited applicability.

---

### 10. args (Command-line arguments) ✅ Good

**Format Characteristics**:
- Flags: `--verbose`, `-v`
- Options: `--name Alice`, `--count=3`
- Positional: `input.txt output.txt`
- Repeatable: `--include foo --include bar`

**Mapping Strategy**:
```bash
mycmd --name Alice --verbose input.txt --tag=v1 --tag=v2
```

Translates to:
```
StructStart
  FieldKey("name", KeyValue)
    Scalar(Str("Alice"))
  FieldKey("verbose", KeyValue)
    Scalar(Bool(true))  // flag present
  FieldKey("0", Argument)  // positional
    Scalar(Str("input.txt"))
  FieldKey("tag", KeyValue)
    SequenceStart
      Scalar(Str("v1"))
      Scalar(Str("v2"))
    SequenceEnd
StructEnd
```

**Key Challenges**:
1. **Short/long flag mapping**: `-v` and `--verbose` map to same field
2. **Flag presence**: `--verbose` without value → `Bool(true)`
3. **Positional arguments**: Use index-based keys or special hint?
4. **Repeating options**: Collect into sequence
5. **Subcommands**: Nested structs or variant selection?

**Evidence Collection**:
```rust
// After scanning args:
[
  FieldEvidence { name: "name", location: KeyValue, type_hint: Some(String) },
  FieldEvidence { name: "verbose", location: KeyValue, type_hint: Some(Bool) },
  FieldEvidence { name: "0", location: Argument, type_hint: Some(String) },
  FieldEvidence { name: "tag", location: KeyValue, type_hint: Some(Sequence) }
]
```

**Fit Assessment**: **Good** - The format-co model works well. The `Argument` hint handles positionals. Evidence collection enables subcommand disambiguation (solving which enum variant). However, facet-args is more complex due to help generation, validation, etc. - the format-co layer might be too low-level.

---

### 11. MessagePack ⚠️ Depends

**Format Characteristics**:
- Binary format with multiple encodings
- Can use integer keys (positional) OR string keys (named)
- Self-describing for structure
- Extension types for custom data

**Two Modes**:

**Map Mode** (with string keys):
```
StructStart
  FieldKey("name", KeyValue)
    Scalar(Str("Alice"))
  FieldKey("age", KeyValue)
    Scalar(I64(30))
StructEnd
```

**Array Mode** (positional):
```
StructStart
  Scalar(Str("Alice"))  // no field name
  Scalar(I64(30))       // no field name
StructEnd
```

**Fit Assessment**: **Depends on encoding mode**
- If using map encoding with string keys: ✅ Good fit
- If using array encoding (positional): 🔴 Poor fit

Most MessagePack usage with serde uses map encoding for structs, so format-co would likely be beneficial.

---

## References

- [Postcard serialization format](https://postcard.jamesmunns.com)
- [Serde data model - structs as sequences](https://serde.rs/deserialize-struct.html)
- [KDL document language](https://kdl.dev)
