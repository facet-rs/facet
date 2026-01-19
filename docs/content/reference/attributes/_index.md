+++
title = "Attributes"
weight = 1
insert_anchor_links = "heading"
+++

Complete reference for `#[facet(...)]` attributes.

## Container attributes

These attributes apply to structs and enums.

### `deny_unknown_fields`

Produce an error when encountering unknown fields during deserialization. By default, unknown fields are silently ignored.

```rust,noexec
#[derive(Facet)]
#[facet(deny_unknown_fields)]
struct Config {
    name: String,
    port: u16,
}
```

### `default`

Use the type's `Default` implementation for missing fields during deserialization.

```rust,noexec
#[derive(Facet, Default)]
#[facet(default)]
struct Config {
    name: String,
    port: u16,  // Will use Default if missing
}
```

### `rename_all`

Rename all fields/variants using a case convention.

```rust,noexec
#[derive(Facet)]
#[facet(rename_all = "camelCase")]
struct Config {
    server_name: String,  // Serialized as "serverName"
    max_connections: u32, // Serialized as "maxConnections"
}
```

**Supported conventions:**
- `"PascalCase"`
- `"camelCase"`
- `"snake_case"`
- `"SCREAMING_SNAKE_CASE"`
- `"kebab-case"`
- `"SCREAMING-KEBAB-CASE"`

### `transparent`

Forward serialization/deserialization to the inner type. Used for newtype patterns.

```rust,noexec
#[derive(Facet)]
#[facet(transparent)]
struct UserId(u64);  // Serialized as just the u64
```

### `metadata_container`

Mark a struct as a metadata container — it serializes transparently through its non-metadata field while preserving metadata for formats that support it.

```rust,noexec
#[derive(Facet)]
#[facet(metadata_container)]
struct Documented<T> {
    value: T,
    #[facet(metadata = "doc")]
    doc: Option<Vec<String>>,
}
```

**Rules for metadata containers:**

1. **Exactly one non-metadata field** — This is the "value" field that the container serializes as
2. **At least one metadata field** — Fields marked with `#[facet(metadata = "...")]`
3. **No duplicate metadata kinds** — Each metadata kind can only appear once

**Serialization behavior:**

During serialization, the container is transparent — `Documented<String>` serializes exactly like `String`. However, formats that support metadata (like Styx) can access the metadata fields and emit them appropriately (e.g., as doc comments).

```rust,noexec
#[derive(Facet)]
#[facet(metadata_container)]
struct Documented<T> {
    value: T,
    #[facet(metadata = "doc")]
    doc: Option<Vec<String>>,
}

#[derive(Facet)]
struct Config {
    name: Documented<String>,
    port: Documented<u16>,
}

let config = Config {
    name: Documented {
        value: "myapp".into(),
        doc: Some(vec!["The application name".into()]),
    },
    port: Documented {
        value: 8080,
        doc: Some(vec!["Port to listen on".into(), "Must be > 1024".into()]),
    },
};

// JSON (no metadata support): {"name": "myapp", "port": 8080}
// Styx (with metadata support):
// /// The application name
// name "myapp"
// /// Port to listen on
// /// Must be > 1024
// port 8080
```

**Composing metadata containers:**

You can nest metadata containers to combine multiple kinds of metadata:

```rust,noexec
#[derive(Facet)]
#[facet(metadata_container)]
struct Spanned<T> {
    value: T,
    #[facet(metadata = "span")]
    span: Option<Span>,
}

#[derive(Facet)]
#[facet(metadata_container)]
struct Documented<T> {
    value: T,
    #[facet(metadata = "doc")]
    doc: Option<Vec<String>>,
}

// Combine both: value with span AND doc metadata
type FullyAnnotated<T> = Spanned<Documented<T>>;

#[derive(Facet)]
struct Schema {
    fields: Vec<FullyAnnotated<Field>>,
}
```

When nested, the outer container's value field is the inner container. The metadata from both levels is preserved and accessible to formats that need it.

**Why use metadata containers?**

- **Preserve source information** — Track spans, doc comments, or other metadata from parsing
- **Format-specific output** — Let formats like Styx emit doc comments while JSON ignores them
- **Type-safe metadata** — The metadata is part of the type system, not a side channel
- **Composable** — Combine different metadata kinds by nesting containers

