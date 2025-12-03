# Format Crate Implementation Guide

This document specifies requirements for implementing a facet format crate (like `facet-json`, `facet-kdl`, `facet-yaml`, `facet-xml`, etc.).

## Format Implementation Status

| Requirement | JSON | KDL | YAML | XML |
|------------|------|-----|------|-----|
| **API** |
| `[r.api.deser]` from_str | ✅ | ✅ | ✅ | ✅ |
| `[r.api.deser]` from_slice | ✅ | ❌ | ❌ | ✅ |
| `[r.api.ser]` to_string | ✅ | ✅ | ❌ | ✅ |
| `[r.api.ser]` to_writer | ✅ | ✅ | ❌ | ✅ |
| `[r.api.errors]` miette Diagnostic | ✅ | ✅ | ✅ | ✅ |
| **Scalars** |
| `[r.types.scalars]` u8–u64, i8–i64 | ✅ | ✅ | ✅ | ✅ |
| `[r.types.scalars]` u128, i128 | ⚠️ | ❓ | ❓ | ✅ |
| `[r.types.floats]` f32, f64 | ✅ | ✅ | ✅ | ✅ |
| `[r.types.bool]` bool | ✅ | ✅ | ✅ | ✅ |
| `[r.types.char]` char | ✅ | ✅ | ✅ | ✅ |
| `[r.types.strings]` String | ✅ | ✅ | ✅ | ✅ |
| `[r.types.strings]` &str zero-copy | ✅ | ❌ | ❌ | ❌ |
| `[r.types.strings]` Cow<str> | ✅ | ❌ | ❌ | ❌ |
| **Compound Types** |
| `[r.types.option]` Option | ✅ | ✅ | ✅ | ✅ |
| `[r.types.result]` Result | ❓ | ❓ | ❓ | ❌ |
| `[r.types.structs]` Named structs | ✅ | ✅ | ✅ | ✅ |
| `[r.types.structs]` Tuple structs | ✅ | ❌ | ✅ | ✅ |
| `[r.types.structs]` Unit structs | ✅ | ❌ | ✅ | ✅ |
| `[r.types.enums]` Externally tagged | ✅ | ✅ | ✅ | ✅ |
| `[r.types.enums]` Internally tagged | ✅ | ❓ | ❓ | ❌ |
| `[r.types.enums]` Adjacently tagged | ✅ | ❓ | ❓ | ❌ |
| `[r.types.enums]` Untagged | ✅ | ❓ | ❓ | ❌ |
| `[r.types.collections]` Vec | ✅ | ✅ | ✅ | ✅ |
| `[r.types.collections]` Arrays | ✅ | ❓ | ✅ | ✅ |
| `[r.types.collections]` Sets | ✅ | ❓ | ✅ | ✅ |
| `[r.types.maps]` Maps | ✅ | ❓ | ✅ | ✅ |
| `[r.types.pointers]` Box/Rc/Arc | ✅ | ❓ | ❓ | ✅ |
| `[r.types.bytes]` byte slices | ✅ | ❓ | ❓ | ❌ |
| **Attributes** |
| `[r.attrs.rename]` rename | ✅ | ✅ | ✅ | ✅ |
| `[r.attrs.default]` default | ✅ | ✅ | ✅ | ✅ |
| `[r.attrs.skip]` skip_serializing | ✅ | ✅ | ✅ | ✅ |
| `[r.attrs.skip]` skip_deserializing | ✅ | ✅ | ✅ | ✅ |
| `[r.attrs.skip_if]` skip_serializing_if | ✅ | ✅ | ❓ | ✅ |
| `[r.attrs.transparent]` transparent | ✅ | ✅ | ✅ | ✅ |
| `[r.attrs.flatten]` flatten | ✅ | ✅ | ✅ | ❌ |
| `[r.attrs.deny_unknown]` deny_unknown | ✅ | ✅ | ✅ | ✅ |
| `[r.attrs.deser_with]` deserialize_with | ❓ | ❓ | ❓ | ❌ |
| `[r.attrs.ser_with]` serialize_with | ❓ | ❓ | ❓ | ❌ |
| `[r.attrs.type_tag]` type_tag | N/A | ✅ | N/A | ⚠️ |
| **Spans** |
| `[r.spans.spanned]` Spanned<T> | ✅ | ✅ | ✅ | ⚠️ |
| **Solver** |
| `[r.solver]` flatten solver | ✅ | ✅ | ✅ | ❌ |

