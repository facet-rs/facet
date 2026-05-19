+++
title = "Field attributes"
weight = 3
insert_anchor_links = "heading"
+++

`#[facet(...)]` attributes that apply to individual struct fields.

## `rename`

Rename a field during serialization/deserialization.

```rust,noexec
#[derive(Facet)]
struct User {
    #[facet(rename = "user_name")]
    name: String,
}
```

## `default`

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

## `skip`

Skip this field entirely during both serialization and deserialization. The field must have a default value.

```rust,noexec
#[derive(Facet)]
struct Session {
    id: String,
    #[facet(skip, default)]
    internal_state: InternalState,
}
```

## `skip_serializing`

Skip this field during serialization only.

```rust,noexec
#[derive(Facet)]
struct User {
    name: String,
    #[facet(skip_serializing)]
    password_hash: String,
}
```

## `skip_deserializing`

Skip this field during deserialization (uses default value).

```rust,noexec
#[derive(Facet)]
struct Record {
    data: String,
    #[facet(skip_deserializing, default)]
    computed_field: i32,
}
```

## `skip_serializing_if`

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

## `skip_unless_truthy`

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

## `sensitive`

Mark a field as containing sensitive data. Tools like [`facet-pretty`](https://docs.rs/facet-pretty) will redact this field in debug output.

```rust,noexec
#[derive(Facet)]
struct Config {
    name: String,
    #[facet(sensitive)]
    api_key: String,  // Shown as [REDACTED] in debug output
}
```

## `flatten`

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

## `trailing`

Mark an opaque field as structurally trailing in its container. Formats that support trailing payloads can treat this field as "remaining bytes" rather than requiring an outer length frame.

```rust,noexec
#[derive(Facet)]
struct Packet {
    tag: u8,
    len: u16,
    #[facet(opaque, trailing)]
    payload: Vec<u8>,
}
```

`#[facet(trailing)]` is checked at compile time and must satisfy all of these rules:

1. It does not accept arguments (`#[facet(trailing)]`, not `#[facet(trailing = ...)]`).
2. The field must be the last field in its container.
3. The field cannot also be `#[facet(flatten)]`.
4. The field must be opaque, either directly (`#[facet(opaque)]`) or via its field type.

## `child`

Mark a field as a child node for hierarchical formats like  XML.

```rust,noexec
#[derive(Facet)]
struct Document {
    title: String,
    #[facet(child)]
    sections: Vec<Section>,
}
```

## `metadata`

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

See [`metadata_container`](@/reference/container-attributes.md#metadata-container) for complete usage examples.

## `invariants`

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

## `proxy`

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

## Format-specific proxies

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

---

See also: [Container attributes](@/reference/container-attributes.md) · [Enum & variant attributes](@/reference/enum-attributes.md) · [Extension attributes](@/reference/extension-attributes.md)
