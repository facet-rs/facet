+++
title = "FAQ"
weight = 7
insert_anchor_links = "heading"
+++

Frequently asked questions about facet.

## General

### What is facet?

facet is a reflection library for Rust. It provides runtime type information through a single derive macro, enabling serialization, pretty-printing, diffing, and more — all without code generation per format.

### How is facet different from serde?

The key difference is in the approach:

- **serde** generates specialized code for each type × format combination (monomorphization)
- **facet** generates data (shapes) that format crates read at runtime

This means facet trades some speed for features: rich error diagnostics, one derive for many tools, and smaller binaries in format-heavy applications.

See [Why facet?](@/guide/why.md) for a detailed comparison.

### Is facet faster than serde?

No. serde's monomorphization allows the compiler to optimize serialization code per-type. facet uses runtime reflection, which is inherently slower.

If you're serializing millions of objects per second in a hot loop, serde is the better choice. For most applications, the difference is negligible and facet's features may be worth the tradeoff.

### Does facet support `no_std`?

Yes. [`facet-core`](https://docs.rs/facet-core) is `no_std` compatible. Format crates typically require `alloc`.

### What rust version does facet require?

facet targets the latest stable Rust. Check the CI configuration for the current MSRV (minimum supported Rust version).

### Why doesn't `Shape` implement `Facet`?

`Shape` contains types that cannot be serialized or deserialized:

1. **Function pointers** — stored in attributes like `default`, `skip_serializing_if`, and `invariants`
2. **`ConstTypeId`** — compile-time type identifiers that have no meaningful serialized form

If you need a generic value type that can hold any facet-compatible data, use [`facet_value::Value`](https://docs.rs/facet-value).

If you need to compare, diff, or store shapes at runtime, see [facet-shapelike](https://docs.rs/facet-shapelike). This crate provides `ShapeLike`, a fully owned and serializable version of `Shape`. It omits function pointers, `TypeId`, and vtables — data that would be meaningless or unsafe across process boundaries.

## Usage

### How do I deserialize from JSON?

```rust,noexec
use facet::Facet;
use facet_json::from_str;

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}

let person: Person = from_str(r#"{"name": "Alice", "age": 30}"#)?;
```

### How do I serialize to JSON?

```rust,noexec
use facet::Facet;
use facet_json::to_string;

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}

let person = Person { name: "Alice".into(), age: 30 };
let json = to_string(&person);
```

### How do I handle optional fields?

Use `Option<T>` and optionally `skip_serializing_if`:

```rust,noexec
#[derive(Facet)]
struct User {
    name: String,
    #[facet(skip_serializing_if = Option::is_none)]
    email: Option<String>,
}
```

### How do I rename fields?

Use `rename` for individual fields or `rename_all` for all fields:

```rust,noexec
#[derive(Facet)]
#[facet(rename_all = "camelCase")]
struct Config {
    #[facet(rename = "serverHost")]  // Override rename_all
    host: String,
    server_port: u16,  // Becomes "serverPort"
}
```

### How do I use a default value for missing fields?

```rust,noexec
#[derive(Facet)]
struct Config {
    name: String,
    #[facet(default = 8080)]
    port: u16,
}
```

### How do I skip a field?

```rust,noexec
#[derive(Facet)]
struct User {
    name: String,
    #[facet(skip, default)]  // Must have a default
    internal_id: u64,
}
```

### How do I flatten nested structs?

```rust,noexec
#[derive(Facet)]
struct Inner {
    x: i32,
    y: i32,
}

#[derive(Facet)]
struct Outer {
    name: String,
    #[facet(flatten)]
    coords: Inner,
}
// Serializes as: {"name": "...", "x": 1, "y": 2}
```

### How do I handle enums?

facet supports multiple enum representations:

```rust,noexec
// Externally tagged (default)
#[derive(Facet)]
enum Message {
    Text(String),
    Data { bytes: Vec<u8> },
}
// {"Text": "hello"} or {"Data": {"bytes": [1, 2, 3]}}

// Internally tagged
#[derive(Facet)]
#[facet(tag = "type")]
enum Event {
    Click { x: i32, y: i32 },
    KeyPress { key: String },
}
// {"type": "Click", "x": 10, "y": 20}

// Adjacently tagged
#[derive(Facet)]
#[facet(tag = "t", content = "c")]
enum Value {
    Int(i32),
    String(String),
}
// {"t": "Int", "c": 42}

// Untagged
#[derive(Facet)]
#[facet(untagged)]
enum AnyValue {
    Int(i32),
    Float(f64),
    String(String),
}
// Just the value: 42 or 3.14 or "hello"
```

### Can I use flatten inside enum variants?

Yes! You can combine `#[facet(flatten)]` with internally-tagged enums to share common fields across variants:

```rust,noexec
#[derive(Facet)]
struct CommonFields {
    id: u64,
    timestamp: String,
}

#[derive(Facet)]
#[facet(tag = "type")]
#[repr(C)]
enum Event {
    UserLogin {
        #[facet(flatten)]
        common: CommonFields,
        username: String,
    },
    PageView {
        #[facet(flatten)]
        common: CommonFields,
        url: String,
    },
}
// UserLogin: {"type": "UserLogin", "id": 1, "timestamp": "...", "username": "alice"}
// PageView: {"type": "PageView", "id": 2, "timestamp": "...", "url": "/home"}
```

See [Attributes Reference](@/reference/attributes/_index.md#field-attributes--flatten) for more details.

## Error handling

### Why do I get "unknown field" errors?

By default, facet ignores unknown fields. Add `deny_unknown_fields` to make them errors:

```rust,noexec
#[derive(Facet)]
#[facet(deny_unknown_fields)]
struct Config {
    name: String,
}
```

### How do I validate data after deserialization?

Use the `invariants` attribute:

```rust,noexec
#[derive(Facet)]
#[facet(invariants = validate)]
struct Port(u16);

fn validate(port: &Port) -> bool {
    port.0 > 0 && port.0 < 65535
}
```

## Compatibility

### Can I use facet with serde?

Not directly. facet has its own derive macro and format crates. However, you can have both derives on a type:

```rust,noexec
#[derive(Facet, serde::Serialize, serde::Deserialize)]
struct MyType {
    // ...
}
```

This lets you migrate incrementally or use serde for formats facet doesn't support yet.

### Does facet support all serde attributes?

Most common attributes have equivalents. See [Comparison with serde](@/guide/serde/_index.md) for a mapping.

Some serde features like `#[serde(borrow)]` don't have direct equivalents due to architectural differences.

### Can I implement `Facet` manually?

Yes, but it's `unsafe` and requires careful attention to invariants. See the [Contribute guide](/contribute/) for details on the type system.

For most cases, use `#[derive(Facet)]` or `#[facet(opaque)]` for types that can't be derived.

## Troubleshooting

### "the trait `Facet` is not implemented for..."

Ensure the type either:
1. Derives `Facet`: `#[derive(Facet)]`
2. Has a built-in implementation (std types like `String`, `Vec<T>`, etc.)
3. Is behind a feature flag (check crate docs)

### "cannot find attribute `facet` in this scope"

Make sure you're using the derive macro:

```rust,noexec
use facet::Facet;

#[derive(Facet)]  // This enables #[facet(...)] attributes
struct MyType { ... }
```

### Compile errors mention extension attributes i'm not using

Extension attributes require importing the crate with an alias:

```rust,noexec
use facet_xml as xml;  // Enables xml:: prefix

#[derive(Facet)]
struct Config {
    #[facet(xml::property)]  // Now works
    name: String,
}
```

Without the import, `xml::property` is not recognized.

### Why do I get errors with recursive data types?

Facet requires explicit annotation for recursive types. Use `#[facet(recursive_type)]` on fields that create cycles:

```rust,noexec
#[derive(Facet)]
struct Node {
    #[facet(recursive_type)] 
    lhs: Option<Box<Node>>,
    #[facet(recursive_type)] 
    rhs: Option<Box<Node>>,
}
```


## Still have questions?

- Join the [Discord](https://discord.gg/JhD7CwCJ8F) to chat with the community
- Check the [Showcases](@/showcases/_index.md) for more examples
- Browse the [API documentation](https://docs.rs/facet) for detailed type information
- Open an [issue](https://github.com/facet-rs/facet/issues) if you've found a bug
