# facet-html-diff

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-html-diff/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-html-diff.svg)](https://crates.io/crates/facet-html-diff)
[![documentation](https://docs.rs/facet-html-diff/badge.svg)](https://docs.rs/facet-html-diff)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-html-diff.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-html-diff

Diff two HTML documents and produce a series of DOM patches to morph one into the other.

## Purpose

Given HTML documents A and B, this crate computes the minimal set of DOM operations needed
to transform A into B. The patches can be serialized and sent to a browser, where they can
be applied incrementally without replacing the entire document.

This enables efficient live-reloading and hot-updating of web pages.

## Example

```rust
use facet_html_diff::{diff_html, apply_patches, parse_html};

let old = "<html><body><p>Hello</p></body></html>";
let new = "<html><body><p>Goodbye</p></body></html>";

let patches = diff_html(old, new).unwrap();

// Apply patches to transform old -> new
let mut doc = parse_html(old).unwrap();
apply_patches(&mut doc, &patches).unwrap();
assert_eq!(doc.to_html(), "<body><p>Goodbye</p></body>");
```

## Chawathe Semantics

This crate implements [Chawathe edit script semantics][chawathe], which differ from
typical array splice operations:

- **Insert and Move do NOT shift siblings** - they *displace* whatever node currently
  occupies the target position into a numbered *slot*
- **Slots hold displaced nodes** for potential later reinsertion via Move
- **Each operation specifies an exact position** - the edit script is position-independent

This model matches tree semantics (parent-child relationships) rather than array indices,
and enables efficient expression of operations like "swap these two nodes".

See [`cinereus/CHAWATHE_SEMANTICS.md`](../cinereus/CHAWATHE_SEMANTICS.md) for the full explanation.

[chawathe]: https://dl.acm.org/doi/10.1145/235968.233366

## How It Works

```text
         facet-html              facet-diff                facet-html-diff
              │                      │                           │
   HTML A ────┼──► typed structs ────┼──► GumTree matching ──────┼──► Patch list
   HTML B ────┘                      │    Chawathe edit script ──┘
                                     │
                              cinereus (tree diff engine)
```

1. Parse both HTML documents using `facet-html` into typed Rust structs
2. Compute structural diff using `cinereus` (GumTree matching + Chawathe edit script)
3. Translate edit operations into DOM patches with slot-based displacement
4. Apply via `apply_patches()` or serialize for browser use with `facet-html-diff-wasm`

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
