+++
title = "Comparison with serde"
weight = 4
insert_anchor_links = "heading"
+++

A side-by-side comparison of facet and serde derive macro attributes.

## Container attributes

### deny_unknown_fields

Produce an error when an unknown field is encountered during deserialization. The default behaviour
is to ignore field that are not known.

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
    field2: Option<i32>, // Option<T> implicitly defaults to None
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

Only allowed for `struct`s, not for `enum`s. During deserialization, any fields that are missing
from the input will be taken from the `Default::default` implementation of the struct. This is not
possible for `enum`s because they can only have a single `Default` implementation producing a single
variant.

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
    field2: Option<i32>, // Option<T> implicitly defaults to None
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

Rename all fields at once using a casing convention. Supported values are

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

Skip this field during serialization.

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

Skip serializing this field when a condition is met. Typically used for `Option` fields when you
want to omit the field entirely from serialized output when the value is `None`.

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
    field2: Option<i32>, // Option<T> implicitly defaults to None
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

#### skip_unless_truthy

Facet provides a more ergonomic alternative: `skip_unless_truthy`. This uses the type's built-in
notion of "truthiness" to decide whether to skip. No predicate function needed.

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
    name: String,        // Omitted if empty
    #[facet(skip_unless_truthy)]
    count: u32,          // Omitted if zero
    #[facet(skip_unless_truthy)]
    tags: Vec<String>,   // Omitted if empty
    #[facet(skip_unless_truthy)]
    email: Option<String>, // Omitted if None
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

**Truthiness by type:**
- **Booleans**: `true` is truthy, `false` is falsy
- **Numbers**: non-zero is truthy (for floats, also excludes NaN)
- **Collections** (`Vec`, `String`, slices, etc.): non-empty is truthy
- **Option**: `Some(_)` is truthy, `None` is falsy

#### skip_all_unless_truthy (container attribute)

For structs where most fields should be skipped when falsy, use the container-level
`skip_all_unless_truthy` attribute instead of marking each field individually.

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
// All fields omitted when falsy!
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

Use a specified function to provide a default value when deserializing if the field is missing from
input. You can either use `default` alone to use `Default::default()` for the field, or provide an
expression producing the default value.

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

#### Implicit defaults (facet-only)

Facet automatically provides default values for certain types without requiring `#[facet(default)]`:

- **`Option<T>`** defaults to `None`
- **`Vec<T>`**, **`HashMap<K, V>`**, **`HashSet<T>`**, and other collection types default to empty

This means you don't need to annotate these fields at all — they just work.

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
    email: Option<String>,   // No attribute needed!
    tags: Vec<String>,       // No attribute needed!
    metadata: HashMap<String, String>, // No attribute needed!
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

## Deriving Default

Facet's plugin system lets you derive `Default` with custom field values using `#[facet(derive(Default))]`.
This requires the `facet-default` crate.

<table>
<tr>
<th>Facet</th>
<th>Serde (std)</th>
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
    debug: bool, // Uses Default::default()
}

let config = Config::default();
// Config { host: "localhost", port: 8080, debug: false }
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
