# facet-xml-diff

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-xml-diff/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-xml-diff.svg)](https://crates.io/crates/facet-xml-diff)
[![documentation](https://docs.rs/facet-xml-diff/badge.svg)](https://docs.rs/facet-xml-diff)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-xml-diff.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Diff-aware XML serialization—render structural diffs as readable XML.

## Overview

This crate renders diffs between facet values as XML with visual diff markers.
It shows what changed between two values in a format that's easy to read,
with proper alignment, colored output, and collapsing of unchanged regions.

## Example

```rust
use facet::Facet;
use facet_diff::tree_diff;

#[derive(Facet)]
struct Rect {
    fill: String,
    x: i32,
    y: i32,
}

let old = Rect { fill: "red".into(), x: 10, y: 20 };
let new = Rect { fill: "blue".into(), x: 10, y: 20 };

let xml = facet_xml_diff::diff_to_string(&old, &new)?;
```

Output:

```xml
<rect
← fill="red"
→ fill="blue"
  x="10" y="20"
/>
```

## Features

- **Diff markers**: `←`/`→` (or `-`/`+`) prefix lines to show old vs new values
- **Value-only coloring**: Only the changed values are colored, not the whole line
- **Alignment**: Attributes align properly for readability
- **Collapsing**: Long runs of unchanged content are collapsed with `...`
- **ANSI colors**: Optional terminal colors for better visibility

## Options

```rust
use facet_xml_diff::{DiffSerializeOptions, DiffSymbols, DiffTheme};

let options = DiffSerializeOptions {
    symbols: DiffSymbols::ascii(),  // Use -/+ instead of arrows
    theme: DiffTheme::default(),
    colors: true,
    indent: "  ",
    max_line_width: 80,
    collapse_threshold: 3,
    ..Default::default()
};
```

## Use Cases

- Debugging configuration changes
- Displaying diffs in CLI tools
- Generating human-readable change logs
- Testing serialization by comparing expected vs actual output

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
