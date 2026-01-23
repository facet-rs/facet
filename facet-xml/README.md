# facet-xml

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-xml/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-xml.svg)](https://crates.io/crates/facet-xml)
[![documentation](https://docs.rs/facet-xml/badge.svg)](https://docs.rs/facet-xml)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-xml.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-xml

XML serialization and deserialization for Rust using the facet reflection framework.

The XML serializer and deserializer assumes every node is `lowerCamelCase`, unless explicitly renamed.

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
struct Banana {
    taste: String,
}
# let xml_str = "<banana><taste>sweet</taste></banana>";
# let b: Banana = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(b.taste, "sweet");
```

Use `rename` to override:

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
#[facet(rename = "Banana")]
struct Banana {
    taste: String,
}
# let xml_str = "<Banana><taste>sweet</taste></Banana>";
# let b: Banana = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(b.taste, "sweet");
```

## Child Elements

By default, fields are matched against child elements with the same name (in `lowerCamelCase`).

```xml
<person>
    <name>Ella</name>
    <age>42</age>
</person>
```

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
struct Person {
    name: String, // captures "Ella"
    age: u32,     // captures 42
}
# let xml_str = "<person><name>Ella</name><age>42</age></person>";
# let person: Person = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(person.name, "Ella");
# assert_eq!(person.age, 42);
```

## Attributes

Use `xml::attribute` to capture XML attributes:

```xml
<link href="/home">Home</link>
```

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
struct Link {
    #[facet(xml::attribute)]
    href: String,      // captures "/home"
    
    #[facet(xml::text)]
    text: String,      // captures "Home"
}
# let xml_str = r#"<link href="/home">Home</link>"#;
# let link: Link = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(link.href, "/home");
# assert_eq!(link.text, "Home");
```

## Text

Use `xml::text` to capture text content:

```xml
<name>Ella</name>
```

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
struct Name {
    #[facet(xml::text)]
    value: String, // captures "Ella"
}
# let xml_str = "<name>Ella</name>";
# let name: Name = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(name.value, "Ella");
```

## Lists

For list types (`Vec`, etc.), facet-xml collects items. By default, items are child elements with the **singularized** field name (via `facet-singularize`).

### Default: child elements with singularized name

```xml
<playlist>
    <track>Song A</track>
    <track>Song B</track>
</playlist>
```

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
struct Playlist {
    tracks: Vec<String>, // "tracks" → expects <track> elements
}
# let xml_str = "<playlist><track>Song A</track><track>Song B</track></playlist>";
# let playlist: Playlist = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(playlist.tracks, vec!["Song A", "Song B"]);
```

### Override element name with `rename`

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
struct Playlist {
    #[facet(rename = "song")]
    tracks: Vec<String>, // expects <song> instead of <track>
}
# let xml_str = "<playlist><song>Song A</song><song>Song B</song></playlist>";
# let playlist: Playlist = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(playlist.tracks, vec!["Song A", "Song B"]);
```

### Explicit `xml::elements` (same as default)

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
struct Playlist {
    #[facet(xml::elements)]
    tracks: Vec<String>,
}
# let xml_str = "<playlist><track>Song A</track><track>Song B</track></playlist>";
# let playlist: Playlist = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(playlist.tracks, vec!["Song A", "Song B"]);
```

### Lists of structs with `rename`

When collecting struct items, use `rename` to specify the element name. The rename overrides
the default singularized field name:

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug, PartialEq)]
struct Person {
    #[facet(xml::attribute)]
    name: String,
}

#[derive(Facet, Debug)]
struct Team {
    // "individual" instead of default "member" (singularized from "members")
    #[facet(xml::elements, rename = "individual")]
    members: Vec<Person>,
}
# let xml_str = r#"<team><individual name="Alice"/><individual name="Bob"/></team>"#;
# let team: Team = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(team.members.len(), 2);
# assert_eq!(team.members[0].name, "Alice");
```

### Collect text nodes with `xml::text`

```xml
<message>Hello <b>world</b>!</message>
```

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
struct Message {
    #[facet(xml::text)]
    parts: Vec<String>, // collects text nodes: ["Hello ", "!"]
}
# let xml_str = "<message>Hello world!</message>";
# let msg: Message = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(msg.parts, vec!["Hello world!"]);
```

### Collect attributes with `xml::attribute`

```xml
<element foo="1" bar="2" baz="3"/>
```

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug)]
#[facet(rename = "element")]
struct Element {
    #[facet(xml::attribute)]
    values: Vec<String>, // collects all attribute values
}
# let xml_str = r#"<element foo="1" bar="2" baz="3"/>"#;
# let elem: Element = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(elem.values, vec!["1", "2", "3"]);
```

## Flattened Lists (Heterogeneous Children)

When you have a `Vec<SomeEnum>` and want each enum variant to appear directly as a child element
(without a wrapper), use `#[facet(flatten)]`:

