+++
title = "Validation"
description = "Attach validation constraints to Facet fields for deserializers and builders to enforce."
weight = 1
insert_anchor_links = "heading"
+++

`facet-validate` defines validation attributes for `Facet` fields: numeric
bounds, length checks, string checks, regexes, and custom validators. Reach for
it when a value is only valid inside a smaller domain than its Rust type can
express on its own.

## Install

Add `facet` and `facet-validate` to your crate. Format crates and builders that
use facet's partial-construction path can then enforce the validators while
constructing the value.

## Minimal example

```rust
use facet::Facet;
use facet_validate as validate;

#[derive(Facet)]
struct Product {
    #[facet(validate::min_length = 1, validate::max_length = 100)]
    title: String,

    #[facet(validate::min = 0)]
    price_cents: i64,

    #[facet(validate::email)]
    contact_email: String,
}
```

The attributes are stored in the type's facet shape, so validation-aware
deserializers can reject bad input at the point where the offending field is
read.

## Built-in constraints

| Attribute | Checks |
|-----------|--------|
| `validate::min` / `validate::max` | Numeric bounds |
| `validate::min_length` / `validate::max_length` | String or collection length |
| `validate::email` | Email-shaped string |
| `validate::url` | URL-shaped string |
| `validate::regex = r"..."` | Pattern match |
| `validate::contains = "..."` | Required substring |
| `validate::custom = fn_name` | Your own validator |

## Custom validators

A custom validator takes a reference to the field value and returns
`Result<(), String>`:

```rust
use facet::Facet;
use facet_validate as validate;

fn validate_currency(s: &str) -> Result<(), String> {
    match s {
        "USD" | "EUR" | "GBP" => Ok(()),
        _ => Err(format!("invalid currency code: {s}")),
    }
}

#[derive(Facet)]
struct Price {
    #[facet(validate::custom = validate_currency)]
    currency: String,
}
```

The validator message becomes the useful part of the construction error, so make
it short and specific.

## Related

- [facet-json](/facet-json/guide/) — deserialize external data into facet-shaped values
- [facet-default](/facet-default/guide/) — combine defaults with validation constraints
- [facet-error](/facet-error/guide/) — model validation failures as typed errors when needed
- [Ecosystem](/ecosystem/) — other facet derive plugins and format crates
