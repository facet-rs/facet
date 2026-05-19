+++
title = "JSON"
weight = 2
insert_anchor_links = "heading"
+++

`facet-json` is the flagship format crate: derive `Facet`, then go to and from
JSON with helpful, span-aware errors. It needs no schema declaration beyond the
derive itself.

## Setup

```bash
cargo add facet facet-json
```

```rust,noexec
use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
struct Person {
    name: String,
    age: u32,
}
```

## Deserialize

```rust,noexec
let json = r#"{"name": "Alice", "age": 30}"#;
let person: Person = facet_json::from_str(json)?;
assert_eq!(person, Person { name: "Alice".into(), age: 30 });
```

`from_slice` does the same from `&[u8]`. For zero-copy borrowing (fields like
`&str` or `Cow<str>` that point into the input buffer), use the borrowed
variants:

```rust,noexec
let person: Person = facet_json::from_str_borrowed(json)?;
// also: from_slice_borrowed
```

## Serialize

```rust,noexec
let person = Person { name: "Alice".into(), age: 30 };

let compact = facet_json::to_string(&person)?;        // {"name":"Alice","age":30}
let pretty  = facet_json::to_string_pretty(&person)?; // multi-line, indented
```

`to_vec` / `to_vec_pretty` produce `Vec<u8>`, and `to_writer_std` /
`to_writer_std_pretty` stream straight into any `std::io::Write`.

## Errors point at the input

facet-json deserialization errors carry the location in the source, so a typo
or type mismatch tells you *where*:

```text
expected u32, found string
  at line 1, column 26
```

No `unwrap()` needed in real code — propagate the error with `?` and print it.

## Controlling output

`to_string_with_options` / `to_vec_with_options` take `SerializeOptions`, which
controls indentation and how byte slices are rendered (raw array vs. hex). See
the [docs.rs reference](https://docs.rs/facet-json) for the full surface.

## Attributes

JSON honors the common facet attributes — `rename`, `rename_all`, `skip`,
`default`, `transparent`, `flatten`, enum tagging — plus `opaque` / `proxy` for
types that don't implement `Facet`. The complete catalog with per-format support
is in the [Attributes reference](@/reference/_index.md) and the
[Format matrix](@/reference/format-crate-matrix/_index.md).

## Related

- [Type Support](@/guide/type-support.md) — using `Uuid`, `DateTime`, paths, etc.
- [facet-validate](@/guide/facet-validate.md) — reject bad data *during* parsing
- [Schema codegen](@/guide/schema-codegen.md) — generate TS/Zod/JSON Schema from the same type
- [Ecosystem](@/ecosystem/_index.md) — every other format crate