```xml
<canvas>
    <circle r="5"/>
    <rect width="10" height="20"/>
    <path d="M0 0 L10 10"/>
</canvas>
```

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Shape {
    Circle {
        #[facet(xml::attribute)]
        r: f64
    },
    Rect {
        #[facet(xml::attribute)]
        width: f64,
        #[facet(xml::attribute)]
        height: f64
    },
    Path {
        #[facet(xml::attribute)]
        d: String
    },
}

#[derive(Facet, Debug)]
struct Canvas {
    #[facet(flatten)]
    children: Vec<Shape>, // collects <circle>, <rect>, <path> directly
}
# let xml_str = r#"<canvas><circle r="5"/><rect width="10" height="20"/><path d="M0 0 L10 10"/></canvas>"#;
# let canvas: Canvas = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(canvas.children.len(), 3);
```

Without `#[facet(flatten)]`, the field would expect wrapper elements:

```xml
<!-- Without flatten: expects <child> wrappers -->
<canvas>
    <child><circle r="5"/></child>
    <child><rect width="10" height="20"/></child>
</canvas>
```

This pattern is essential for XML formats like SVG, HTML, or any schema where parent elements
contain heterogeneous children identified by their element names.

## Tuples

Tuples are treated like lists: each element becomes a child element with the field's name (or singularized name for plural field names). Elements are matched by position.

```xml
<record>
    <value>42</value>
    <value>hello</value>
    <value>true</value>
</record>
```

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "record")]
struct Record {
    #[facet(rename = "value")]
    data: (i32, String, bool),
}
# let xml_str = "<record><value>42</value><value>hello</value><value>true</value></record>";
# let record: Record = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(record.data, (42, "hello".to_string(), true));
```

Without `rename`, the field name is used directly (no singularization for tuples since tuple fields typically have singular names):

```rust
# use facet::Facet;
# use facet_xml as xml;
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "record")]
struct Record {
    data: (i32, String, bool),
}
# let xml_str = "<record><data>42</data><data>hello</data><data>true</data></record>";
# let record: Record = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(record.data, (42, "hello".to_string(), true));
```

## Enums

In XML, enums are always treated as **externally tagged** - the element name *is* the variant
discriminator. This is natural for XML because the element structure already provides tagging.

Any `#[facet(tag = "...")]` or `#[facet(content = "...")]` attributes are ignored for XML
serialization. These attributes are useful for JSON (which needs explicit tag fields), but
XML doesn't need them since element names serve this purpose.

### Unit variants

Unit variants become empty elements:

```rust
# use facet::Facet;
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Status {
    Active,
    Inactive,
}
// "Active" becomes <active> (lowerCamelCase)
# let xml_str = "<active/>";
# let status: Status = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(status, Status::Active);
```

### Newtype variants

Newtype variants (single unnamed field) wrap their content:

```rust
# use facet::Facet;
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Value {
    Text(String),
    Number(i32),
}
// <text>hello</text> deserializes to Value::Text("hello")
# let xml_str = "<text>hello</text>";
# let value: Value = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(value, Value::Text("hello".into()));
```

### Struct variants

Struct variants have child elements for their fields:

```rust
# use facet::Facet;
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}
// <circle><radius>5.0</radius></circle>
# let xml_str = "<circle><radius>5.0</radius></circle>";
# let shape: Shape = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(shape, Shape::Circle { radius: 5.0 });
```

### Variant renaming

Use `#[facet(rename = "...")]` on variants to override the element name:

```rust
# use facet::Facet;
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Status {
    #[facet(rename = "on")]
    Active,
    #[facet(rename = "off")]
    Inactive,
}
# let xml_str = "<on/>";
# let status: Status = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(status, Status::Active);
```

### Internally/adjacently tagged enums

Attributes like `#[facet(tag = "type")]` or `#[facet(tag = "t", content = "c")]` are
**ignored** for XML. They exist for JSON compatibility but don't affect XML serialization:

```rust
# use facet::Facet;
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
#[facet(tag = "type")] // ignored for XML!
enum Shape {
    Circle { radius: f64 },
}
// Still uses element name as discriminator
# let xml_str = "<circle><radius>3.0</radius></circle>";
# let shape: Shape = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(shape, Shape::Circle { radius: 3.0 });
```

### Untagged enums

Untagged enums (`#[facet(untagged)]`) use the enum's own name as the element, not a variant name.
The content determines which variant is selected:

```rust
# use facet::Facet;
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
#[facet(untagged, rename = "point")]
enum Point {
    Coords { x: i32, y: i32 },
}
# let xml_str = "<point><x>10</x><y>20</y></point>";
# let point: Point = facet_xml::from_str(xml_str).unwrap();
# assert_eq!(point, Point::Coords { x: 10, y: 20 });
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
