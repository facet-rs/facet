+++
title = "Comparison with serde"
weight = 4
insert_anchor_links = "heading"
+++

A side-by-side comparison of facet and serde derive macro attributes.

## Container attributes

### deny_unknown_fields

Rejects unknown fields during deserialization. By default, unknown fields are silently ignored.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
#[facet(deny_unknown_fields)]
struct MyStruct {
    field1: i32,
    field2: Option<i32>,
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MyStruct {
    field1: i32,
    field2: Option<i32>,
}
```

</td>
</tr>
</table>

### default

Applies to structs only. Missing fields are filled from the type's `Default` implementation.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
#[facet(default)]
struct MyStruct {
    field1: i32,
    field2: Option<i32>,
}

impl Default for MyStruct {
    fn default() -> Self {
        Self {
            field1: 1,
            field2: Some(2),
        }
    }
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Deserialize)]
#[serde(default)]
struct MyStruct {
    field1: i32,
    field2: Option<i32>,
}

impl Default for MyStruct {
    fn default() -> Self {
        Self {
            field1: 1,
            field2: Some(2),
        }
    }
}
```

</td>
</tr>
</table>

### rename_all

Renames all fields using a casing convention. Supported values:

* `"PascalCase"`
* `"camelCase"`
* `"snake_case"`
* `"SCREAMING_SNAKE_CASE"`
* `"kebab-case"`
* `"SCREAMING-KEBAB-CASE"`

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
#[facet(rename_all = "camelCase")]
struct MyStruct {
    field_one: i32,
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MyStruct {
    field_one: i32,
}
```

</td>
</tr>
</table>

## Field attributes

### skip_serializing

Excludes this field from serialization.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
struct MyStruct {
    field1: i32,
    #[facet(skip_serializing)]
    field2: String,
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Serialize)]
struct MyStruct {
    field1: i32,
    #[serde(skip_serializing)]
    field2: String,
}
```

</td>
</tr>
</table>


### skip_serializing_if

Conditionally excludes a field from serialization based on a predicate.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
struct MyStruct {
    #[facet(skip_serializing_if = |n| n % 2 == 0)]
    field1: i32,
    #[facet(skip_serializing_if = Option::is_none)]
    field2: Option<i32>,
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Serialize)]
struct MyStruct {
    #[serde(skip_serializing_if = is_even)]
    field1: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    field2: Option<i32>,
}

fn is_even(n: i32) -> bool {
    n % 2 == 0
}
```

</td>
</tr>
</table>

Facet accepts closures directly; serde requires a named function passed as a string.

#### skip_unless_truthy

Facet also provides `skip_unless_truthy`, which uses built-in truthiness predicates instead of requiring a custom function.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
struct MyStruct {
    #[facet(skip_unless_truthy)]
    name: String,
    #[facet(skip_unless_truthy)]
    count: u32,
    #[facet(skip_unless_truthy)]
    tags: Vec<String>,
    #[facet(skip_unless_truthy)]
    email: Option<String>,
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Serialize)]
struct MyStruct {
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(skip_serializing_if = "is_zero")]
    count: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
}

fn is_zero(n: &u32) -> bool { *n == 0 }
```

</td>
</tr>
</table>

Truthiness is defined per type:
- **Booleans**: `false` is falsy
- **Numbers**: zero is falsy (for floats, NaN is also falsy)
- **Collections** (`Vec`, `String`, slices, etc.): empty is falsy
- **Option**: `None` is falsy

#### skip_all_unless_truthy (container attribute)

Applies `skip_unless_truthy` to all fields in the struct.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
#[facet(skip_all_unless_truthy)]
struct Config {
    name: String,
    description: String,
    count: u32,
    enabled: bool,
    tags: Vec<String>,
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Serialize)]
struct Config {
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(skip_serializing_if = "is_zero")]
    count: u32,
    #[serde(skip_serializing_if = "is_false")]
    enabled: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

fn is_zero(n: &u32) -> bool { *n == 0 }
fn is_false(b: &bool) -> bool { !*b }
```

</td>
</tr>
</table>

### default

Provides a default value when deserializing a missing field. Can use `Default::default()` or a custom expression.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
struct MyStruct {
    field1: i32,
    #[facet(default)]
    field2: String,
    #[facet(default = 42)]
    field3: i32,
    #[facet(default = rand::random())]
    field4: i32,
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Deserialize)]
struct MyStruct {
    field1: i32,
    #[serde(default)]
    field2: String,
    #[serde(default = "default_value")]
    field3: i32,
    #[serde(default = "rand::random")]
    field4: i32,
}

fn default_value() -> i32 {
    42
}
```

</td>
</tr>
</table>

Facet accepts expressions directly; serde requires a function path as a string.

#### Implicit defaults

Facet implicitly defaults certain types when the field is absent:

- **`Option<T>`** defaults to `None`
- **`Vec<T>`**, **`HashMap<K, V>`**, **`HashSet<T>`**, and other collection types default to empty

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust,noexec
#[derive(facet::Facet)]
struct MyStruct {
    name: String,
    email: Option<String>,
    tags: Vec<String>,
    metadata: HashMap<String, String>,
}
```

</td>
<td>

```rust,noexec
#[derive(serde::Deserialize)]
struct MyStruct {
    name: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}
```

</td>
</tr>
</table>

This is a trade-off: facet cannot distinguish between "field absent" and "field present but empty" for these types. If that distinction matters, use a wrapper type or explicit handling.

## Deriving Default

The `facet-default` crate provides a plugin for deriving `Default` using field-level `#[facet(default = ...)]` attributes.

<table>
<tr>
<th>Facet</th>
<th>Rust stdlib</th>
</tr>
<tr>
<td>

```rust,noexec
use facet::Facet;
use facet_default as _;

#[derive(Facet, Debug)]
#[facet(derive(Default))]
struct Config {
    #[facet(default = "localhost")]
    host: String,
    #[facet(default = 8080u16)]
    port: u16,
    debug: bool,
}

let config = Config::default();
```

</td>
<td>

```rust,noexec
#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
    debug: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8080,
            debug: false,
        }
    }
}

let config = Config::default();
```

</td>
</tr>
</table>

For enums, mark the default variant with `#[facet(default::variant)]`:

```rust,noexec
#[derive(Facet, Debug)]
#[facet(derive(Default))]
#[repr(u8)]
enum Status {
    #[facet(default::variant)]
    Pending,
    Active,
    Done,
}

let status = Status::default(); // Status::Pending
```
