# facet-xml

[![crates.io](https://img.shields.io/crates/v/facet-xml.svg)](https://crates.io/crates/facet-xml)
[![documentation](https://docs.rs/facet-xml/badge.svg)](https://docs.rs/facet-xml)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-xml.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

XML serialization and deserialization for Facet types.

## Quick start

Add `facet-xml` alongside your Facet types and derive `Facet`:

```rust
use facet::Facet;
use facet_xml as xml;

#[derive(Facet, Debug, PartialEq)]
struct Person {
    #[facet(xml::attribute)]
    id: u32,
    #[facet(xml::element)]
    name: String,
    #[facet(xml::element)]
    age: Option<u32>,
}

fn main() -> Result<(), facet_xml::XmlError> {
    let xml_str = r#"<Person id="42"><name>Alice</name></Person>"#;
    let person: Person = facet_xml::from_str(xml_str)?;
    assert_eq!(person.name, "Alice");
    assert_eq!(person.id, 42);
    assert_eq!(person.age, None);

    let output = facet_xml::to_string(&person)?;
    // Output: <Person id="42"><name>Alice</name></Person>
    Ok(())
}
```

## Common patterns

### Attributes vs Elements

XML has two main ways to represent data: attributes and elements.

```xml
<!-- Attributes are key="value" pairs in the opening tag -->
<Person id="42" active="true">
    <!-- Elements are nested tags -->
    <name>Alice</name>
    <email>alice@example.com</email>
</Person>
```

Use `#[facet(xml::attribute)]` for simple values that fit well as attributes:

```rust
use facet::Facet;
use facet_xml as xml;

#[derive(Facet)]
struct Person {
    #[facet(xml::attribute)]
    id: u32,
    #[facet(xml::attribute)]
    active: bool,
    #[facet(xml::element)]
    name: String,
    #[facet(xml::element)]
    email: String,
}
```

### Lists of elements

Use `#[facet(xml::elements)]` for collections:

```rust
use facet::Facet;
use facet_xml as xml;

#[derive(Facet)]
struct Library {
    #[facet(xml::attribute)]
    name: String,
    #[facet(xml::elements)]
    books: Vec<Book>,
}

#[derive(Facet)]
struct Book {
    #[facet(xml::attribute)]
    isbn: String,
    #[facet(xml::element)]
    title: String,
}
// Deserializes:
// <Library name="City Library">
//   <Book isbn="123"><title>1984</title></Book>
//   <Book isbn="456"><title>Brave New World</title></Book>
// </Library>
```

### Text content

Use `#[facet(xml::text)]` for the text content of an element:

```rust
use facet::Facet;
use facet_xml as xml;

#[derive(Facet)]
struct Message {
    #[facet(xml::attribute)]
    from: String,
    #[facet(xml::attribute)]
    to: String,
    #[facet(xml::text)]
    content: String,
}
// Deserializes: <Message from="alice" to="bob">Hello, world!</Message>
```

### Optional fields

Fields with `Option<T>` are automatically treated as optional:

```rust
use facet::Facet;
use facet_xml as xml;

#[derive(Facet)]
struct Config {
    #[facet(xml::element)]
    name: String,
    #[facet(xml::element)]
    description: Option<String>,  // Optional element
    #[facet(xml::attribute)]
    version: Option<u32>,         // Optional attribute
}
```

## Feature flags

- `default`/`std`: enables `std` for dependencies.
- `alloc`: `no_std` builds with `alloc` only.

## Error reporting

Errors use `miette` spans where possible, so diagnostics can point back to the offending XML source.

## License

MIT OR Apache-2.0, at your option.
