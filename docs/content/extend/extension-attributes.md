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
3. Type-safe runtime storage (either enum values or direct payload types, depending on variant kind)

### Grammar components

| Component | Purpose | Example |
|-----------|---------|---------|
| `ns "...";` | Namespace for attributes | `ns "xml";` → `#[facet(xml::element)]` |
| `crate_path ...;` | Path to your crate for macro hygiene | `crate_path ::facet_xml;` |
| `pub enum Attr { ... }` | The attribute variants | See above |

### Variant types and runtime storage (exhaustive)

`define_attr_grammar!` payload syntax determines the runtime type stored in `Attr.data`.

| Grammar payload syntax | Example variant | Runtime type stored in `Attr.data` | `attr.get_as::<your_ns::Attr>()` |
|---|---|---|---|
| _(no payload)_ | `Marker` | `()` | `None` |
| `&'static str` | `EnvPrefix(&'static str)` | `&'static str` | `None` |
| `i64` | `Min(i64)` | `i64` | `None` |
| `usize` | `MaxLen(usize)` | `usize` | `None` |
| `shape_type` | `Proxy(shape_type)` | `facet::Shape` | `None` |
| `predicate TypeName` | `SkipIf(predicate SkipSerializingIfFn)` | function payload (type-erased storage) | `None` |
| `validator TypeName` | `Validate(validator ValidatorFn)` | function payload (type-erased storage) | `None` |
| `make_t` / `make_t or $ty::default()` | `Default(make_t or $ty::default())` | `Option<facet::DefaultInPlaceFn>` | `None` |
| `arbitrary` | `FromRef(arbitrary)` | `()` | `None` |
| `Option<char>` | `Short(Option<char>)` | `your_ns::Attr` | `Some(...)` |
| `Option<&'static str>` | `Name(Option<&'static str>)` | `your_ns::Attr` | `Some(...)` |
| `&'static SomeType` | `Mode(&'static Mode)` | `your_ns::Attr` | `Some(...)` |
| `StructName` | `Column(Column)` | `your_ns::Attr` | `Some(...)` |
| `fn_ptr TypeName` | `Hook(fn_ptr HookFn)` | `your_ns::Attr` | `Some(...)` |
| any other payload type | `Custom(MyType)` | `your_ns::Attr` | `Some(...)` |

If a variant is marked `#[storage(flag)]` or `#[storage(field)]`, treat the dedicated accessor/field as the source of truth. Do not rely on whether it also appears in `field.attributes`.

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

`Field::attributes` is a slice of [`Attr`](https://docs.rs/facet-core/latest/facet_core/struct.Attr.html).  
`FieldAttribute` is a type alias to `Attr`.

Use this decoding flow:

1. Select by namespace + key (`field.get_attr(Some("ns"), "key")`).
2. Decode with the exact runtime payload type from the table above.
3. For built-in attributes, prefer dedicated fields/accessors (`field.rename`, `field.is_sensitive()`, flags) before scanning `attributes`.

```rust
use facet_core::Field;

fn process_field(field: &Field) {
    if let Some(attr) = field.get_attr(Some("docs"), "env_prefix") {
        let prefix = attr
            .get_as::<&'static str>()
            .expect("docs::env_prefix stores &'static str");
        println!("env prefix: {}", *prefix);
    }

    if field.get_attr(Some("docs"), "marker").is_some() {
        println!("marker present");
    }
}
```

```rust
use facet_core::Field;

fn process_builtin(field: &Field) {
    if let Some(name) = field.rename {
        println!("renamed to {name}");
    }

    if field.is_sensitive() {
        println!("sensitive field");
    }
}
```

### Runnable reference implementation

- Run: `cargo run -p facet --example extension_attr_runtime_matrix`
- Verified in tests: `cargo nextest run -p facet --test main extension_attr_runtime_matrix`
- Source example: [`facet/examples/extension_attr_runtime_matrix.rs`](https://github.com/facet-rs/facet/blob/main/facet/examples/extension_attr_runtime_matrix.rs)
- Source test: [`facet/tests/integration/extension_attr_runtime_matrix.rs`](https://github.com/facet-rs/facet/blob/main/facet/tests/integration/extension_attr_runtime_matrix.rs)

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
