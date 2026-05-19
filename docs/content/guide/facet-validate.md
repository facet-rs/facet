+++
title = "Validation (facet-validate)"
weight = 10
insert_anchor_links = "heading"
+++

[`facet-validate`](https://docs.rs/facet-validate) adds constraint attributes
that run **during deserialization**. Bad input is rejected at the parse site,
and the error points at the exact location in the source.

## Setup

```bash
cargo add facet facet-validate
cargo add facet-json --features validate
```

The validator hooks into the format crate, so enable the format's `validate`
feature (shown above for `facet-json`).

```rust,noexec
use facet::Facet;
use facet_validate as validate;

#[derive(Facet)]
struct Product {
    #[facet(validate::min_length = 1, validate::max_length = 100)]
    title: String,

    #[facet(validate::min = 0)]
    price: i64,

    #[facet(validate::email)]
    contact_email: String,
}
```

Parsing `{"title":"","price":-5,"contact_email":"nope"}` fails with an error
that names the offending field and its span in the JSON — not a generic "invalid
input".

## Built-in constraints

| Attribute | Checks |
|-----------|--------|
| `validate::min` / `validate::max` | Numeric bounds |
| `validate::min_length` / `validate::max_length` | String / collection length |
| `validate::email` | Valid email address |
| `validate::url` | Valid HTTP(S) URL |
| `validate::regex = r"..."` | Pattern match |
| `validate::contains = "..."` | Substring present |
| `validate::custom = fn_name` | Your own validator |

## Custom validators

A custom validator takes a reference to the field value and returns
`Result<(), String>`:

```rust,noexec
fn validate_positive(n: &i64) -> Result<(), String> {
    if *n > 0 { Ok(()) } else { Err(format!("must be positive, got {n}")) }
}

#[derive(Facet)]
struct Order {
    #[facet(validate::custom = validate_positive)]
    quantity: i64,
}
```

Validating at parse time means invalid values never construct a value of your
type — the type system and the validator together keep bad data out.

## Related

- [JSON](@/guide/json.md) — the parse path validation plugs into
- [Ecosystem](@/ecosystem/_index.md) — other derive plugins
