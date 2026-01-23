# facet-validate

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-validate/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-validate.svg)](https://crates.io/crates/facet-validate)
[![documentation](https://docs.rs/facet-validate/badge.svg)](https://docs.rs/facet-validate)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-validate.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-validate

Field validation during deserialization for the facet ecosystem.

## Features

- Validation runs **during deserialization**, so errors include source spans (e.g., pointing to the exact JSON location)
- Custom validators with meaningful error messages via `Result<(), String>`
- Auto-deref: `fn(&str)` validators work for `String` fields

## Usage

```rust
use facet::Facet;
use facet_validate as validate;

fn validate_positive(n: &i64) -> Result<(), String> {
    if *n <= 0 {
        Err(format!("must be positive, got {}", n))
    } else {
        Ok(())
    }
}

#[derive(Facet)]
struct Product {
    #[facet(validate::custom = validate_positive)]
    price: i64,
}
```

## Built-in Validators

| Validator | Syntax | Applies To |
|-----------|--------|------------|
| `min` | `validate::min = 0` | numbers |
| `max` | `validate::max = 100` | numbers |
| `min_length` | `validate::min_length = 1` | String, Vec, slices |
| `max_length` | `validate::max_length = 100` | String, Vec, slices |
| `email` | `validate::email` | String |
| `url` | `validate::url` | String |
| `regex` | `validate::regex = r"..."` | String |
| `contains` | `validate::contains = "foo"` | String |
| `custom` | `validate::custom = fn_name` | any |

## Integration

Enable the `validate` feature on `facet-json` (or other format crates):

```toml
[dependencies]
facet-json = { version = "0.41", features = ["validate"] }
facet-validate = "0.41"
```

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
