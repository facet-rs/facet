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

### Runtime decoding contract (exhaustive)

The enum declaration in `define_attr_grammar!` is your runtime contract.
Facet stores either:

1. A direct payload (`usize`, `i64`, `&'static str`, `()`, `Shape`, function payloads), or
2. The generated enum value (`your_ns::Attr`).

Decode with `attr.get_as::<T>()` where `T` matches what is actually stored.

| Variant declaration in your grammar | Stored in `Attr.data` | Decode code |
|---|---|---|
| `Marker` | `()` | `attr.get_as::<()>().is_some()` (or key presence only) |
| `EnvPrefix(&'static str)` | `&'static str` | `attr.get_as::<&'static str>()` |
| `Min(i64)` | `i64` | `attr.get_as::<i64>()` |
| `MaxLen(usize)` | `usize` | `attr.get_as::<usize>()` |
| `Proxy(shape_type)` | `facet::Shape` | `attr.get_as::<facet::Shape>()` |
| `SkipIf(predicate SkipSerializingIfFn)` | function payload | decode with the function payload type |
| `Validate(validator ValidatorFn)` | function payload | decode with the function payload type |
| `Default(make_t ...)` | `Option<facet::DefaultInPlaceFn>` | decode as `Option<facet::DefaultInPlaceFn>` |
| `FromRef(arbitrary)` | `()` | usually check key presence |
| `Short(Option<char>)` | `your_ns::Attr` | `attr.get_as::<your_ns::Attr>()` then match variant |
| `Name(Option<&'static str>)` | `your_ns::Attr` | `attr.get_as::<your_ns::Attr>()` then match variant |
| `Mode(&'static SomeType)` | `your_ns::Attr` | `attr.get_as::<your_ns::Attr>()` then match variant |
| `Column(Column)` | `your_ns::Attr` | `attr.get_as::<your_ns::Attr>()` then match variant |
| `Hook(fn_ptr HookFn)` | `your_ns::Attr` | `attr.get_as::<your_ns::Attr>()` then match variant |
| `Custom(MyType)` | `your_ns::Attr` | `attr.get_as::<your_ns::Attr>()` then match variant |

`Column(Column)` is not special: `Column` is a struct you define in the same grammar.

```rust
facet::define_attr_grammar! {
    ns "myfmt";
    crate_path ::myfmt;

    pub enum Attr {
        Column(Column),
    }

    pub struct Column {
        pub rename: Option<&'static str>,
        pub indexed: bool,
    }
}
```

If a variant has `#[storage(flag)]` or `#[storage(field)]`, use the generated dedicated accessor/field as the primary API.

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

Use this pipeline every time:

1. Declare the variant in your grammar.
2. Apply the attribute on the reflected type.
3. Query by `ns + key`, then decode with the exact runtime type.

### Pipeline: numeric payload (`usize`)

```rust
use facet::{Facet, StructType, Type, UserType};
use facet_testattrs as testattrs;

#[derive(Facet)]
struct User {
    #[facet(testattrs::max_len = 64)]
    name: String,
}

let Type::User(UserType::Struct(StructType { fields, .. })) = User::SHAPE.ty else {
    panic!("expected struct");
};
let field = &fields[0];

let max_len: usize = field
    .get_attr(Some("testattrs"), "max_len")
    .and_then(|attr| attr.get_as::<usize>().copied())
    .expect("testattrs::max_len should decode as usize");
assert_eq!(max_len, 64);
```

Why this is `usize`: the variant is declared as `MaxLen(usize)`, and that payload form is stored directly as `usize`.

Typical use cases:
- Maximum string/list length constraints during parsing.
- Emitting schema limits (for example JSON Schema `maxLength`).
- Pre-validation before allocation-heavy decode paths.

### Pipeline: struct payload (`Column(Column)`)

```rust
use facet::{Facet, StructType, Type, UserType};
use facet_testattrs as testattrs;

#[derive(Facet)]
struct IndexedUser {
    #[facet(testattrs::column(rename = "user_name", indexed))]
    username: String,
}

let Type::User(UserType::Struct(StructType { fields, .. })) = IndexedUser::SHAPE.ty else {
    panic!("expected struct");
};
let field = &fields[0];

let attr = field
    .get_attr(Some("testattrs"), "column")
    .expect("column attr should exist");

let decoded = attr
    .get_as::<testattrs::Attr>()
    .expect("column payload is wrapped in testattrs::Attr");

match decoded {
    testattrs::Attr::Column(column) => {
        assert_eq!(column.rename, Some("user_name"));
        assert!(column.indexed);
    }
    _ => panic!("unexpected variant"),
}
```

Typical use cases:
- Per-field database/index metadata.
- Format-specific output shape options (renaming, indexing, flags).
- Rich configuration that is awkward as flat scalar attributes.

### Built-in attributes

For built-ins, use dedicated accessors/fields first:

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

### Runnable references

- Numeric/string/unit pipeline:
  - Run: `cargo run -p facet --example extension_attr_runtime_matrix`
  - Test: `cargo nextest run -p facet --test main extension_attr_runtime_matrix`
  - Source: [`facet/examples/extension_attr_runtime_matrix.rs`](https://github.com/facet-rs/facet/blob/main/facet/examples/extension_attr_runtime_matrix.rs)
- Struct payload pipeline (`Column(Column)`):
  - Run: `cargo run -p facet --example extension_attr_struct_payload`
  - Test: `cargo nextest run -p facet --test main extension_attr_struct_payload`
  - Source: [`facet/examples/extension_attr_struct_payload.rs`](https://github.com/facet-rs/facet/blob/main/facet/examples/extension_attr_struct_payload.rs)

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
