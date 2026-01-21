# facet-html-diff

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-html-diff/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-html-diff.svg)](https://crates.io/crates/facet-html-diff)
[![documentation](https://docs.rs/facet-html-diff/badge.svg)](https://docs.rs/facet-html-diff)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-html-diff.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-html-diff

Diff two HTML documents and produce a series of DOM patches to morph one into the other in a browser.

## Purpose

Given HTML documents A and B, this crate computes the minimal set of DOM operations needed to transform A into B. The patches can be serialized and sent to a browser, where they can be applied incrementally without replacing the entire document.

This enables efficient live-reloading and hot-updating of web pages.

## Example

```rust
use facet_html_diff::{diff_html, Patch, NodePath};

let old = "<html><body><p>Hello</p></body></html>";
let new = "<html><body><p>Goodbye</p></body></html>";

let patches = diff_html(old, new).unwrap();
// patches contains SetText operations to update "Hello" -> "Goodbye"
```

## Patch Types

- `Replace` - Replace a node with new HTML
- `ReplaceInnerHtml` - Replace all children of a node
- `InsertBefore` / `InsertAfter` - Insert HTML relative to a node
- `AppendChild` - Append HTML as last child
- `Remove` - Remove a node
- `SetText` - Update text content
- `SetAttribute` / `RemoveAttribute` - Modify attributes
- `Move` - Move a node from one location to another

## How It Works

1. Parse both HTML documents using `facet-html`
2. Compute structural diff using `facet-diff` (GumTree/Chawathe algorithm)
3. Translate the edit operations into DOM-specific patches
4. Patches reference nodes by path (e.g., `[0, 2, 1]` = body's child 0, then child 2, then child 1)

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