**Difference from `transparent`:**

- `transparent` requires exactly one field total
- `metadata_container` requires exactly one *non-metadata* field, plus one or more metadata fields
- Both serialize transparently through their inner value
- `metadata_container` preserves metadata for formats that support it

### `opaque`

Mark a type as opaque — its inner structure is hidden from facet. The type itself implements `Facet`, but its fields are not inspected or serialized. This is useful for:

- Types with fields that don't implement `Facet`
- Types whose internal structure shouldn't be exposed
- Wrapper types around FFI or unsafe internals

```rust,noexec
#[derive(Facet)]
#[facet(opaque)]
struct InternalState {
    handle: *mut c_void,  // Doesn't need Facet
    cache: SomeNonFacetType,
}
```

**Important:** Opaque types cannot be serialized or deserialized on their own — use them with `#[facet(proxy = ...)]` to provide a serializable representation:

```rust,noexec
// A type that doesn't implement Facet
struct SecretKey([u8; 32]);

// A proxy that can be serialized (as hex string)
#[derive(Facet)]
#[facet(transparent)]
struct SecretKeyProxy(String);

impl TryFrom<SecretKeyProxy> for SecretKey {
    type Error = &'static str;
    fn try_from(proxy: SecretKeyProxy) -> Result<Self, Self::Error> {
        // Parse hex string into bytes
        let bytes = hex::decode(&proxy.0).map_err(|_| "invalid hex")?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| "wrong length")?;
        Ok(SecretKey(arr))
    }
}

impl TryFrom<&SecretKey> for SecretKeyProxy {
    type Error = std::convert::Infallible;
    fn try_from(key: &SecretKey) -> Result<Self, Self::Error> {
        Ok(SecretKeyProxy(hex::encode(&key.0)))
    }
}

#[derive(Facet)]
struct Config {
    name: String,
    #[facet(opaque, proxy = SecretKeyProxy)]
    key: SecretKey,  // Serialized as hex string via proxy
}
```

**Note:** For now, field-level `#[facet(opaque)]` requires the field type to be `'static`
(no borrowed references). This avoids unsound lifetime laundering through reflection.
If you need to hide a borrowed type, use a proxy or a wrapper type that owns its data.

**When `assert_same!` encounters an opaque type**, it returns `Sameness::Opaque` — you cannot structurally compare opaque values.

### `pod`

Mark a type as Plain Old Data. POD types have no invariants — any combination of valid field values produces a valid instance. This enables safe mutation through reflection.

```rust,noexec
#[derive(Facet)]
#[facet(pod)]
struct Point {
    x: i32,
    y: i32,
}
```

**What POD means:**

- Any combination of valid field values is valid for the struct as a whole
- There are no hidden constraints or relationships between fields
- The type can be safely mutated field-by-field through reflection

**What POD does NOT mean:**

- POD is **not** an auto-trait — a struct with all POD fields is not automatically POD
- The type author must explicitly opt in to assert there are no semantic invariants

**POD vs invariants:** These attributes are mutually exclusive. If you need validation, use `invariants`; if you want unrestricted mutation, use `pod`.

```rust,noexec
// This is an error:
#[derive(Facet)]
#[facet(pod, invariants = validate)]  // ❌ Compile error
struct Invalid { x: i32 }
```

**Primitives are implicitly POD:** Types like `u32`, `bool`, `f64`, and `char` are always considered POD — any value of those types is valid.

**Containers don't need POD:** `Vec<T>`, `Option<T>`, and similar containers are manipulated through their vtables, which maintain their own internal invariants. The POD-ness of the element type `T` matters when mutating elements, not the container itself.

**When to use POD:**

Use `#[facet(pod)]` when your type is a simple data container with no semantic constraints:

```rust,noexec
// Good candidates for POD:
#[derive(Facet)]
#[facet(pod)]
struct Color { r: u8, g: u8, b: u8 }

#[derive(Facet)]
#[facet(pod)]
struct Dimensions { width: u32, height: u32 }

// NOT good for POD (has invariant: start <= end):
#[derive(Facet)]
#[facet(invariants = Range::is_valid)]
struct Range { start: u32, end: u32 }

impl Range {
    fn is_valid(&self) -> bool { self.start <= self.end }
}
```

