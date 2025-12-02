+++
title = "Attributes Reference"
weight = 2
insert_anchor_links = "heading"
+++

Complete reference for `#[facet(...)]` attributes.

## Container Attributes

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

Mark a container as opaque — inner fields don't need to implement `Facet`. Useful for types that shouldn't be inspected.

```rust
#[derive(Facet)]
#[facet(opaque)]
struct InternalState {
    // Fields don't need Facet
}
```

### `type_tag`

Add a type identifier for self-describing formats.

```rust
#[derive(Facet)]
#[facet(type_tag = "com.example.User")]
struct User {
    name: String,
}
```

## Enum Attributes

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

## Field Attributes

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

Validate type invariants after deserialization.

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

### `proxy`

Use a proxy type for serialization/deserialization. The proxy type must implement the appropriate `TryFrom` conversions.

```rust
#[derive(Facet)]
struct Record {
    #[facet(proxy = String)]
    id: CustomId,  // Serialized as String, converted via TryFrom
}
```

## Extension Attributes

Format crates can define their own namespaced attributes. See the [Extend guide](/extend/) for details.

### KDL Attributes

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
```

### Args Attributes

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

## Next Steps

- Check the [Showcases](@/learn/showcases/_index.md) to see these attributes in action
- Read [Comparison with serde](@/learn/migration/_index.md) if you're migrating
- See the [Extend guide](/extend/) to create your own extension attributes
