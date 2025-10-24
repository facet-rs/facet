# Deriving `Facet`

The `Facet` derive macro provides runtime (and to some extent, const-time) knowledge about types, also known as ["reflection"](https://en.wikipedia.org/wiki/Reflective_programming).

This macro uses [unsynn](https://docs.rs/unsynn) instead of [syn](https://docs.rs/syn), which makes it lighter but means it does not implement a complete Rust grammar. Some complex syntax may fail to parse. If you encounter a parsing issue, please [open an issue](https://github.com/facet-rs/facet/issues).

```rust
use facet::Facet;

#[derive(Facet)]
struct FooBar {
    foo: u32,
    bar: String,
}
```

## How renaming works

Unlike serde, where `rename` only affects the serialized form, in facet the renamed value becomes the field's _effective name_ — the name you see when enumerating fields via reflection.

The original Rust identifier is still preserved and accessible, but the effective name is what most operations use by default.

## Serialization and deserialization

The `facet` crate itself does not perform serialization or deserialization — it only provides reflection capabilities. The `#[facet(...)]` attributes are stored as metadata on fields, variants, and containers, which other crates can then read and act upon.

Serialization and deserialization are handled by separate crates:

- [`facet-serialize`](https://docs.rs/facet-serialize) — serialization support
- [`facet-deserialize`](https://docs.rs/facet-deserialize) — deserialization support
- [`facet-json`](https://docs.rs/facet-json) — JSON format implementation (as an example)

These crates read the [field attributes](#field-attributes), [variant attributes](#variant-attributes), and [container attributes](#container-attributes) defined below to control their behavior.

## Container Attributes

Container attributes apply to the struct or enum as a whole.

```rust
use facet::Facet;

#[derive(Facet)]
#[facet(rename_all = "kebab-case")]
struct FooBar {
    // fields...
}
```

### Naming

| Attribute | Description |
|-----------|-------------|
| `rename_all = ".."` | Rename all the fields (if this is a struct) or variants (if this is an enum) according to the given case convention. The possible values are: `"snake_case"`, `"SCREAMING_SNAKE_CASE"`, `"PascalCase"`, `"camelCase"`, `"kebab-case"`, `"SCREAMING-KEBAB-CASE"`. |
| `type_tag = ".."` | Identify type by tag and serialize with this tag, e.g., `#[facet(type_tag = "com.example.MyType")]`. |

### Type representation

| Attribute | Description |
|-----------|-------------|
| `transparent` | Serialize and deserialize a newtype struct exactly the same as if its single field were serialized and deserialized by itself. |
| `opaque` | The inner field does not have to implement `Facet`. |

### Serialization

| Attribute | Description |
|-----------|-------------|
| `skip_serializing` | Don't allow this type to be serialized. |
| `skip_serializing_if = ".."` | Don't allow this type to be serialized if the function returns `true`. |
| `serialize_with = ..` | Support serialization using the specified function. Takes the form `fn(&input_type) -> output_shape` where `input_type` can be any type, including opaque types. |

### Deserialization

| Attribute | Description |
|-----------|-------------|
| `deny_unknown_fields` | Always throw an error when encountering unknown fields during deserialization. When this attribute is not present, unknown fields are ignored. |
| `deserialize_with = ..` | Support deserialization using the specified function. Takes the form `fn(&input_shape) -> output_type` where `output_type` can be any type, including opaque types. |

### Validation

| Attribute | Description |
|-----------|-------------|
| `invariants = ".."` | Called when doing `Partial::build`. **TODO** |

## Field Attributes

Field attributes apply to individual fields within a struct or enum variant.

```rust
use facet::Facet;

#[derive(Facet)]
struct FooBar {
    #[facet(default)]
    foo: u32,
}
```

### Naming

| Attribute | Description |
|-----------|-------------|
| `rename = ".."` | Rename this field to the given name. |

### Structure

| Attribute | Description |
|-----------|-------------|
| `flatten` | Flatten the field's content into the container structure. |
| `child` | Mark as child node in a hierarchy. **TODO** |

### Serialization

| Attribute | Description |
|-----------|-------------|
| `skip_serializing` | Ignore when serializing. |
| `skip_serializing_if = ".."` | Ignore when serializing if the function returns `true`. |
| `serialize_with = ..` | Support serialization of this field using the specified function. |

### Deserialization

| Attribute | Description |
|-----------|-------------|
| `default` | Use the field's value from the container's `Default::default()` implementation when the field is missing during deserializing. |
| `default = ".."` | Use the expression when the field is missing during deserializing. |
| `deserialize_with = ..` | Support deserialization of this field using the specified function. |

### Debug

| Attribute | Description |
|-----------|-------------|
| `sensitive` | Don't show the value in debug outputs. |

## Variant Attributes

Variant attributes apply to individual variants within an enum.

```rust
use facet::Facet;

#[derive(Facet)]
#[repr(C)]
enum FooBar {
    #[facet(rename = "kebab-case")]
    Foo(u32),
}
```

### Naming

| Attribute | Description |
|-----------|-------------|
| `rename = ".."` | Rename this variant to the given name. |

### Serialization

| Attribute | Description |
|-----------|-------------|
| `skip_serializing` | Ignore when serializing. |
| `skip_serializing_if = ".."` | Ignore when serializing if the function returns `true`. |
| `serialize_with = ..` | Support serialization of this variant using the specified function. |

### Deserialization

| Attribute | Description |
|-----------|-------------|
| `deserialize_with = ..` | Support deserialization of this variant using the specified function. |

## Examples

**TODO**.
