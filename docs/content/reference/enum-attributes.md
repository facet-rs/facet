+++
title = "Enum & variant attributes"
weight = 2
insert_anchor_links = "heading"
+++

`#[facet(...)]` attributes that control how enums are tagged and how individual
variants behave. `untagged`, `tag`, and `tag` + `content` apply to the enum;
`other` applies to a single variant.

## `untagged`

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

### Variant matching order

For formats with typed values (JSON, YAML, MessagePack), variants are tried in **definition order**. The first matching variant wins. In the example above, a JSON `42` matches `Int(i64)` because integers match `i64`.

### Text-based formats (XML)

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

## `tag`

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

## `tag` + `content`

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

## `other`

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

### Capturing variant tags with `#[facet(tag)]` and `#[facet(content)]`

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

---

See also: [Container attributes](@/reference/container-attributes.md) · [Field attributes](@/reference/field-attributes.md) · [Extension attributes](@/reference/extension-attributes.md)
