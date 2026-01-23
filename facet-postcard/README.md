# facet-postcard

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-postcard/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-postcard.svg)](https://crates.io/crates/facet-postcard)
[![documentation](https://docs.rs/facet-postcard/badge.svg)](https://docs.rs/facet-postcard)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-postcard.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-postcard

Postcard binary format for facet with Tier-0 and Tier-2 JIT deserialization support.

## Wire Compatibility

For **statically-typed structs, enums, and primitives**, facet-postcard produces
wire-compatible output with the standard `postcard` crate. You can serialize with
facet-postcard and deserialize with serde's postcard (and vice versa), as long as
both sides agree on the schema.

## Dynamic Values (`facet_value::Value`)

> **Warning**: `Value` serialization uses a **facet-specific tagged encoding** that
> is **NOT compatible** with standard postcard.

Since postcard is not a self-describing format, there's no standard way to serialize
dynamic/any values. facet-postcard solves this by prefixing each `Value` with a type
tag byte:

| Tag | Type     | Encoding                                    |
|-----|----------|---------------------------------------------|
| 0   | Null     | (no payload)                                |
| 1   | Bool     | 1 byte (0 or 1)                             |
| 2   | I64      | zigzag varint                               |
| 3   | U64      | varint                                      |
| 4   | F64      | 8 bytes little-endian                       |
| 5   | String   | varint length + UTF-8 bytes                 |
| 6   | Bytes    | varint length + raw bytes                   |
| 7   | Array    | varint count + tagged elements recursively  |
| 8   | Object   | varint count + (string key, tagged value) pairs |
| 9   | DateTime | string (RFC3339)                            |

**This means:**

- You **cannot** deserialize facet-postcard `Value` bytes using serde's postcard
- You **cannot** serialize with serde's postcard and deserialize as `Value` with facet-postcard
- Both sides of an RPC/serialization boundary must use facet-postcard when `Value` is involved

**Example wire format** for `{"name": "Alice", "age": 30}`:

```
08                      # tag 8 = Object
02                      # 2 entries
04 6e 61 6d 65          # key: string "name" (len=4)
05                      # tag 5 = String
05 41 6c 69 63 65       # value: string "Alice" (len=5)
03 61 67 65             # key: string "age" (len=3)
03                      # tag 3 = U64
1e                      # value: varint 30
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
