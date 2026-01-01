# facet-svg

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-svg/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-svg.svg)](https://crates.io/crates/facet-svg)
[![documentation](https://docs.rs/facet-svg/badge.svg)](https://docs.rs/facet-svg)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-svg.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Provides strongly-typed SVG parsing for Facet types using facet-format-xml.

## Why facet-svg?

SVG is ubiquitous in diagram generation—tools like **pikchr**, **Graphviz**, **Mermaid**, and countless design applications output SVG as their primary format. However, SVG parsing in Rust typically requires either:

- **Generic XML libraries** that lose type information and require error-prone casting
- **Special-purpose SVG libraries** that add heavyweight dependencies and force you into their abstractions
- **Manual string parsing** that's fragile and doesn't scale

facet-svg solves this by providing **strongly-typed, compile-time-safe SVG structures** derived from Facet's reflection system. You get:

- **Type Safety**: The Rust compiler catches mismatches between your SVG structure and actual data
- **Zero Dependencies**: Built on facet-format-xml, which uses only quick-xml for parsing
- **Graceful Degradation**: Unknown elements are safely ignored, so SVGs from any tool work without modification
- **Structured Access**: Navigate SVG geometry programmatically with full IDE support and type checking

This makes facet-svg ideal for:
- Processing SVG output from diagram tools in build pipelines
- Extracting geometric data (paths, shapes, text) for analysis or transformation
- Building Rust applications that need to consume or validate SVGs at compile time

## Difference from facet-svg

This crate uses `facet-format-xml` instead of `facet-xml`. The `facet-format-*` crates are the
next-generation format infrastructure for Facet, featuring a unified parser/serializer architecture.

## Supported Elements

The following SVG elements are fully supported for parsing and type-safe access:

- **Shapes**: `<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<path>`, `<polygon>`, `<polyline>`
- **Text**: `<text>` with styling attributes
- **Grouping**: `<g>` with transform support, `<defs>`, `<style>`, `<symbol>`
- **References**: `<use>` for referencing defined elements
- **Media**: `<image>` for embedded or linked images
- **Metadata**: `<title>`, `<desc>` for accessibility and documentation

Unsupported elements (such as `<filter>`, `<marker>`, `<tspan>`, and specialized SVG filters) are gracefully ignored during parsing, allowing real-world SVG files to be processed without errors.

## Basic Usage

```rust
use facet_svg::Svg;

let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
    <rect x="10" y="10" width="80" height="80" fill="blue"/>
</svg>"#;

let svg: Svg = facet_svg::from_str(svg_str)?;
assert_eq!(svg.width, Some("100".to_string()));
```

## Features

- **Type-safe parsing**: Strongly-typed SVG structures via Facet's derive macro
- **Attribute support**: Full attribute parsing for colors, dimensions, transforms, and styling
- **Namespace handling**: Proper SVG namespace support
- **Graceful degradation**: Unknown elements are safely ignored
- **Path parsing**: Complex SVG path data with multiple commands

## Use Cases

- Parsing SVG output from diagram tools (like pikchr, Graphviz)
- Extracting geometric data from SVG files
- Type-safe SVG manipulation in Rust applications

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
