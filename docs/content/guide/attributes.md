+++
title = "Attributes Reference"
weight = 2
insert_anchor_links = "heading"
+++

Complete reference for `#[facet(...)]` attributes.

## Container attributes

These attributes apply to structs and enums.

### `deny_unknown_fields`

Produce an error when encountering unknown fields during deserialization. By default, unknown fields are silently ignored.

```rust
#[derive(Facet)]
#[facet(deny_unknown_fields)]
struct Config {
    name: String,
    port: u16,
}
```

### `default`

Use the type's `Default` implementation for missing fields during deserialization.

```rust
#[derive(Facet, Default)]
#[facet(default)]
struct Config {
    name: String,
    port: u16,  // Will use Default if missing
}
```

### `rename_all`

Rename all fields/variants using a case convention.

```rust
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

```rust
#[derive(Facet)]
#[facet(transparent)]
struct UserId(u64);  // Serialized as just the u64
```

### `opaque`

Mark a type as opaque — its inner structure is hidden from facet. The type itself implements `Facet`, but its fields are not inspected or serialized. This is useful for:

- Types with fields that don't implement `Facet`
- Types whose internal structure shouldn't be exposed
- Wrapper types around FFI or unsafe internals

```rust
#[derive(Facet)]
#[facet(opaque)]
struct InternalState {
    handle: *mut c_void,  // Doesn't need Facet
    cache: SomeNonFacetType,
}
```

**Important:** Opaque types cannot be serialized or deserialized on their own — use them with `#[facet(proxy = ...)]` to provide a serializable representation:

```rust
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

**When `assert_same!` encounters an opaque type**, it returns `Sameness::Opaque` — you cannot structurally compare opaque values.

### `type_tag`

Add a type identifier for self-describing formats.

```rust
#[derive(Facet)]
#[facet(type_tag = "com.example.User")]
struct User {
    name: String,
}
```

### `crate`

Specify a custom path to the facet crate. This is primarily useful for crates that re-export facet and want users to derive `Facet` without adding facet as a direct dependency.

```rust
// In a crate that re-exports facet
use other_crate::facet;

#[derive(other_crate::facet::Facet)]
#[facet(crate = other_crate::facet)]
struct MyStruct {
    field: u32,
}
```

This attribute can also be used with enums and all struct variants:

```rust
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

```rust
#[derive(Facet)]
#[facet(untagged)]
enum Value {
    Int(i64),
    Float(f64),
    String(String),
}
```

### `tag`

Use internal tagging — the variant name becomes a field inside the object.

```rust
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

```rust
#[derive(Facet)]
#[facet(tag = "t", content = "c")]
enum Message {
    Text(String),
    Data(Vec<u8>),
}
// {"t": "Text", "c": "hello"}
```

## Field attributes

These attributes apply to struct fields.

### `rename`

Rename a field during serialization/deserialization.

```rust
#[derive(Facet)]
struct User {
    #[facet(rename = "user_name")]
    name: String,
}
```

### `default`

Use a default value when the field is missing during deserialization.

```rust
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

```rust
#[derive(Facet)]
struct Session {
    id: String,
    #[facet(skip, default)]
    internal_state: InternalState,
}
```

### `skip_serializing`

Skip this field during serialization only.

```rust
#[derive(Facet)]
struct User {
    name: String,
    #[facet(skip_serializing)]
    password_hash: String,
}
```

### `skip_deserializing`

Skip this field during deserialization (uses default value).

```rust
#[derive(Facet)]
struct Record {
    data: String,
    #[facet(skip_deserializing, default)]
    computed_field: i32,
}
```

### `skip_serializing_if`

Conditionally skip serialization based on a predicate.

```rust
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

### `sensitive`

Mark a field as containing sensitive data. Tools like [`facet-pretty`](https://docs.rs/facet-pretty) will redact this field in debug output.

```rust
#[derive(Facet)]
struct Config {
    name: String,
    #[facet(sensitive)]
    api_key: String,  // Shown as [REDACTED] in debug output
}
```

### `flatten`

Flatten a nested struct's fields into the parent.

```rust
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

### `child`

Mark a field as a child node for hierarchical formats like KDL or XML.

```rust
use facet_kdl as kdl;

#[derive(Facet)]
struct Document {
    title: String,
    #[facet(child)]
    sections: Vec<Section>,
}
```

### `invariants`

Validate type invariants after deserialization. The function takes `&self` and returns `bool` — returning `false` causes deserialization to fail.

```rust
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

```rust
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

```rust
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

```rust
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

```rust
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

```rust
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

```rust
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

```rust
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

## Extension attributes

Format crates can define their own namespaced attributes. See the [Extend guide](/extend/) for details.

### KDL attributes

```rust
use facet_kdl as kdl;

#[derive(Facet)]
struct Dependency {
    #[facet(kdl::node_name)]
    name: String,

    #[facet(kdl::argument)]
    version: String,

    #[facet(kdl::property)]
    features: Vec<String>,
}

// For children collections, you can specify a custom node name:
#[derive(Facet)]
struct Config {
    // Matches "dependency" nodes (auto-singularized from field name)
    #[facet(kdl::children)]
    dependencies: Vec<Dependency>,

    // Matches "extra" nodes (custom node name)
    #[facet(kdl::children = "extra")]
    extras: Vec<Extra>,
}
```

### Args attributes

```rust
use facet_args as args;

#[derive(Facet)]
struct Cli {
    #[facet(args::positional)]
    input: String,

    #[facet(args::named, args::short = 'o')]
    output: Option<String>,
}
```

See each format crate's documentation for available extension attributes.

## Next steps

- Check the [Showcases](@/guide/showcases/_index.md) to see these attributes in action
- Read [Comparison with serde](@/guide/migration/_index.md) if you're migrating
- See the [Extend guide](/extend/) to create your own extension attributes