Legend: ✅ = implemented, ⚠️ = partial, ❌ = not implemented, ❓ = unknown/untested, N/A = not applicable

## API Surface

### The Facet Trait

The `Facet` trait is defined in `facet-core`:

```rust,ignore
pub unsafe trait Facet<'facet>: 'facet {
    const SHAPE: &'static Shape;
}
```

Key points:
- The `'facet` lifetime allows types to borrow from the input during deserialization
- The trait is `unsafe` because implementors must guarantee the `SHAPE` accurately describes the type's memory layout
- There is no `Sized` bound — `Facet` can be implemented for `str`, `[T]`, etc.

### `[r.api.deser]` Deserialization

Your format crate should expose deserialization functions with signatures like:

```rust,ignore
pub fn from_str<'input, 'facet, T: Facet<'facet>>(
    input: &'input str,
) -> Result<T, YourError>
where
    'input: 'facet;

pub fn from_slice<'input, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
) -> Result<T, YourError>
where
    'input: 'facet;
```

The `'input: 'facet` bound means the input must outlive the deserialized value. This enables zero-copy deserialization where the output can borrow directly from the input (e.g., `&'a str` fields).

### `[r.api.ser]` Serialization

```rust,ignore
pub fn to_string<'facet, T: Facet<'facet> + ?Sized>(value: &T) -> String;

pub fn to_vec<'facet, T: Facet<'facet> + ?Sized>(value: &T) -> Vec<u8>;

pub fn to_writer<'facet, T: Facet<'facet> + ?Sized, W: Write>(
    value: &T,
    writer: W,
) -> Result<(), YourError>;
```

Note the `?Sized` bound — this allows serializing unsized types like `str` directly.

### `[r.api.errors]` Error Reporting

Errors must implement `std::error::Error`.

For text formats, implement `miette::Diagnostic` to provide rich error reporting with source spans:

```rust,ignore
impl miette::Diagnostic for YourError {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> { ... }
    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> { ... }
}
```

Example rendered output:
```text
× task validation failed
  ╭─[1:7]
1 │ build timeout=0
  ·       ────┬────
  ·           ╰── timeout cannot be zero
2 │ test timeout=999999
  ·      ───────┬──────
  ·             ╰── timeout too large (max 86400s)
  ╰────
```

Binary formats cannot provide meaningful source spans to miette — byte offsets in error messages are sufficient.

## Supported Types

### `[r.types.scalars]` Integers

- `u8`, `u16`, `u32`, `i8`, `i16`, `i32` — universally supported
- `u64`, `i64` — supported, but JSON numbers lose precision beyond 53 bits (IEEE 754). Document this limitation.
- `u128`, `i128` — not universally supported. JSON cannot represent these natively. Consider a feature flag or string representation.

### `[r.types.floats]` Floats

- `f32`, `f64` — standard support
- NaN / Infinity: JSON has no representation. Document your policy (error? null? string like `"NaN"`?).

### `[r.types.bool]` Booleans

Standard `true`/`false` mapping.

### `[r.types.char]` Characters

Serialize as a single-character string.

### `[r.types.strings]` Strings

- `String` — fully supported
- `&str` — zero-copy: borrow directly from input when possible (e.g., unescaped strings). Error if borrowing isn't possible (e.g., string requires unescaping/allocation).
- `Cow<'_, str>` — use `Cow::Borrowed` when possible, `Cow::Owned` when allocation is required

### `[r.types.option]` Option

`[r.types.option.missing]` When a field is `Option<T>` **without** `#[facet(default)]`:
- Missing field → **error** (differs from serde, which defaults to `None`)
- Explicit `null` → `None`
- Value present → `Some(value)`

`[r.types.option.default]` When a field is `Option<T>` **with** `#[facet(default)]`:
- Missing field → `None`
- Explicit `null` → `None`
- Value present → `Some(value)`

