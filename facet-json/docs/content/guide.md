+++
title = "JSON"
description = "Serialize and deserialize Facet values as JSON, with span-aware errors."
weight = 1
insert_anchor_links = "heading"
+++

`facet-json` is the main JSON format crate for facet: derive `Facet`, then
serialize and deserialize without maintaining a second schema. Reach for it when
JSON is your wire format, config format, fixture format, or oracle in tests.

## Install

Add `facet` and `facet-json` to your crate.

## Minimal example

```rust
use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
struct Person {
    name: String,
    age: u32,
}

let person = Person {
    name: "Alice".into(),
    age: 30,
};

let json = facet_json::to_string(&person).unwrap();
assert_eq!(json, r#"{"name":"Alice","age":30}"#);

let roundtrip: Person = facet_json::from_str(&json).unwrap();
assert_eq!(roundtrip, person);
```

## Deserialize

Use `from_str` for `&str` input and `from_slice` for bytes. When the output type
borrows from the JSON buffer, use the borrowed variants:

```rust
use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
struct BorrowedPerson<'a> {
    name: &'a str,
}

let json = r#"{"name":"Alice"}"#;
let person: BorrowedPerson<'_> = facet_json::from_str_borrowed(json).unwrap();
assert_eq!(person.name, "Alice");
```

`from_slice_borrowed` does the same for `&[u8]`.

## Serialize

The compact helpers return a `String` or `Vec<u8>`. The pretty helpers use
multi-line indentation, and the writer helpers stream directly into
`std::io::Write`:

```rust
use facet::Facet;

#[derive(Facet)]
struct Point {
    x: i32,
    y: i32,
}

let point = Point { x: 1, y: 2 };

let compact = facet_json::to_string(&point).unwrap();
let pretty = facet_json::to_string_pretty(&point).unwrap();

assert_eq!(compact, r#"{"x":1,"y":2}"#);
assert!(pretty.contains('\n'));
```

For lower-level control, `to_string_with_options`, `to_vec_with_options`, and
`to_writer_std_with_options` take `SerializeOptions`, which controls indentation
and byte rendering.

## Attributes

`facet-json` honors the common facet attributes such as `rename`, `rename_all`,
`skip`, `default`, `transparent`, `flatten`, and enum tagging. It also supports
`opaque` and `proxy` for types that need a format-specific representation. The
complete catalog lives in the [attributes reference](/reference/) and
[format matrix](/reference/format-crate-matrix/).

## Errors

Deserialization errors carry source locations, so malformed JSON and type
mismatches can point back to the input that caused them. In application code,
propagate the error with `?` and let your diagnostic layer decide how much
context to show.

## Related

- [facet-validate](/facet-validate/guide/) — attach constraints that deserializers can enforce
- [facet-pretty](/facet-pretty/guide/) — inspect parsed values without deriving `Debug`
- [figue](/figue/guide/) — build CLI and config structs from the same kind of shape
- [Ecosystem](/ecosystem/) — every other facet crate
