# facet-postcard

[![codecov](https://codecov.io/gh/facet-rs/facet/graph/badge.svg)](https://codecov.io/gh/facet-rs/facet)
[![crates.io](https://img.shields.io/crates/v/facet-postcard.svg)](https://crates.io/crates/facet-postcard)
[![documentation](https://docs.rs/facet-postcard/badge.svg)](https://docs.rs/facet-postcard)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-postcard.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)


# facet-postcard

Postcard serialization and deserialization for Facet types.

[Postcard](https://github.com/jamesmunns/postcard) is a compact, efficient binary serialization format designed for embedded and `no_std` environments. This crate provides byte-for-byte compatible output with the standard `postcard` crate, while using Facet's reflection capabilities instead of serde.

## Features

- Compact binary format optimized for size
- Byte-for-byte compatibility with the `postcard` crate
- `no_std` support with the `alloc` feature
- Zero-copy deserialization where possible

## Usage

```rust
use facet::Facet;
use facet_postcard::{to_vec, from_slice};

#[derive(Debug, Facet)]
struct Message {
    id: u32,
    payload: Vec<u8>,
}

// Serialize
let msg = Message { id: 42, payload: vec![1, 2, 3] };
let bytes = to_vec(&msg).unwrap();

// Deserialize
let decoded: Message = from_slice(&bytes).unwrap();
```

For `no_std` environments without an allocator, use `to_slice`:

```rust
# use facet::Facet;
# #[derive(Debug, Facet)]
# struct Message { id: u32, payload: Vec<u8> }
# let msg = Message { id: 42, payload: vec![1, 2, 3] };
let mut buf = [0u8; 64];
let used = facet_postcard::to_slice(&msg, &mut buf).unwrap();
```

## Feature Flags

- `std` (default): Enables standard library support
- `alloc`: Enables heap allocation without full std (for `no_std` with allocator)



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
