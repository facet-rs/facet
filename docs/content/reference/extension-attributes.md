+++
title = "Extension attributes"
weight = 4
insert_anchor_links = "heading"
+++

Format crates can define their own namespaced attributes with compile-time
validation and helpful error messages. This is the catalog; for how to design
your own grammar, see the [Extend → Extension Attributes](@/extend/extension-attributes.md) guide.

## Using extension attributes

The namespace comes from how you import the crate:

```rust,noexec
use facet::Facet;
use facet_xml as xml;
use figue as args;

#[derive(Facet)]
struct Config {
    #[facet(xml::element)]
    server: Server,

    #[facet(xml::attribute)]
    name: String,
}

#[derive(Facet)]
struct Cli {
    #[facet(args::positional)]
    input: String,

    #[facet(args::named, args::short = 'o')]
    output: Option<String>,
}
```

## Available namespaces

| Crate | Namespace | Attributes |
|-------|-----------|------------|
| [`figue`](https://docs.rs/figue) | `args` | `positional`, `named`, `short`, `subcommand` |
| [`facet-xml`](https://docs.rs/facet-xml) | `xml` | `element`, `elements`, `attribute`, `text`, `tag`, `ns`, `ns_all`, `proxy` |
| [`facet-html`](https://docs.rs/facet-html) | `html` | `element`, `elements`, `attribute`, `text`, `tag`, `custom_element`, `proxy` |
| [`facet-yaml`](https://docs.rs/facet-yaml) | `serde` | `rename` |
| [`facet-json`](https://docs.rs/facet-json) | `json` | `proxy` |

**Note:** The `proxy` attribute is available for any format namespace (e.g., `json::proxy`, `xml::proxy`). It uses the same syntax as the format-agnostic `proxy` attribute but only applies when serializing/deserializing with that specific format.

## Creating your own

Use [`define_attr_grammar!`](https://docs.rs/facet/latest/facet/macro.define_attr_grammar.html):

```rust,noexec
facet::define_attr_grammar! {
    ns "myformat";
    crate_path ::my_format_crate;

    pub enum Attr {
        /// Description shown in error messages
        MyAttribute,
        /// Attribute with a value
        WithValue(&'static str),
    }
}
```

Typos produce helpful compile errors:

```
error: unknown attribute `chld`, did you mean `child`?
       available attributes: child, children, property, argument
```

See [Extend → Extension Attributes](@/extend/extension-attributes.md) for the complete guide.

---

See also: [Container attributes](@/reference/container-attributes.md) · [Enum & variant attributes](@/reference/enum-attributes.md) · [Field attributes](@/reference/field-attributes.md)
