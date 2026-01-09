+++
title = "Extension Attributes"
weight = 1
+++

Extension attributes allow format crates to define custom `#[facet(...)]` attributes with **compile-time validation** and helpful error messages.

For the full guide on creating extension attributes, see [Extend → Extension Attributes](/extend/#extension-attributes).

## Quick reference

### Using extension attributes

```rust,noexec
use facet::Facet;
use facet_kdl as kdl;

#[derive(Facet)]
struct Config {
    #[facet(kdl::child)]
    server: Server,

    #[facet(kdl::property)]
    name: String,
}
```

The namespace (`kdl`) comes from how you import the crate:

```rust,noexec
use facet_kdl as kdl;  // Enables kdl:: prefix
use facet_args as args;  // Enables args:: prefix
```

### Available namespaces

| Crate | Namespace | Example Attributes |
|-------|-----------|-------------------|
| [`facet-args`](https://docs.rs/facet-args) | `args` | `positional`, `named`, `short`, `subcommand` |
| [`facet-yaml`](https://docs.rs/facet-yaml) | `serde` | `rename` |

### Creating your own

Use [`define_attr_grammar!`](https://docs.rs/facet/latest/facet/macro.define_attr_grammar.html) in your crate:

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

See [Extend](/extend/) for the complete guide on declaring attributes and querying them at runtime.
