+++
title = "Extension Attributes"
weight = 2
insert_anchor_links = "heading"
+++

Extension attributes let your crate define custom `#[facet(...)]` attributes with **compile-time validation** and helpful error messages.

This page covers both using extension attributes and creating your own.

## Using extension attributes

```rust,noexec
use facet::Facet;
use facet_xml as xml;

#[derive(Facet)]
struct Server {
    #[facet(xml::attribute)]
    name: String,
    #[facet(xml::element)]
    host: String,
}
```

The namespace (`xml`) comes from how you import the crate:

```rust,noexec
use facet_xml as xml;  // Enables xml:: prefix
use figue as args;  // Enables args:: prefix
```

## Declaring attributes with `define_attr_grammar!`

Use the [`define_attr_grammar!`](https://docs.rs/facet/latest/facet/macro.define_attr_grammar.html) macro to declare your attribute grammar. Here's how [`facet-xml`](https://docs.rs/facet-xml) does it:

```rust,noexec
facet::define_attr_grammar! {
    ns "xml";
    crate_path ::facet_xml;

    /// XML attribute types for field and container configuration.
    pub enum Attr {
        /// Marks a field as a single XML child element
        Element,
        /// Marks a field as collecting multiple XML child elements
        Elements,
        /// Marks a field as an XML attribute (on the element tag)
        Attribute,
        /// Marks a field as the text content of the element
        Text,
        /// Marks a field as storing the XML element tag name dynamically
        Tag,
        /// Specifies the XML namespace URI for this field.
        Ns(&'static str),
        /// Specifies the default XML namespace URI for all fields in this container.
        NsAll(&'static str),
    }
}
```

This generates:

1. An `Attr` enum with variants for each attribute
2. Compile-time parsing that validates attribute usage
3. Type-safe data storage accessible at runtime

### Grammar components

| Component | Purpose | Example |
|-----------|---------|---------|
| `ns "...";` | Namespace for attributes | `ns "xml";` → `#[facet(xml::element)]` |
| `crate_path ...;` | Path to your crate for macro hygiene | `crate_path ::facet_xml;` |
| `pub enum Attr { ... }` | The attribute variants | See above |

### Variant types

#### Unit variants (markers)

Simple flags with no arguments:

```rust,noexec
pub enum Attr {
    /// A marker attribute
    Element,
}
```

Usage: `#[facet(xml::element)]`

#### String values

Attributes that take a string:

```rust,noexec
pub enum Attr {
    /// Rename to a different name
    Rename(&'static str),
}
```

Usage: `#[facet(rename = "new_name")]`

#### Optional characters

For single-character flags (like CLI short options):

```rust,noexec
pub enum Attr {
    /// Short flag, optionally with a character
    Short(Option<char>),
}
```

Usage: `#[facet(args::short)]` or `#[facet(args::short = 'v')]`

### Advanced: how built-in attributes work

The built-in facet attributes use additional payload types not typically needed by extension crates. For reference:

```rust,noexec
// Inside the facet crate itself:
define_attr_grammar! {
    builtin;
    ns "";
    crate_path ::facet::builtin;

    pub enum Attr {
        // Simple markers
        Sensitive,
        Skip,
        Flatten,

        // String values
        Rename(&'static str),
        Tag(&'static str),

        // Function-based defaults (uses field type's Default impl)
        Default(make_t or $ty::default()),

        // Predicate functions for conditional serialization
        SkipSerializingIf(predicate SkipSerializingIfFn),

        // Type references (for proxy serialization)
        Proxy(shape_type),
    }
}
```

These special payload types enable powerful features but are primarily for core facet development.

## Compile-Time validation

One of the major benefits of `define_attr_grammar!`: **typos are caught at compile time** with helpful suggestions.

```rust,noexec
#[derive(Facet)]
struct Parent {
    #[facet(xml::elemnt)]  // Typo!
    child: Child,
}
```

```
error: unknown attribute `elemnt`, did you mean `element`?
       available attributes: element, elements, attribute, text, tag, ns, ns_all
 --> src/lib.rs:4:12
  |
4 |     #[facet(xml::elemnt)]
  |            ^^^^^^^^^^^
```

The system uses string similarity to suggest corrections.

## Querying attributes at runtime

When your format crate needs to check for attributes, use the `get_as` method on [`Attr`](https://docs.rs/facet-core/latest/facet_core/struct.Attr.html):

```rust,noexec
use facet_core::{Field, FieldAttribute, Facet};
use facet_xml::Attr as XmlAttr;

fn process_field(field: &Field) {
    for attr in field.attributes {
        if let FieldAttribute::Extension(ext) = attr {
            // Check namespace first
            if ext.ns == Some("xml") {
                // Get typed attribute data
                if let Some(xml_attr) = ext.get_as::<XmlAttr>() {
                    match xml_attr {
                        XmlAttr::Element => { /* handle element */ }
                        XmlAttr::Attribute => { /* handle attribute */ }
                        XmlAttr::Text => { /* handle text content */ }
                        // ...
                    }
                }
            }
        }
    }
}
```

For built-in attributes:

```rust,noexec
use facet::builtin::Attr as BuiltinAttr;

for attr in field.attributes {
    if let FieldAttribute::Extension(ext) = attr {
        if ext.is_builtin() {
            if let Some(builtin) = ext.get_as::<BuiltinAttr>() {
                match builtin {
                    BuiltinAttr::Rename(name) => { /* use renamed field */ }
                    BuiltinAttr::Skip => { /* skip this field */ }
                    // ...
                }
            }
        }
    }
}
```

## Namespacing

- Use short aliases if desired: `use facet_xml as x; #[facet(x::element)]`.
- Namespaces prevent collisions across format crates.
- Built-in attributes remain short (`#[facet(rename = "...")]`, etc.).

## Real-World examples

### figue

[`figue`](https://docs.rs/figue) provides CLI argument parsing:

```rust,noexec
facet::define_attr_grammar! {
    ns "args";
    crate_path ::figue;

    pub enum Attr {
        /// Marks a field as a positional argument
        Positional,
        /// Marks a field as a named argument
        Named,
        /// Short flag character
        Short(Option<char>),
        /// Marks a field as a subcommand
        Subcommand,
    }
}
```

Usage:

```rust,noexec
use figue as args;

#[derive(Facet)]
struct Cli {
    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    #[facet(args::positional)]
    input: String,

    #[facet(args::subcommand)]
    command: Command,
}
```

### facet-xml

[`facet-xml`](https://docs.rs/facet-xml) provides XML-specific attributes:

```rust,noexec
facet::define_attr_grammar! {
    ns "xml";
    crate_path ::facet_xml;

    pub enum Attr {
        Element,
        Elements,
        Attribute,
        Text,
        Tag,
        Ns(&'static str),
        NsAll(&'static str),
    }
}
```

Usage:

```rust,noexec
use facet_xml as xml;

#[derive(Facet)]
struct Person {
    #[facet(xml::attribute)]
    id: u32,

    #[facet(xml::element)]
    name: String,

    #[facet(xml::text)]
    bio: String,
}
```

## Next steps
- Learn what information `Shape` exposes: [Shape](@/extend/shape.md).
- See how to read values: [Peek](@/extend/peek.md).
- Build values (strict vs deferred): [Partial](@/extend/partial.md).
- Put it together for a format crate: [Build a Format Crate](@/extend/format-crate.md).
