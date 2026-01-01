# facet-default

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-default/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-default.svg)](https://crates.io/crates/facet-default)
[![documentation](https://docs.rs/facet-default/badge.svg)](https://docs.rs/facet-default)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-default.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-default

Derive [`Default`](https://doc.rust-lang.org/std/default/trait.Default.html) for your types using facet's plugin system with custom field defaults.

## Usage

```rust
use facet::Facet;
use facet_default as default;

#[derive(Facet, Debug)]
#[facet(derive(Default))]
pub struct Config {
    #[facet(default::value = "localhost")]
    host: String,
    #[facet(default::value = 8080u16)]
    port: u16,
    #[facet(default::func = "default_timeout")]
    timeout: std::time::Duration,
    // No attribute = uses Default::default()
    debug: bool,
}

fn default_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(30)
}
```

## Attributes

### Field Level

- `#[facet(default::value = literal)]` - Use a literal value (converted via `.into()`)
- `#[facet(default::func = "path")]` - Call a function to get the default value

Fields without attributes use `Default::default()`.

**Note:** For numeric literals, use type suffixes to ensure correct types (e.g., `8080u16` instead of `8080` for a `u16` field). String literals are automatically converted via `.into()`.

## Enums

For enums, mark the default variant with `#[facet(default::variant)]`:

```rust
use facet::Facet;
use facet_default as default;

#[derive(Facet, Debug, PartialEq)]
#[facet(derive(Default))]
#[repr(u8)]
pub enum Status {
    #[facet(default::variant)]
    Pending,
    Active,
    Done,
}

assert_eq!(Status::default(), Status::Pending);
```

Enum variants with fields also work - fields use their own default attributes:

```rust
use facet::Facet;
use facet_default as default;

#[derive(Facet, Debug)]
#[facet(derive(Default))]
#[repr(u8)]
pub enum Request {
    #[facet(default::variant)]
    Get {
        #[facet(default::value = "/")]
        path: String,
        #[facet(default::value = 80u16)]
        port: u16,
    },
    Post { path: String, body: String },
}
```

## LLM contribution policy

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
