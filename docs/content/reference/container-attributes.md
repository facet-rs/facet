+++
title = "Container attributes"
weight = 1
insert_anchor_links = "heading"
+++

`#[facet(...)]` attributes that apply to a struct or enum as a whole.

## `deny_unknown_fields`

Produce an error when encountering unknown fields during deserialization. By default, unknown fields are silently ignored.

```rust,noexec
#[derive(Facet)]
#[facet(deny_unknown_fields)]
struct Config {
    name: String,
    port: u16,
}
```

## `default`

Use the type's `Default` implementation for missing fields during deserialization.

```rust,noexec
#[derive(Facet, Default)]
#[facet(default)]
struct Config {
    name: String,
    port: u16,  // Will use Default if missing
}
```

## `rename_all`

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

## `transparent`

Forward serialization/deserialization to the inner type. Used for newtype patterns.

```rust,noexec
#[derive(Facet)]
#[facet(transparent)]
struct UserId(u64);  // Serialized as just the u64
```

## `metadata_container`

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

**Supported metadata kinds:**

| Kind | Expected type | Description |
|------|---------------|-------------|
| `"span"` | `Option<facet_reflect::Span>` | Source location (byte offset and length). `Span` has `offset: u32` and `len: u32` fields. |
| `"doc"` | `Option<Vec<S>>` | Documentation comments as lines (without the `///` prefix). `S` can be `String`, `Cow<str>`, or any string-like type. |
| `"tag"` | `Option<S>` | Type tag for formats that support tagged values (like [Styx](https://github.com/bearcove/styx)'s `@string`). `S` can be `String`, `Cow<str>`, etc. |

**Serialization behavior:**

During serialization, the container is transparent — `Documented<String>` serializes exactly like `String`. However, formats that support metadata (like [Styx](https://github.com/bearcove/styx)) can access the metadata fields and emit them appropriately (e.g., as doc comments).

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

**Deserialization behavior:**

During deserialization, the container is also transparent — the deserializer reads the value field normally and populates metadata fields from format-specific sources:

- **`span`** — Populated from the parser's position information (byte offset and length in the source)
- **`doc`** — Populated from doc comments in formats that support them (like [Styx](https://github.com/bearcove/styx)'s `///` comments)
- **`tag`** — Populated from type tags in formats that support them (like Styx's `@string` patterns)

For formats that don't provide metadata (like JSON), the metadata fields receive their default values (typically `None`).

```rust,noexec
// Parsing this Styx input:
// /// The application name
// name "myapp"

// Into this type:
#[derive(Facet)]
struct Config {
    name: Documented<String>,
}

// Results in:
// Config {
//     name: Documented {
//         value: "myapp".to_string(),
//         doc: Some(vec!["The application name".to_string()]),
//     }
// }
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

## `opaque`

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

**Note:** Field-level `#[facet(opaque)]` supports borrowed field types via an internal
lifetime-aware wrapper. Direct use of `facet::Opaque<T>` still requires `T: 'static`,
which keeps `Poke::get_mut` sound.

**When `assert_same!` encounters an opaque type**, it returns `Sameness::Opaque` — you cannot structurally compare opaque values.

### Opaque adapter (`#[facet(opaque = AdapterType)]`)

For container-level opaque types, you can provide an adapter instead of using `proxy`. This gives explicit control over how opaque bytes are mapped during serialization and deserialization.

```rust,noexec
use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst};

#[derive(Facet)]
#[facet(opaque = PayloadAdapter)]
struct Payload<'a>(&'a [u8]);

struct PayloadAdapter;

impl FacetOpaqueAdapter for PayloadAdapter {
    type Error = String;
    type SendValue<'a> = Payload<'a>;
    type RecvValue<'de> = Payload<'de>;

    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
        OpaqueSerialize {
            ptr: PtrConst::new(&value.0 as *const &[u8]),
            shape: <&[u8] as Facet>::SHAPE,
        }
    }

    fn deserialize_build<'de>(
        input: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error> {
        Ok(match input {
            OpaqueDeserialize::Borrowed(bytes) => Payload(bytes),
            OpaqueDeserialize::Owned(_) => {
                return Err("expected borrowed bytes".to_string());
            }
        })
    }
}
```

When forwarding opaque payloads that are already postcard-encoded for postcard,
adapters can return `facet_postcard::opaque_encoded_borrowed(bytes)` (or
`opaque_encoded_owned`) instead of a mapped typed value.

Rules and notes:

1. `#[facet(opaque = ...)]` is currently container-level only (not supported on fields).
2. The adapter type must implement `FacetOpaqueAdapter`.
3. This is an alternative to `proxy` for opaque values.
4. `#[facet(trailing)]` accepts either a field marked `#[facet(opaque)]` or a field type whose shape has a container-level opaque adapter.

## `pod`

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

## `skip_all_unless_truthy`

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

## `type_tag`

Add a type identifier for self-describing formats.

```rust,noexec
#[derive(Facet)]
#[facet(type_tag = "com.example.User")]
struct User {
    name: String,
}
```

## `crate`

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

---

See also: [Field attributes](@/reference/field-attributes.md) · [Enum & variant attributes](@/reference/enum-attributes.md) · [Extension attributes](@/reference/extension-attributes.md)