### `[r.types.result]` Result

TODO: Define canonical representation. Suggested: `{"Ok": value}` or `{"Err": error}`.

### `[r.types.structs]` Structs

- **Named fields**: serialize as object/map with field names as keys
- **Tuple structs**: serialize as array (e.g., `struct Point(i32, i32)` → `[1, 2]`)
- **Unit structs**: serialize as `null` or empty object (document your choice)

### `[r.types.enums]` Enums

- **Unit variants**: `enum E { A, B }` — typically `"A"` or `{"A": null}`
- **Newtype variants**: `enum E { A(T) }` — `{"A": value}`
- **Tuple variants**: `enum E { A(T, U) }` — `{"A": [t, u]}`
- **Struct variants**: `enum E { A { x: T } }` — `{"A": {"x": value}}`

### `[r.types.collections]` Collections

- `Vec<T>` — array
- `[T; N]` — fixed-size array (no length prefix in binary; exact length enforced on deser)
- `HashSet<T>`, `BTreeSet<T>` — array (`BTreeSet` iteration is deterministic)

### `[r.types.maps]` Maps

TODO: String keys are straightforward. Non-string keys need discussion — stringify the key? Use array of tuples `[[k, v], ...]`?

### `[r.types.pointers]` Smart Pointers

`Box<T>`, `Rc<T>`, `Arc<T>` serialize transparently as the inner `T`.

TODO: Current facet-reflect handling may need work.

### `[r.types.bytes]` Byte Data

`Vec<u8>`, `&[u8]`, `bytes::Bytes` — no special `#[serde(with = "serde_bytes")]` needed.

For text formats, choose a representation:
- **Array of u8**: `[72, 101, 108, 108, 111]` — this is what facet-json currently does
- **Base64**: `"SGVsbG8="` — more compact for large blobs

TODO: Decide whether base64 should be default or opt-in.

### `[r.types.temporal]` Temporal Types

Common crates: `chrono`, `time`, `jiff`.

Unlike serde, facet has **no standard attribute** to specify datetime format (no `#[serde(with = "...")]` equivalent yet). Recommendations:

**Text formats**: Use RFC 3339 / ISO 8601 as the default:
- Instant: `"2024-01-15T10:30:00Z"`
- Date: `"2024-01-15"`
- Time: `"10:30:00"`

**Binary formats**: Use native timestamp if available (e.g., msgpack ext type), otherwise `i64` seconds + `u32` nanoseconds.

Types to support:
- `chrono`: `DateTime<Utc>`, `DateTime<FixedOffset>`, `NaiveDate`, `NaiveTime`, `NaiveDateTime`
- `time`: `OffsetDateTime`, `PrimitiveDateTime`, `Date`, `Time`
- `jiff`: `Timestamp`, `Zoned`, `civil::Date`, `civil::Time`, `Span` (duration)
- `std`: `SystemTime`, `Duration`

## Attributes

### `[r.attrs.rename]` rename / rename_all

```rust,ignore
#[facet(rename = "userName")]
user_name: String,

#[facet(rename_all = "camelCase")]
struct Config { ... }
```

These are reflected in the `Shape` at compile time. The field's `name` in the shape is already the renamed value — **your format crate doesn't need to do anything special**.

### `[r.attrs.default]` default

```rust,ignore
#[facet(default)]
count: u32,

#[facet(default = "default_port")]
port: u16,
```

If using facet-reflect's deferred materialization (`Partial`), defaults are filled automatically when you call `.build()`. Your format just needs to not set the field.

### `[r.attrs.skip]` skip_serializing / skip_deserializing

```rust,ignore
struct Data {
    #[facet(skip_serializing)]
    cached_value: String,

    #[facet(skip_deserializing)]
    computed: u64,
}
```

`[r.attrs.skip.ser]` For `skip_serializing`: Check `FieldFlags::SKIP_SERIALIZING` and omit these fields from output.

`[r.attrs.skip.deser]` For `skip_deserializing`: Ignore input for these fields. They must have `#[facet(default)]` or implement `Default`.

### `[r.attrs.skip_if]` skip_serializing_if