### `skip_all_unless_truthy`

Applies `skip_unless_truthy` to every field in the container. This is a convenient shorthand when all or most fields should be omitted if they're falsy.

```rust,noexec
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
struct Config {
    name: String,              // Omitted if empty
    description: String,       // Omitted if empty
    count: u32,                // Omitted if zero
    enabled: bool,             // Omitted if false
}
```

Individual fields can still override this with `#[facet(skip_serializing)]` or by not being marked for skipping.

### `type_tag`

Add a type identifier for self-describing formats.

```rust,noexec
#[derive(Facet)]
#[facet(type_tag = "com.example.User")]
struct User {
    name: String,
}
```

### `crate`

Specify a custom path to the facet crate. This is primarily useful for crates that re-export facet and want users to derive `Facet` without adding facet as a direct dependency.

```rust,noexec
// In a crate that re-exports facet
use other_crate::facet;

#[derive(other_crate::facet::Facet)]
#[facet(crate = other_crate::facet)]
struct MyStruct {
    field: u32,
}
```

This attribute can also be used with enums and all struct variants:

```rust,noexec
use other_crate::facet;

#[derive(other_crate::facet::Facet)]
#[facet(crate = other_crate::facet)]
enum MyEnum {
    Variant1,
    Variant2 { data: String },
}

#[derive(other_crate::facet::Facet)]
#[facet(crate = other_crate::facet)]
struct TupleStruct(u32, String);
```

## Enum attributes

These attributes control enum serialization format.

### `untagged`

Serialize enum variants without a discriminator tag. The deserializer tries each variant in order until one succeeds.

```rust,noexec
#[derive(Facet)]
#[facet(untagged)]
enum Value {
    Int(i64),
    Float(f64),
    String(String),
}
```

#### Variant matching order

For formats with typed values (JSON, YAML, MessagePack), variants are tried in **definition order**. The first matching variant wins. In the example above, a JSON `42` matches `Int(i64)` because integers match `i64`.

#### Text-based formats (XML)

Text-based formats like XML represent all values as strings. When deserializing `<value>42</value>`, the parser produces a "stringly-typed" value — text that may encode a more specific type.

For stringly-typed values, facet uses **two-tier matching**:

1. **Tier 1 (Parseable types)**: Try non-string types that can parse the value (`i64`, `f64`, `bool`, etc.). First successful parse wins, in definition order.

2. **Tier 2 (String fallback)**: If no parseable type matched, fall back to `String`/`&str`/`Cow<str>` variants.

This ensures `<value>42</value>` matches `Int(i64)` rather than `String(String)`, regardless of definition order:

```rust,noexec
#[derive(Facet)]
#[facet(untagged)]
enum Value {
    Text(String),   // Tier 2: tried last for stringly-typed values
    Number(i64),    // Tier 1: tried first (parses "42")
    Flag(bool),     // Tier 1: tried first (doesn't parse "42")
}

// XML: <v>42</v>    → Number(42)
// XML: <v>true</v>  → Flag(true)
// XML: <v>hello</v> → Text("hello")
```

### `tag`

Use internal tagging — the variant name becomes a field inside the object.

```rust,noexec
#[derive(Facet)]
#[facet(tag = "type")]
enum Message {
    Request { id: u64, method: String },
    Response { id: u64, result: String },
}
// {"type": "Request", "id": 1, "method": "get"}
```

### `tag` + `content`

Use adjacent tagging — separate fields for the tag and content.

```rust,noexec
#[derive(Facet)]
#[facet(tag = "t", content = "c")]
enum Message {
    Text(String),
    Data(Vec<u8>),
}
// {"t": "Text", "c": "hello"}
```

## Variant attributes

These attributes apply to enum variants.

### `other`

Mark a variant as a catch-all for unknown variant names. When deserializing, if no variant matches the input tag, the variant marked with `other` will be used.

```rust,noexec
#[derive(Facet)]
enum Status {
    Active,
    Inactive,
    #[facet(other)]
    Unknown(String),  // Catches "Pending", "Archived", etc.
}
```

#### Capturing variant tags with `#[facet(tag)]` and `#[facet(content)]`

