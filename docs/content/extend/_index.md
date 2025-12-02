+++
title = "Extend"
sort_by = "weight"
weight = 2
insert_anchor_links = "heading"
+++

This guide is for developers building format crates (like [`facet-json`](https://docs.rs/facet-json), [`facet-kdl`](https://docs.rs/facet-kdl)) or tools that use facet's reflection system. If you just want to *use* facet for serialization, see the [Learn](/learn) section instead.

## Extension Attributes

Extension attributes let your crate define custom `#[facet(...)]` attributes that users can put on their types. For example, [`facet-kdl`](https://docs.rs/facet-kdl) defines attributes like `kdl::child` and `kdl::argument`:

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

The key insight: **even facet's built-in attributes use this system**. Attributes like `rename`, `default`, and `skip` are defined using the exact same macro that third-party crates use.

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
| `pub enum Attr { ... }` | The attribute variants | See below |

### Variant Types

Your enum variants can hold different types of data:

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

These special payload types (`make_t`, `predicate`, `shape_type`) enable powerful features but are primarily for core facet development.

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

The system uses string similarity ([Jaro-Winkler distance](https://en.wikipedia.org/wiki/Jaro%E2%80%93Winkler_distance) via [`strsim`](https://docs.rs/strsim)) to suggest corrections.

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
                    BuiltinAttr::Rename(name) => {
                        // Use the renamed field name
                    }
                    BuiltinAttr::Skip => {
                        // Skip this field entirely
                    }
                    // ...
                }
            }
        }
    }
}
```

## The Namespacing Question

You might think: "But now everything needs `kdl::` or `args::` prefixes! That's verbose!"

A few responses:

**1. Use short aliases.** If you really want brevity:

```rust
use facet_kdl as k;

#[derive(Facet)]
struct Config {
    #[facet(k::child)]
    server: Server,
}
```

**2. Namespaces prevent conflicts.** What if you're using both [`facet-kdl`](https://docs.rs/facet-kdl) and [`facet-args`](https://docs.rs/facet-args)? Without namespaces, attributes could collide.

**3. Built-in attributes are still short.** The most common attributes don't need a namespace:

```rust
#[derive(Facet)]
struct User {
    #[facet(rename = "user_name")]
    name: String,
    #[facet(skip)]
    internal_id: u64,
    #[facet(default)]
    role: String,
}
```

The namespace is only required for format-specific attributes that wouldn't make sense globally.

## Real-World Examples

### facet-args

[`facet-args`](https://docs.rs/facet-args) provides CLI argument parsing with short flags and subcommands:

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

[`facet-yaml`](https://docs.rs/facet-yaml) provides YAML support with serde-compatible attribute names:

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

Notice how this is nested in a `serde` module — you can organize your attributes however makes sense for your crate.

## Summary

| What | How |
|------|-----|
| Declare attributes | `facet::define_attr_grammar! { ... }` |
| Namespace | `ns "your_ns";` |
| Crate path | `crate_path ::your_crate;` |
| Simple marker | `AttributeName,` |
| With string | `AttributeName(&'static str),` |
| Optional char | `AttributeName(Option<char>),` |
| Query at runtime | `ext.get_as::<YourAttr>()` |
| Check builtin | `ext.is_builtin()` |

The attribute grammar system gives you compile-time validation, type-safe runtime access, and helpful error messages — all from a single macro invocation.

## Next Steps

- See how [`facet-kdl`](https://docs.rs/facet-kdl) uses extension attributes for KDL-specific features
- Check the [Contribute guide](/contribute/) if you want to work on facet itself
- Browse the [API documentation](https://docs.rs/facet) for the full type reference
