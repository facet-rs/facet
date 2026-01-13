+++
title = "XML and HTML"
weight = 7
insert_anchor_links = "heading"
+++

Guide to using `facet-xml` and `facet-html` for parsing and serializing markup documents.

## Overview

Both crates use a DOM-based approach with extension attributes to map Rust structs to markup elements. The key difference is in their data models:

| Aspect | XML (`facet-xml`) | HTML (`facet-html`) |
|--------|-------------------|---------------------|
| **Data model** | Data-centric | Structure-centric |
| **Text elements** | Treated as scalar values | Always child nodes |
| **Use case** | Config files, APIs, data exchange | Web documents, scraping |
| **Import as** | `use facet_xml as xml;` | `use facet_html as html;` |

## Extension Attributes

After importing the crate with an alias, you can use namespaced attributes:

```rust
use facet::Facet;
use facet_xml as xml;

#[derive(Facet)]
struct Person {
    #[facet(xml::attribute)]
    id: u64,
    
    #[facet(xml::element)]
    name: String,
    
    #[facet(xml::text)]
    bio: String,
}
```

### Field Attributes

#### `xml::attribute` / `html::attribute`

Marks a field as an XML/HTML attribute on the element tag.

```rust
#[derive(Facet)]
struct Link {
    #[facet(html::attribute)]
    href: String,
    
    #[facet(html::attribute)]
    target: Option<String>,
    
    #[facet(html::text)]
    text: String,
}
```

Parses: `<a href="/home" target="_blank">Home</a>`

#### `xml::element` / `html::element`

Marks a field as a single child element.

```rust
#[derive(Facet)]
struct Document {
    #[facet(xml::element)]
    header: Header,
    
    #[facet(xml::element)]
    body: Body,
}
```

Parses:
```xml
<Document>
    <header>...</header>
    <body>...</body>
</Document>
```

#### `xml::elements` / `html::elements`

Marks a field as collecting multiple child elements of the same type.

```rust
#[derive(Facet)]
struct Playlist {
    #[facet(xml::attribute)]
    name: String,
    
    #[facet(xml::elements)]
    tracks: Vec<Track>,
}

#[derive(Facet)]
struct Track {
    #[facet(xml::attribute)]
    title: String,
    
    #[facet(xml::attribute)]
    duration: u32,
}
```

Parses:
```xml
<Playlist name="Favorites">
    <Track title="Song A" duration="180"/>
    <Track title="Song B" duration="240"/>
</Playlist>
```

**Note:** Elements are "flat" — child elements appear directly without a wrapper. This matches serde-xml-rs behavior.

#### `xml::text` / `html::text`

Marks a field as the text content of the element.

```rust
#[derive(Facet)]
struct Paragraph {
    #[facet(html::attribute)]
    class: Option<String>,
    
    #[facet(html::text)]
    content: String,
}
```

Parses: `<p class="intro">Hello, world!</p>`

#### `xml::element_name` / `html::element_name`

Captures the actual element tag name into a field. Useful for dynamic element handling.

```rust
#[derive(Facet)]
struct AnyElement {
    #[facet(html::element_name)]
    tag: String,
    
    #[facet(html::text)]
    content: String,
}
```

### Container Attributes

#### `xml::ns` / `html::ns`

Specifies the XML namespace URI for a specific field.

```rust
#[derive(Facet)]
#[facet(rename = "root")]
struct Document {
    #[facet(xml::element, xml::ns = "http://example.com/ns")]
    data: String,
}
```

When deserializing, the field only matches elements in the specified namespace. When serializing, the element is emitted with the appropriate namespace prefix.

#### `xml::ns_all` / `html::ns_all`

Sets the default namespace for all fields in a container.

```rust
#[derive(Facet)]
#[facet(rename = "svg", xml::ns_all = "http://www.w3.org/2000/svg")]
struct Svg {
    #[facet(xml::attribute, rename = "viewBox")]
    view_box: Option<String>,
    
    #[facet(xml::attribute)]
    width: Option<String>,
    
    #[facet(xml::attribute)]
    height: Option<String>,
    
    #[facet(xml::elements)]
    shapes: Vec<Shape>,
}
```

Individual fields can override this with their own `xml::ns` attribute.

## XML vs HTML Data Models

### XML: Data-Centric

In XML, elements containing only text are treated as scalar values:

```rust
use facet::Facet;

#[derive(Facet)]
struct Person {
    name: String,  // No attribute needed - elements are the default
    age: u32,
}

// Parses: <Person><name>Alice</name><age>30</age></Person>
```

This enables natural mappings where `<age>25</age>` deserializes directly to `age: u32`.

### HTML: Structure-Centric

In HTML, every element is a structural node with a tag name, attributes, and children. Text is always a child node, never the element itself:

