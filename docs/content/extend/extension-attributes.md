+++
title = "Extension Attributes"
weight = 2
insert_anchor_links = "heading"
+++

Extension attributes let your crate define custom `#[facet(...)]` attributes with **compile-time validation** and helpful error messages.

For the full guide on creating extension attributes, see this page; for a quick consumer reference, see [Extension Attributes](@/extension-attributes.md).

## Using Extension Attributes

```rust
use facet::Facet;
use facet_kdl as kdl;

#[derive(Facet)]
struct Server {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::property)]
    host: String,
}
```

The namespace (`kdl`) comes from how you import the crate:

```rust
use facet_kdl as kdl;  // Enables kdl:: prefix
use facet_args as args;  // Enables args:: prefix
```

## Declaring Attributes with `define_attr_grammar!`

Use the [`define_attr_grammar!`](https://docs.rs/facet/latest/facet/macro.define_attr_grammar.html) macro to declare your attribute grammar. Here's how [`facet-kdl`](https://docs.rs/facet-kdl) does it:

```rust
facet::define_attr_grammar! {
    ns "kdl";
    crate_path ::facet_kdl;

    /// KDL attribute types for field and container configuration.
    pub enum Attr {
        /// Marks a field as a single KDL child node
        Child,
        /// Marks a field as collecting multiple KDL children
        Children,
        /// Marks a field as a KDL property (key=value)
        Property,
        /// Marks a field as a single KDL positional argument
        Argument,
        /// Marks a field as collecting all KDL positional arguments
        Arguments,
        /// Marks a field as storing the KDL node name
        NodeName,
    }
}
```

This generates:

1. An `Attr` enum with variants for each attribute
2. Compile-time parsing that validates attribute usage
3. Type-safe data storage accessible at runtime

### Grammar Components

| Component | Purpose | Example |
|-----------|---------|---------|
| `ns "...";` | Namespace for attributes | `ns "kdl";` → `#[facet(kdl::child)]` |
| `crate_path ...;` | Path to your crate for macro hygiene | `crate_path ::facet_kdl;` |
| `pub enum Attr { ... }` | The attribute variants | See above |

### Variant Types

#### Unit Variants (Markers)

Simple flags with no arguments:

```rust
pub enum Attr {
    /// A marker attribute
    Child,
}
```

Usage: `#[facet(kdl::child)]`

#### String Values

Attributes that take a string:

```rust
pub enum Attr {
    /// Rename to a different name
    Rename(&'static str),
}
```

Usage: `#[facet(rename = "new_name")]`

#### Optional Characters

For single-character flags (like CLI short options):

```rust
pub enum Attr {
    /// Short flag, optionally with a character
    Short(Option<char>),
}
```

Usage: `#[facet(args::short)]` or `#[facet(args::short = 'v')]`

### Advanced: How Built-in Attributes Work

The built-in facet attributes use additional payload types not typically needed by extension crates. For reference:

```rust
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

        // Function-based defaults
        Default(make_t),

        // Predicate functions for conditional serialization
        SkipSerializingIf(predicate SkipSerializingIfFn),

        // Type references (for proxy serialization)
        Proxy(shape_type),
    }
}
```

These special payload types enable powerful features but are primarily for core facet development.

## Compile-Time Validation

One of the major benefits of `define_attr_grammar!`: **typos are caught at compile time** with helpful suggestions.

```rust
#[derive(Facet)]
struct Parent {
    #[facet(kdl::chld)]  // Typo!
    child: Child,
}
```

```
error: unknown attribute `chld`, did you mean `child`?
       available attributes: child, children, property, argument, arguments, node_name
 --> src/lib.rs:4:12
  |
4 |     #[facet(kdl::chld)]
  |            ^^^^^^^^^
```

The system uses string similarity to suggest corrections.

## Querying Attributes at Runtime

When your format crate needs to check for attributes, use the `get_as` method on [`ExtensionAttr`](https://docs.rs/facet-core/latest/facet_core/struct.ExtensionAttr.html):

```rust
use facet_core::{Field, FieldAttribute, Facet};
use facet_kdl::Attr as KdlAttr;

fn process_field(field: &Field) {
    for attr in field.attributes {
        if let FieldAttribute::Extension(ext) = attr {
            // Check namespace first
            if ext.ns == Some("kdl") {
                // Get typed attribute data
                if let Some(kdl_attr) = ext.get_as::<KdlAttr>() {
                    match kdl_attr {
                        KdlAttr::Child => { /* handle child */ }
                        KdlAttr::Property => { /* handle property */ }
                        KdlAttr::Argument => { /* handle argument */ }
                        // ...
                    }
                }
            }
        }
    }
}
```

For built-in attributes:

```rust
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

- Use short aliases if desired: `use facet_kdl as k; #[facet(k::child)]`.
- Namespaces prevent collisions across format crates.
- Built-in attributes remain short (`#[facet(rename = "...")]`, etc.).

## Real-World Examples

### facet-args

[`facet-args`](https://docs.rs/facet-args) provides CLI argument parsing:

```rust
facet::define_attr_grammar! {
    ns "args";
    crate_path ::facet_args;

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

```rust
use facet_args as args;

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

### facet-yaml (serde compatibility)

[`facet-yaml`](https://docs.rs/facet-yaml) provides serde-compatible names:

```rust
pub mod serde {
    facet::define_attr_grammar! {
        ns "serde";
        crate_path ::facet_yaml::serde;

        pub enum Attr {
            /// Rename a field
            Rename(&'static str),
        }
    }
}
```

Usage:

```rust
use facet_yaml::serde;

#[derive(Facet)]
struct Config {
    #[facet(serde::rename = "serverName")]
    server_name: String,
}
```

## Next Steps
- Learn what information `Shape` exposes: [Shape](@/extend/shape.md).
- See how to read values: [Peek](@/extend/peek.md).
- Build values (strict vs deferred): [Partial](@/extend/partial.md).
- Put it together for a format crate: [Build a Format Crate](@/extend/format-crate.md).