For self-describing formats that emit `VariantTag` events (like Styx or XML), an `#[facet(other)]` variant can capture both the tag name and its payload using field-level `#[facet(tag)]` and `#[facet(content)]` attributes:

```rust,noexec
#[derive(Facet)]
enum Schema {
    // Known variants
    Object(ObjectSchema),
    Seq(SeqSchema),
    
    // Catch-all for unknown tags like @string, @unit, @MyCustomType
    #[facet(other)]
    Type {
        #[facet(tag)]
        name: String,      // Captures the tag name (e.g., "string", "unit")
        #[facet(content)]
        payload: Value,    // Captures the payload
    },
}
```

With this definition:
- `@object{...}` → `Schema::Object(...)` (known variant)
- `@string` → `Schema::Type { name: "string", payload: Value::Unit }`
- `@custom"data"` → `Schema::Type { name: "custom", payload: Value::Str("data") }`
- `@foo{x 1}` → `Schema::Type { name: "foo", payload: Value::Object(...) }`

**Rules for `#[facet(other)]` variants:**

1. Only one variant per enum can be marked `#[facet(other)]`
2. When used with `VariantTag` events:
   - A field with `#[facet(tag)]` receives the tag name as a `String`
   - A field with `#[facet(content)]` receives the deserialized payload
   - If no `#[facet(content)]` field exists, the payload must be unit (empty)
3. For non-self-describing formats (JSON with external tagging), the variant name from the input is used directly

**Discarding payloads:**

If you only care about the tag name and know the payload is always unit, you can omit the `#[facet(content)]` field:

```rust,noexec
#[derive(Facet)]
enum TypeRef {
    Object,
    Seq,
    #[facet(other)]
    Named {
        #[facet(tag)]
        name: String,  // Just capture the tag name, payload must be unit
    },
}
```

This will fail deserialization if the unknown variant has a non-unit payload.

## Field attributes

These attributes apply to struct fields.

### `rename`

Rename a field during serialization/deserialization.

```rust,noexec
#[derive(Facet)]
struct User {
    #[facet(rename = "user_name")]
    name: String,
}
```

### `default`

Use a default value when the field is missing during deserialization.

```rust,noexec
#[derive(Facet)]
struct Config {
    name: String,

    #[facet(default)]  // Uses Default::default()
    tags: Vec<String>,

    #[facet(default = 8080)]  // Uses literal value
    port: u16,

    #[facet(default = default_timeout())]  // Uses function
    timeout: Duration,
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}
```

### `skip`

Skip this field entirely during both serialization and deserialization. The field must have a default value.

```rust,noexec
#[derive(Facet)]
struct Session {
    id: String,
    #[facet(skip, default)]
    internal_state: InternalState,
}
```

### `skip_serializing`

Skip this field during serialization only.

```rust,noexec
#[derive(Facet)]
struct User {
    name: String,
    #[facet(skip_serializing)]
    password_hash: String,
}
```

### `skip_deserializing`

Skip this field during deserialization (uses default value).

```rust,noexec
#[derive(Facet)]
struct Record {
    data: String,
    #[facet(skip_deserializing, default)]
    computed_field: i32,
}
```

### `skip_serializing_if`

Conditionally skip serialization based on a predicate.

```rust,noexec
#[derive(Facet)]
struct User {
    name: String,

    #[facet(skip_serializing_if = Option::is_none)]
    email: Option<String>,

    #[facet(skip_serializing_if = Vec::is_empty)]
    tags: Vec<String>,

    #[facet(skip_serializing_if = |n| *n == 0)]
    count: i32,
}
```

### `skip_unless_truthy`

Conditionally skip serialization unless the value is truthy. Uses the type's registered truthiness predicate.

Truthiness is evaluated based on the type:
- **Booleans**: `true` is truthy, `false` is falsy
- **Numbers**: non-zero is truthy (for floats, also excludes NaN)
- **Collections** (Vec, String, slice, etc.): non-empty is truthy
- **Option**: `Some(_)` is truthy, `None` is falsy
- **Arrays**: non-zero-length arrays are truthy

```rust,noexec
#[derive(Facet)]
struct User {
    name: String,

    #[facet(skip_unless_truthy)]
    email: Option<String>,  // Omitted if None

    #[facet(skip_unless_truthy)]
    tags: Vec<String>,  // Omitted if empty

    #[facet(skip_unless_truthy)]
    bio: String,  // Omitted if empty
}
```