```rust
use facet::Facet;
use facet_html as html;

#[derive(Facet)]
struct Heading {
    #[facet(html::attribute)]
    id: Option<String>,
    
    #[facet(html::text)]
    text: String,
}

// Parses: <h1 id="title">Welcome</h1>
```

## Working with Enums

Use enums to handle multiple possible child element types:

```rust
use facet::Facet;
use facet_xml as xml;

#[derive(Facet)]
#[repr(u8)]
enum Shape {
    circle(Circle),
    rect(Rect),
    line(Line),
}

#[derive(Facet)]
struct Circle {
    #[facet(xml::attribute)]
    cx: f64,
    #[facet(xml::attribute)]
    cy: f64,
    #[facet(xml::attribute)]
    r: f64,
}

#[derive(Facet)]
struct Drawing {
    #[facet(xml::attribute)]
    name: String,
    
    #[facet(xml::elements)]
    shapes: Vec<Shape>,
}
```

Parses:
```xml
<Drawing name="test">
    <circle cx="10" cy="20" r="5"/>
    <rect x="0" y="0" width="50" height="30"/>
</Drawing>
```

## Capturing Unknown Content

Use `#[facet(flatten)]` with a `HashMap` to capture unknown attributes or elements:

```rust
use facet::Facet;
use facet_html as html;
use std::collections::HashMap;

#[derive(Facet)]
struct DivWithExtras {
    #[facet(html::attribute)]
    id: Option<String>,
    
    #[facet(html::attribute)]
    class: Option<String>,
    
    /// Captures data-*, aria-*, and other unknown attributes
    #[facet(flatten, default)]
    extra: HashMap<String, String>,
    
    #[facet(html::text)]
    content: String,
}
```

Parses: `<div id="widget" data-user-id="123" aria-label="Card">Content</div>`

The `extra` field will contain `{"data-user-id": "123", "aria-label": "Card"}`.

## Basic Usage

### Deserializing

```rust
use facet::Facet;

#[derive(Facet, Debug)]
struct Config {
    name: String,
    port: u16,
}

let xml = r#"<Config><name>server</name><port>8080</port></Config>"#;
let config: Config = facet_xml::from_str(xml).unwrap();
println!("{:?}", config);
```

### Serializing

```rust
use facet::Facet;

#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
}

let config = Config {
    name: "server".into(),
    port: 8080,
};

// Compact output
let xml = facet_xml::to_string(&config).unwrap();
// <Config><name>server</name><port>8080</port></Config>

// Pretty-printed
let xml_pretty = facet_xml::to_string_pretty(&config).unwrap();
```

## Combining with Standard Attributes

Extension attributes work alongside standard facet attributes:

```rust
use facet::Facet;
use facet_xml as xml;

#[derive(Facet)]
#[facet(rename = "item", deny_unknown_fields)]
struct Item {
    #[facet(xml::attribute, rename = "itemId")]
    id: u64,
    
    #[facet(xml::element, default)]
    description: String,
    
    #[facet(skip_serializing_if = Option::is_none)]
    metadata: Option<String>,
}
```

## Migration from 0.42.0 to 0.43.0

Version 0.43.0 changed the list serialization format from **wrapped** to **flat** (matching serde-xml-rs behavior).

### What Changed

**Before (0.42.0):** Lists were wrapped in an element named after the field:

```xml
<Playlist name="Favorites">
    <tracks>
        <Track title="Song A"/>
        <Track title="Song B"/>
    </tracks>
</Playlist>
```

**After (0.43.0):** List items appear directly as children (flat):

```xml
<Playlist name="Favorites">
    <Track title="Song A"/>
    <Track title="Song B"/>
</Playlist>
```

### Updating Your Code

If you have existing XML data with wrapped lists, you have two options:

#### Option 1: Update your XML data

Remove the wrapper elements from your XML files to match the new flat format.

#### Option 2: Add an explicit wrapper struct

If you need to preserve the wrapped format, create an intermediate struct:

```rust
// Before (0.42.0 - implicit wrapper)
#[derive(Facet)]
struct Playlist {
    #[facet(xml::elements)]
    tracks: Vec<Track>,
}

// After (0.43.0 - explicit wrapper for same XML structure)
#[derive(Facet)]
struct Playlist {
    #[facet(xml::element)]
    tracks: TrackList,
}

#[derive(Facet)]
struct TrackList {
    #[facet(xml::elements, rename = "Track")]
    items: Vec<Track>,
}
```

### Why This Change?

The flat format is more common in real-world XML (RSS, Atom, SVG, SOAP) and matches how serde-xml-rs works, making migration easier.

## See Also

- [HTML Showcase](@/guide/showcases/html.md) — interactive examples of HTML parsing
- [Attributes Reference](@/guide/attributes.md) — complete attribute catalog
- [Extension Attributes](@/extend/extension-attributes.md) — how extension attributes work
