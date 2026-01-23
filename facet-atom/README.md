# facet-atom

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-atom/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-atom.svg)](https://crates.io/crates/facet-atom)
[![documentation](https://docs.rs/facet-atom/badge.svg)](https://docs.rs/facet-atom)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-atom.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Provides strongly-typed Atom Syndication Format (RFC 4287) parsing and generation using facet-xml.

## Why facet-atom?

Atom is the standard XML-based format for web content syndication. While RSS is more widely known, Atom (RFC 4287) offers a cleaner, more precisely specified format that's used by many content platforms, feed readers, and publishing tools.

facet-atom provides **strongly-typed, compile-time-safe Atom structures** derived from Facet's reflection system. You get:

- **Full RFC 4287 Compliance**: All standard elements and constructs are supported
- **Type Safety**: The Rust compiler catches mismatches between your Atom structure and actual data
- **Zero Dependencies**: Built on facet-xml, which uses only quick-xml for parsing
- **Bidirectional**: Both parsing and generation are supported with consistent types

This makes facet-atom ideal for:
- Feed aggregators and readers
- Publishing systems that generate Atom feeds
- Content syndication pipelines
- Feed validation and transformation tools

## Supported Elements

The following Atom elements are fully supported:

### Container Elements
- **`<feed>`**: Top-level feed container with metadata and entries
- **`<entry>`**: Individual content entries
- **`<source>`**: Original feed metadata for aggregated entries

### Metadata Elements
- **`<author>` / `<contributor>`**: Person constructs with name, uri, email
- **`<category>`**: Categorization with term, scheme, label
- **`<generator>`**: Feed generator information
- **`<icon>` / `<logo>`**: Feed imagery
- **`<link>`**: Related resources with full attribute support (href, rel, type, hreflang, title, length)
- **`<id>`**: Permanent, universally unique identifiers

### Content Elements
- **`<title>` / `<subtitle>` / `<summary>` / `<rights>`**: Text constructs supporting text/html/xhtml
- **`<content>`**: Entry content (inline or external via src)
- **`<published>` / `<updated>`**: RFC 3339 timestamps

## Basic Usage

```rust
use facet_atom::{Feed, Entry, Person, Link, TextContent};

// Parse an Atom feed
let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
    <title>Example Feed</title>
    <id>urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6</id>
    <updated>2003-12-13T18:30:02Z</updated>
    <author>
        <name>John Doe</name>
    </author>
    <entry>
        <title>First Post</title>
        <id>urn:uuid:1225c695-cfb8-4ebb-aaaa-80da344efa6a</id>
        <updated>2003-12-13T18:30:02Z</updated>
        <summary>Some text.</summary>
    </entry>
</feed>"#;

let feed: Feed = facet_atom::from_str(xml)?;
assert_eq!(feed.entries.len(), 1);
```

## Features

- **Full RFC 4287 support**: All standard elements and attributes
- **Text construct types**: Plain text, escaped HTML, and inline XHTML
- **Namespace handling**: Proper Atom namespace (`http://www.w3.org/2005/Atom`)
- **Roundtrip support**: Parse and regenerate valid Atom XML
- **Link relations**: Support for alternate, self, enclosure, related, via, and custom relations

## References

- [RFC 4287 - The Atom Syndication Format](https://www.rfc-editor.org/rfc/rfc4287)
- [Atom on Wikipedia](https://en.wikipedia.org/wiki/Atom_(web_standard))

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