```rust,ignore
#[facet(skip_serializing_if = "Option::is_none")]
maybe: Option<String>,
```

The predicate has signature `fn(&T) -> bool`. Check `FieldVTable::skip_serializing_if` — if present and returns `true`, omit the field.

### `[r.attrs.transparent]` transparent

```rust,ignore
#[facet(transparent)]
struct UserId(u64);
```

Serialize/deserialize as the inner type directly (`42` not `{"0": 42}`).

TODO: Verify behavior across facet-json and facet-kdl.

### `[r.attrs.flatten]` flatten

```rust,ignore
struct Server {
    name: String,
    #[facet(flatten)]
    config: ServerConfig,
}
```

Flattened fields merge into the parent structure. See the [Flatten Solver](#flatten-solver) section.

### `[r.attrs.enum_repr]` Enum Representations

Facet supports four enum representations, matching serde's model. Consider this enum:

```rust,ignore
#[derive(Facet)]
enum Message {
    Request { id: String, method: String },
    Response { id: String, result: Value },
}
```

#### Externally Tagged (default)

The default representation. The variant name wraps the content:

```json
{"Request": {"id": "...", "method": "..."}}
```

Characteristics:
- Works across all text and binary formats
- Variant is known before parsing content
- Handles all variant types: unit, newtype, tuple, struct

#### Internally Tagged

```rust,ignore
#[derive(Facet)]
#[facet(tag = "type")]
enum Message {
    Request { id: String, method: String },
    Response { id: String, result: Value },
}
```

The tag is inside the content, alongside other fields:

```json
{"type": "Request", "id": "...", "method": "..."}
```

Characteristics:
- Common in Java libraries and REST APIs
- Works for struct variants, newtype variants containing structs/maps, and unit variants
- Does **not** work for tuple variants (compile-time error)

#### Adjacently Tagged

```rust,ignore
#[derive(Facet)]
#[facet(tag = "t", content = "c")]
enum Block {
    Para(Vec<Inline>),
    Str(String),
}
```

Tag and content are sibling fields:

```json
{"t": "Para", "c": [{...}, {...}]}
{"t": "Str", "c": "the string"}
```

Characteristics:
- Common in Haskell ecosystem
- Handles all variant types

#### Untagged

```rust,ignore
#[derive(Facet)]
#[facet(untagged)]
enum Message {
    Request { id: String, method: String },
    Response { id: String, result: Value },
}
```

No tag — the deserializer tries each variant in order:

```json
{"id": "...", "method": "..."}
```

Characteristics:
- Useful for "string or int" union types
- Deserializer tries variants in declaration order, first success wins
- Handles all variant types

Example for union types:

```rust,ignore
#[derive(Facet)]
#[facet(untagged)]
enum StringOrInt {
    Int(i64),
    String(String),
}
```

Can deserialize from either `42` or `"hello"`.

#### Helper Methods

Format crates can use these `Shape` methods:
- `shape.is_untagged()` — returns `true` if `#[facet(untagged)]`
- `shape.get_tag_attr()` — returns `Some("type")` for `#[facet(tag = "type")]`
- `shape.get_content_attr()` — returns `Some("c")` for `#[facet(content = "c")]`

Determining representation:
- `is_untagged()` → untagged
- `get_tag_attr().is_some() && get_content_attr().is_some()` → adjacently tagged
- `get_tag_attr().is_some()` → internally tagged
- otherwise → externally tagged (default)

### `[r.attrs.deny_unknown]` deny_unknown_fields

```rust,ignore
#[facet(deny_unknown_fields)]
struct Strict { ... }
```

Your deserializer must reject input containing unrecognized fields. Error messages should name the unknown field.

### `[r.attrs.deser_with]` deserialize_with

```rust,ignore
#[facet(deserialize_with = "parse_hex")]
value: u64,
```

Function signature:

```rust,ignore
unsafe fn parse_hex<'mem>(
    source: PtrConst<'mem>,   // points to intermediate type (e.g., &str)
    target: PtrUninit<'mem>,  // points to uninitialized field
) -> Result<PtrMut<'mem>, String>;
```

Primarily for text formats. The deserializer:
1. Deserializes into the source shape (from `FieldAttribute::DeserializeFrom`)
2. Calls the conversion function to produce the final value

### `[r.attrs.ser_with]` serialize_with

```rust,ignore
#[facet(serialize_with = "to_hex")]
value: u64,
```

Similar pattern — convert to intermediate type, then serialize that.

### `[r.attrs.type_tag]` type_tag

```rust,ignore
#[facet(type_tag = "server")]
struct Server { ... }
```

For self-describing hierarchical formats (KDL, XML) where elements have names. Not applicable to JSON/YAML.

## Spans

### `[r.spans.spanned]` Spanned Types

When deserializing into a type with this shape:

```rust,ignore
struct Spanned<T> {
    node: T,    // or `value: T`
    span: Span,
}

struct Span {
    start: usize,  // or `offset: usize`
    len: usize,
}
```

The deserializer should populate `span` with the source location where `T` was parsed.

**Current state**: Detection uses duck-typing (field name matching). This should migrate to type ID comparison against a canonical `Spanned<T>` from `facet-reflect`.

### `[r.spans.text]` Text Format Spans

Provide byte offset and length. Line/column in error messages is helpful but not required in the span itself.

### `[r.spans.binary]` Binary Format Spans

Byte offset and length. Include field/variant name in diagnostics since raw offsets are harder to interpret.

## Flatten Solver

The `facet-solver` crate handles disambiguation for `#[facet(flatten)]` on enums and structs. **You consume the solver — you don't reimplement its logic.**

### What It Solves

Given:

```rust,ignore
struct Config {
    name: String,
    #[facet(flatten)]
    backend: Backend,
}

enum Backend {
    File { path: String },
    Database { url: String, port: u16 },
}
```

And JSON input:
```json
{"name": "myapp", "url": "localhost", "port": 5432}
```

The solver determines `Database` is the matching variant because both `url` and `port` are present.

### `[r.solver.field_presence]` Disambiguation by Field Presence

- `path` present → `File`
- `url` + `port` present → `Database`
- `url` without `port` → error (incomplete variant)
- `path` + `url` → error (conflicting/ambiguous)

### `[r.solver.value_range]` Disambiguation by Value Range

```rust,ignore
enum Size {
    Small { v: u8 },   // 0–255
    Large { v: u16 },  // 0–65535
}
```

- `{"v": 100}` → `Small` (fits in u8)
- `{"v": 1000}` → `Large` (exceeds u8 range)

### `[r.solver.optimistic]` Disambiguation by Optimistic Parsing

When multiple variants parse from the same token type:

```rust,ignore
enum Value {
    Date(chrono::NaiveDate),   // parses "2024-01-15"
    Time(chrono::NaiveTime),   // parses "10:30:00"
    Text(String),              // parses any string
}
```

All are strings in JSON. The solver tries in order:
1. Try parsing as `NaiveDate` — success → `Date`
2. Try parsing as `NaiveTime` — success → `Time`
3. Fall back to `Text`

### `[r.solver.ambiguous]` Ambiguity Errors

```rust,ignore
enum Kind {
    A { x: u8 },
    B { x: u8 },
}
```

Input `{"x": 5}` matches both variants equally. This **must error** — do not pick arbitrarily.

### Using facet-solver

See the `facet-solver` crate documentation. It provides:
- Schema construction from `Shape`
- Match resolution given available fields/values
- Clear error messages for ambiguous or incomplete matches

## Binary Formats

Support for binary formats (bincode, postcard, msgpack) is less mature than text formats.

### `[r.binary.order]` Field Ordering

Serialize/deserialize fields in declaration order. No field names in the wire format.

### `[r.binary.extra]` Extra Data

Extra bytes after deserializing **must error**. There's no concept of "unknown fields to skip."

### `[r.binary.encoding]` Encoding Policy

Document and keep stable:
- Endianness (little / big / native)
- Integer encoding (fixed-width vs varint)
- Length prefix format (u8 / u16 / u32 / varint)

---

*Binary format requirements are not fully specified. If you're implementing one, please open an issue to discuss.*