This is more ergonomic than `skip_serializing_if` when the type already has a natural notion of truthiness.

### `sensitive`

Mark a field as containing sensitive data. Tools like [`facet-pretty`](https://docs.rs/facet-pretty) will redact this field in debug output.

```rust,noexec
#[derive(Facet)]
struct Config {
    name: String,
    #[facet(sensitive)]
    api_key: String,  // Shown as [REDACTED] in debug output
}
```

### `flatten`

Flatten a nested struct's fields into the parent.

```rust,noexec
#[derive(Facet)]
struct Pagination {
    page: u32,
    per_page: u32,
}

#[derive(Facet)]
struct Query {
    search: String,
    #[facet(flatten)]
    pagination: Pagination,
}
// Serializes as: {"search": "...", "page": 1, "per_page": 10}
```

**Flatten with internally-tagged enums:**

You can use `#[facet(flatten)]` inside variants of internally-tagged enums. The flattened fields are merged with the variant's own fields:

```rust,noexec
#[derive(Facet)]
struct Base {
    name: String,
    value: i32,
}

#[derive(Facet)]
#[facet(tag = "type")]
#[repr(C)]
enum Message {
    #[facet(rename = "request")]
    Request {
        #[facet(flatten)]
        base: Base,
        method: String,
    },
    #[facet(rename = "response")]
    Response {
        #[facet(flatten)]
        base: Base,
    },
}

// Request serializes as:
// {"type": "request", "name": "...", "value": 42, "method": "GET"}
//
// Response serializes as:
// {"type": "response", "name": "...", "value": 42}
```

This pattern is useful for sharing common fields across enum variants while keeping the JSON structure flat.

### `child`

Mark a field as a child node for hierarchical formats like  XML.

```rust,noexec
#[derive(Facet)]
struct Document {
    title: String,
    #[facet(child)]
    sections: Vec<Section>,
}
```

### `metadata`

Mark a field as carrying metadata about the value, not part of the value itself. Used with `#[facet(metadata_container)]` to create transparent wrappers that preserve metadata.

```rust,noexec
#[derive(Facet)]
#[facet(metadata_container)]
struct Spanned<T> {
    value: T,
    #[facet(metadata = "span")]
    span: Option<Span>,
}

#[derive(Facet)]
#[facet(metadata_container)]
struct Documented<T> {
    value: T,
    #[facet(metadata = "doc")]
    doc: Option<Vec<String>>,
}
```

**The metadata kind string** identifies what type of metadata this field carries:

- `"span"` — Source location information (line, column, byte offset)
- `"doc"` — Documentation comments from the source
- Custom kinds for format-specific metadata

**How formats use metadata:**

Formats can query metadata fields during serialization/deserialization:

```rust,noexec
// In a format's serializer:
if let Some(doc_field) = struct_def.fields.iter()
    .find(|f| f.metadata_kind() == Some("doc"))
{
    // Access the doc field's value and emit it appropriately
    // e.g., as doc comments in Styx: /// line 1\n/// line 2
}
```

**Metadata fields are not serialized** in the normal field list — they're either:
1. Handled specially by formats that understand them (e.g., Styx emits doc comments)
2. Ignored by formats that don't support metadata (e.g., JSON)

See [`metadata_container`](#metadata_container) for complete usage examples.

### `invariants`

Validate type invariants after deserialization. The function takes `&self` and returns `bool` — returning `false` causes deserialization to fail.

```rust,noexec
#[derive(Facet)]
#[facet(invariants = validate_port)]
struct ServerConfig {
    port: u16,
}

fn validate_port(config: &ServerConfig) -> bool {
    config.port > 0 && config.port < 65535
}
```

**When is it called?** The invariant function is called when finalizing a `Partial` value — that is, when `partial.build()` is called after all fields have been set. At this point, the entire value is initialized and can be validated as a whole.

**Method syntax:** You can also use a method on the type itself:

```rust,noexec
#[derive(Facet)]
#[facet(invariants = Point::is_valid)]
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn is_valid(&self) -> bool {
        // Point must be in first quadrant
        self.x >= 0 && self.y >= 0
    }
}
```

**Multi-field invariants:** This is where invariants really shine — validating relationships between fields:

```rust,noexec
#[derive(Facet)]
#[facet(invariants = Range::is_valid)]
struct Range {
    min: u32,
    max: u32,
}

impl Range {
    fn is_valid(&self) -> bool {
        self.min <= self.max
    }
}
```

**With enums:** Enums themselves don't support invariants directly, but you can wrap them in a struct:

```rust,noexec
#[derive(Facet)]
#[repr(C)]
enum RangeKind {
    Low(u8),
    High(u8),
}

#[derive(Facet)]
#[facet(invariants = ValidatedRange::is_valid)]
struct ValidatedRange {
    range: RangeKind,
}

impl ValidatedRange {
    fn is_valid(&self) -> bool {
        match &self.range {
            RangeKind::Low(v) => *v <= 50,
            RangeKind::High(v) => *v > 50,
        }
    }
}
```

**Why this matters:** Invariants are crucial for types where certain field combinations are invalid. Without them, deserialization could produce values that violate your type's assumptions, potentially leading to logic errors or — in `unsafe` code — undefined behavior.

**Current limitation:** Invariants are only checked at the top level when building a `Partial`. Nested structs with their own invariants are not automatically validated when contained in a parent struct. If you need nested validation, add an invariant to the parent that explicitly checks nested values.

### `proxy`

Use a proxy type for serialization/deserialization. The proxy type handles the format representation while your actual type handles the domain logic.

**Required trait implementations:**
- `TryFrom<ProxyType> for FieldType` — for deserialization (proxy → actual)
- `TryFrom<&FieldType> for ProxyType` — for serialization (actual → proxy)

```rust,noexec
use facet::Facet;

// Your domain type
struct CustomId(u64);

// Proxy: serialize as a string with "ID-" prefix
#[derive(Facet)]
#[facet(transparent)]
struct CustomIdProxy(String);

impl TryFrom<CustomIdProxy> for CustomId {
    type Error = &'static str;
    fn try_from(proxy: CustomIdProxy) -> Result<Self, Self::Error> {
        let num = proxy.0.strip_prefix("ID-")
            .ok_or("missing ID- prefix")?
            .parse()
            .map_err(|_| "invalid number")?;
        Ok(CustomId(num))
    }
}

impl TryFrom<&CustomId> for CustomIdProxy {
    type Error = std::convert::Infallible;
    fn try_from(id: &CustomId) -> Result<Self, Self::Error> {
        Ok(CustomIdProxy(format!("ID-{}", id.0)))
    }
}

#[derive(Facet)]
struct Record {
    #[facet(proxy = CustomIdProxy)]
    id: CustomId,
}

// Serialization: actual type → proxy → JSON
let record = Record { id: CustomId(12345) };
let json = facet_json::to_string(&record);
assert_eq!(json, r#"{"id":"ID-12345"}"#);

// Deserialization: JSON → proxy → actual type
let parsed: Record = facet_json::from_str(&json).unwrap();
assert_eq!(parsed.id.0, 12345);
```

**Use cases for proxy:**

1. **Custom serialization format** — serialize numbers as strings, dates as timestamps, etc.
2. **Type conversion** — deserialize a string into a parsed type using `FromStr`
3. **Validation** — reject invalid values during `TryFrom` conversion
4. **Non-Facet types** — combine with `#[facet(opaque)]` for types that don't implement `Facet`

**Example: Delegate to `FromStr` and `Display`**

A common pattern is parsing string fields using a type's `FromStr` implementation. For example, parsing `"#ff00ff"` into a color struct:

```rust,noexec
use facet::Facet;
use std::str::FromStr;

/// A color type that can be parsed from hex strings like "#ff00ff"
#[derive(Debug, PartialEq)]
struct Color(u8, u8, u8);

impl FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix('#').unwrap_or(s);
        if s.len() != 6 {
            return Err("expected 6 hex digits".into());
        }
        let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| e.to_string())?;
        let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| e.to_string())?;
        let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| e.to_string())?;
        Ok(Color(r, g, b))
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

// Step 1: Create a transparent proxy that wraps String
#[derive(Facet)]
#[facet(transparent)]
struct ColorProxy(String);

// Step 2: Implement TryFrom for deserialization (Proxy → Color)
impl TryFrom<ColorProxy> for Color {
    type Error = String;

    fn try_from(proxy: ColorProxy) -> Result<Self, Self::Error> {
        Color::from_str(&proxy.0)  // Delegate to FromStr
    }
}

// Step 3: Implement TryFrom for serialization (Color → Proxy)
impl TryFrom<&Color> for ColorProxy {
    type Error = std::convert::Infallible;

    fn try_from(color: &Color) -> Result<Self, Self::Error> {
        Ok(ColorProxy(color.to_string()))  // Delegate to Display
    }
}

// Step 4: Use the proxy attribute on your field
#[derive(Facet)]
struct Theme {
    #[facet(proxy = ColorProxy)]
    foreground: Color,

    #[facet(proxy = ColorProxy)]
    background: Color,
}

// Serialization works in both directions:
let theme = Theme {
    foreground: Color(255, 0, 255),
    background: Color(0, 0, 0),
};
let json = facet_json::to_string(&theme);
assert_eq!(json, r#"{"foreground":"#ff00ff","background":"#000000"}"#);

// And deserialization:
let parsed: Theme = facet_json::from_str(&json).unwrap();
assert_eq!(parsed.foreground, Color(255, 0, 255));
```

The key insight: `#[facet(transparent)]` on the proxy makes it serialize as just a string (not `{"0": "..."}`), and the `TryFrom` impls handle the conversion in both directions.

**Example: Parse integers from hex strings:**

```rust,noexec
#[derive(Facet)]
#[facet(transparent)]
struct HexU64(String);

impl TryFrom<HexU64> for u64 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: HexU64) -> Result<Self, Self::Error> {
        let s = proxy.0.strip_prefix("0x").unwrap_or(&proxy.0);
        u64::from_str_radix(s, 16)
    }
}

impl TryFrom<&u64> for HexU64 {
    type Error = std::convert::Infallible;
    fn try_from(n: &u64) -> Result<Self, Self::Error> {
        Ok(HexU64(format!("0x{:x}", n)))
    }
}

#[derive(Facet)]
struct Pointer {
    #[facet(proxy = HexU64)]
    address: u64,
}

// Serialization: the address is formatted as hex
let ptr = Pointer { address: 0x7fff5fbff8c0 };
let json = facet_json::to_string(&ptr);
assert_eq!(json, r#"{"address":"0x7fff5fbff8c0"}"#);

// Deserialization: hex string is parsed back to u64
let parsed: Pointer = facet_json::from_str(&json).unwrap();
assert_eq!(parsed.address, 0x7fff5fbff8c0);
```

**Example: Nested proxy with opaque type:**

```rust,noexec
// Arc<T> with a custom serialization
#[derive(Facet)]
struct ArcU64Proxy { val: u64 }

impl TryFrom<ArcU64Proxy> for std::sync::Arc<u64> {
    type Error = std::convert::Infallible;
    fn try_from(proxy: ArcU64Proxy) -> Result<Self, Self::Error> {
        Ok(std::sync::Arc::new(proxy.val))
    }
}

impl TryFrom<&std::sync::Arc<u64>> for ArcU64Proxy {
    type Error = std::convert::Infallible;
    fn try_from(arc: &std::sync::Arc<u64>) -> Result<Self, Self::Error> {
        Ok(ArcU64Proxy { val: **arc })
    }
}

#[derive(Facet)]
struct Container {
    #[facet(opaque, proxy = ArcU64Proxy)]
    counter: std::sync::Arc<u64>,
}

// Serialization: Arc<u64> → ArcU64Proxy → JSON object
let container = Container { counter: std::sync::Arc::new(42) };
let json = facet_json::to_string(&container);
assert_eq!(json, r#"{"counter":{"val":42}}"#);

// Deserialization: JSON object → ArcU64Proxy → Arc<u64>
let parsed: Container = facet_json::from_str(&json).unwrap();
assert_eq!(*parsed.counter, 42);
```

### Format-specific proxies

Sometimes a type needs different serialization representations for different formats. For example, you might want hex strings in JSON but binary strings in a custom format.

Use the format namespace syntax: `#[facet(json::proxy = JsonProxy)]`, `#[facet(xml::proxy = XmlProxy)]`, etc.

**Resolution order:**
1. Format-specific proxy (e.g., `json::proxy` when serializing JSON)
2. Format-agnostic proxy (`proxy`)
3. Normal serialization (no proxy)

```rust,noexec
use facet::Facet;

/// Proxy for JSON: serialize as hex string
#[derive(Facet)]
#[facet(transparent)]
struct HexProxy(String);

impl TryFrom<HexProxy> for u32 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: HexProxy) -> Result<Self, Self::Error> {
        let s = proxy.0.trim_start_matches("0x");
        u32::from_str_radix(s, 16)
    }
}

impl From<&u32> for HexProxy {
    fn from(v: &u32) -> Self {
        HexProxy(format!("0x{:x}", v))
    }
}

/// Proxy for other formats: serialize as decimal string
#[derive(Facet)]
#[facet(transparent)]
struct DecimalProxy(String);

impl TryFrom<DecimalProxy> for u32 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: DecimalProxy) -> Result<Self, Self::Error> {
        proxy.0.parse()
    }
}

impl From<&u32> for DecimalProxy {
    fn from(v: &u32) -> Self {
        DecimalProxy(v.to_string())
    }
}

#[derive(Facet)]
struct Config {
    name: String,
    #[facet(json::proxy = HexProxy)]  // Use hex in JSON
    #[facet(proxy = DecimalProxy)]     // Use decimal elsewhere
    port: u32,
}

// JSON serialization uses hex:
// {"name":"app","port":"0x1f90"}

// Other formats use decimal:
// name: app
// port: "8080"
```

**Use cases:**
- Different encoding requirements per format (hex vs binary vs base64)
- XML attributes need strings, JSON can use native types
- Legacy format compatibility with different representations

**Note:** For a format to support format-specific proxies, its parser/serializer must implement `format_namespace()` to return its namespace. The following built-in format crates support this:
- `facet-json` → `"json"` (use `#[facet(json::proxy = ...)]`)
- `facet-xml` → `"xml"` (use `#[facet(xml::proxy = ...)]`)
- `facet-html` → `"html"` (use `#[facet(html::proxy = ...)]`)

## Extension attributes

Format crates can define their own namespaced attributes with compile-time validation and helpful error messages.

### Using extension attributes

The namespace comes from how you import the crate:

```rust,noexec
use facet::Facet;
use facet_xml as xml;
use facet_args as args;

#[derive(Facet)]
struct Config {
    #[facet(xml::element)]
    server: Server,

    #[facet(xml::attribute)]
    name: String,
}

#[derive(Facet)]
struct Cli {
    #[facet(args::positional)]
    input: String,

    #[facet(args::named, args::short = 'o')]
    output: Option<String>,
}
```

### Available namespaces

| Crate | Namespace | Attributes |
|-------|-----------|------------|
| [`facet-args`](https://docs.rs/facet-args) | `args` | `positional`, `named`, `short`, `subcommand` |
| [`facet-xml`](https://docs.rs/facet-xml) | `xml` | `element`, `elements`, `attribute`, `text`, `tag`, `ns`, `ns_all`, `proxy` |
| [`facet-html`](https://docs.rs/facet-html) | `html` | `element`, `elements`, `attribute`, `text`, `tag`, `custom_element`, `proxy` |
| [`facet-yaml`](https://docs.rs/facet-yaml) | `serde` | `rename` |
| [`facet-json`](https://docs.rs/facet-json) | `json` | `proxy` |

**Note:** The `proxy` attribute is available for any format namespace (e.g., `json::proxy`, `xml::proxy`). It uses the same syntax as the format-agnostic `proxy` attribute but only applies when serializing/deserializing with that specific format.

### Creating your own

Use [`define_attr_grammar!`](https://docs.rs/facet/latest/facet/macro.define_attr_grammar.html):

```rust,noexec
facet::define_attr_grammar! {
    ns "myformat";
    crate_path ::my_format_crate;

    pub enum Attr {
        /// Description shown in error messages
        MyAttribute,
        /// Attribute with a value
        WithValue(&'static str),
    }
}
```

Typos produce helpful compile errors:

```
error: unknown attribute `chld`, did you mean `child`?
       available attributes: child, children, property, argument
```

See [Extend → Extension Attributes](@/extend/extension-attributes.md) for the complete guide.
