+++
title = "Dynamic Values"
weight = 4
insert_anchor_links = "heading"
+++

Facet supports dynamic, schema-less data through `facet_value::Value` — a pointer-sized type that can hold any structured data. This enables powerful patterns like two-phase deserialization, mixed static/dynamic comparisons, and deferred parsing.

## facet-value: the dynamic value type

[`facet_value::Value`](https://docs.rs/facet-value) is facet's equivalent to `serde_json::Value`, but format-agnostic and more memory-efficient. It supports eight value types: Null, Bool, Number, String, Bytes, Array, Object, and DateTime.

### Deserialize to value, then to a type

A common pattern is to deserialize into `Value` first, then convert to a concrete type later. This is useful when:

- You don't know the schema at compile time
- You want to inspect the data before choosing a target type
- You're building configuration layers that merge multiple sources

```rust,noexec
use facet::Facet;
use facet_value::{Value, from_value};

#[derive(Facet, Debug)]
struct Config {
    host: String,
    port: u16,
}

// First, deserialize JSON into a dynamic Value
let json = r#"{"host": "localhost", "port": 8080}"#;
let value: Value = facet_json::from_str(json)?;

// Inspect or transform the value if needed
println!("Keys: {:?}", value.as_object().map(|o| o.keys().collect::<Vec<_>>()));

// Then convert to a concrete type
let config: Config = from_value(&value)?;
println!("{:?}", config);
```

### Serialize a type to Value

The inverse operation — converting a typed value to a `Value` — is done with `to_value`. This is useful when you want to manipulate data dynamically or serialize through a common format:

```rust,noexec
use facet::Facet;
use facet_value::{to_value, from_value, Value};

#[derive(Facet, Debug, PartialEq)]
struct Config {
    host: String,
    port: u16,
}

let config = Config { host: "localhost".into(), port: 8080 };

// Convert to a dynamic Value
let value: Value = to_value(&config)?;

// Now you can inspect or modify it
let obj = value.as_object().unwrap();
assert_eq!(obj.get("host").unwrap().as_string().unwrap().as_str(), "localhost");
assert_eq!(obj.get("port").unwrap().as_number().unwrap().to_u64(), Some(8080));

// And convert back if needed
let config2: Config = from_value(value)?;
assert_eq!(config, config2);
```

### Partial tree extraction

You can also extract just part of a `Value` tree into a typed struct:

```rust,noexec
use facet::Facet;
use facet_value::{Value, from_value};

#[derive(Facet, Debug)]
struct DatabaseConfig {
    host: String,
    port: u16,
}

let json = r#"{
    "app": {"name": "myapp"},
    "database": {"host": "db.example.com", "port": 5432},
    "logging": {"level": "info"}
}"#;

let value: Value = facet_json::from_str(json)?;

// Extract just the database section
if let Some(db_value) = value.as_object().and_then(|o| o.get("database")) {
    let db_config: DatabaseConfig = from_value(db_value)?;
    println!("Database: {:?}", db_config);
}
```

## assert_same!: compare anything to anything

[`facet_assert::assert_same!`](https://docs.rs/facet-assert) compares values structurally using reflection — no `PartialEq` required. The powerful part? **It can compare values of different types**, including a `Value` against a typed struct.

### Dynamic vs typed comparison

This is incredibly useful for testing: verify that your JSON matches the expected typed structure without manually constructing the typed value:

```rust,noexec
use facet::Facet;
use facet_assert::assert_same;
use facet_value::Value;

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
}

// Your actual typed value
let user = User { name: "Alice".into(), age: 30 };

// Expected data as a dynamic Value (maybe from a test fixture)
let expected: Value = facet_json::from_str(r#"{"name": "Alice", "age": 30}"#)?;

// Compare them directly — different types, same structure!
assert_same!(user, expected);
```

### Cross-Version DTO comparison

Compare DTOs across API versions without implementing `PartialEq` between them:

```rust,noexec
#[derive(Facet)]
struct UserV1 { name: String, age: u32 }

#[derive(Facet)]
struct UserV2 { name: String, age: u32 }  // Same fields, different type

let v1 = UserV1 { name: "Bob".into(), age: 25 };
let v2 = UserV2 { name: "Bob".into(), age: 25 };

assert_same!(v1, v2);  // Works! Compares by structure, not type
```

### Rich diff output

When values differ, you get a detailed, colored diff showing exactly what's different:

```
assertion `assert_same!(left, right)` failed

  .name: "Alice" → "Bob"
  .age: 30 → 25
```

## RawJson: defer parsing

[`facet_json::RawJson`](https://docs.rs/facet-json) captures unparsed JSON text, letting you delay or skip deserialization of parts of a document. The JSON text can be borrowed (zero-copy) or owned.

### Use cases

- **Unknown schema**: Part of your JSON has an unknown or highly variable structure
- **Pass-through**: You need to store/forward JSON without parsing it
- **Lazy parsing**: Defer expensive parsing until you know you need it
- **Selective parsing**: Only parse the parts you care about

### Basic usage

```rust,noexec
use facet::Facet;
use facet_json::RawJson;

#[derive(Facet, Debug)]
struct ApiResponse<'a> {
    status: u32,
    // We don't know what shape `data` has, so keep it as raw JSON
    data: RawJson<'a>,
}

let json = r#"{"status": 200, "data": {"nested": [1, 2, 3], "complex": true}}"#;
let response: ApiResponse = facet_json::from_str(json)?;

assert_eq!(response.status, 200);
// The data field is still raw JSON text, not parsed
assert_eq!(response.data.as_str(), r#"{"nested": [1, 2, 3], "complex": true}"#);
```

### Zero-Copy borrowing

When possible, `RawJson` borrows from the input string (zero allocation):

```rust,noexec
use facet_json::RawJson;
use std::borrow::Cow;

#[derive(Facet)]
struct Envelope<'a> {
    kind: String,
    payload: RawJson<'a>,  // Borrows from input when possible
}

let json = r#"{"kind": "event", "payload": {"type": "click", "x": 100}}"#;
let envelope: Envelope = facet_json::from_str(json)?;

// payload.0 is Cow::Borrowed — no allocation for the payload!
```

### Owned rawJson

If you need to outlive the input, convert to owned:

```rust,noexec
let owned: RawJson<'static> = envelope.payload.into_owned();
```

### Later parsing

Parse the raw JSON when you're ready:

```rust,noexec
#[derive(Facet)]
struct ClickEvent {
    r#type: String,
    x: i32,
}

// Parse the raw JSON into a concrete type
let event: ClickEvent = facet_json::from_str(response.data.as_str())?;
```

## Combining these patterns

These features compose naturally:

```rust,noexec
use facet::Facet;
use facet_assert::assert_same;
use facet_json::RawJson;
use facet_value::{Value, from_value};

#[derive(Facet)]
struct Wrapper<'a> {
    version: u32,
    data: RawJson<'a>,  // Defer parsing
}

#[derive(Facet, Debug)]
struct Payload {
    items: Vec<String>,
}

// Parse outer structure, defer inner
let json = r#"{"version": 1, "data": {"items": ["a", "b", "c"]}}"#;
let wrapper: Wrapper = facet_json::from_str(json)?;

// Parse inner to Value for inspection
let data_value: Value = facet_json::from_str(wrapper.data.as_str())?;

// Convert to typed when ready
let payload: Payload = from_value(&data_value)?;

// Verify against expected
let expected = Payload { items: vec!["a".into(), "b".into(), "c".into()] };
assert_same!(payload, expected);
```

## Next steps

- See the [Assertions showcase](@/showcases/assert.md) for `assert_same!` examples
- Check out [facet-value on docs.rs](https://docs.rs/facet-value) for the full API
