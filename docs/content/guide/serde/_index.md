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
    field2: Vec<String>,
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
    field2: Vec<String>,
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
