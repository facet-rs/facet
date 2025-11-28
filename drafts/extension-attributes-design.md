# Extension Attributes Design

## Overview

Extension attributes allow third-party crates (like `facet-kdl`, `facet-args`) to define custom attributes that users can apply to their types via `#[facet(ns::attr)]` syntax.

## User-Facing Syntax

```rust
#[derive(Facet)]
struct Config {
    #[facet(kdl::child)]
    server: Server,

    #[facet(kdl::property)]
    name: String,
}

#[derive(Facet)]
struct Args {
    #[facet(args::named, args::short = 'j')]
    concurrency: usize,
}
```

## Crate Author Side

Crates define their attributes using the `define_extension_attrs!` macro:

```rust
// In facet-kdl/src/lib.rs

facet::define_extension_attrs! {
    /// Marks a field as a KDL child node.
    ///
    /// # Example
    /// ```
    /// #[derive(Facet)]
    /// struct Parent {
    ///     #[facet(kdl::child)]
    ///     child: Child,
    /// }
    /// ```
    child,

    /// Marks a field as collecting multiple KDL children.
    children,

    /// Marks a field as a KDL property (key=value).
    property,

    /// Marks a field as a KDL positional argument.
    argument,

    /// Marks a field as collecting all KDL positional arguments.
    arguments,

    /// Captures the KDL node name into a field.
    node_name,
}
```

For attributes that take arguments and need custom parsing:

```rust
facet::define_extension_attrs! {
    /// Marks this as a named argument.
    named,

    /// Specifies a short flag character.
    ///
    /// # Example
    /// ```
    /// #[derive(Facet)]
    /// struct Args {
    ///     #[facet(args::short = 'v')]
    ///     verbose: bool,
    /// }
    /// ```
    short(args) -> &'static char {
        // Parse the character from tokens
        // args is &[facet::Token]
        static CHAR: std::sync::OnceLock<char> = std::sync::OnceLock::new();
        CHAR.get_or_init(|| {
            // Parse '=' and char literal from args
            parse_char_arg(args).expect("short requires a char argument")
        })
    },
}
```

## Macro Expansion

The `define_extension_attrs!` macro expands to a `pub mod attrs` with documented functions:

```rust
/// Extension attributes for KDL serialization.
///
/// Use these with `#[facet(kdl::attr)]` syntax.
pub mod attrs {
    /// Marks a field as a KDL child node.
    ///
    /// # Example
    /// ```
    /// #[derive(Facet)]
    /// struct Parent {
    ///     #[facet(kdl::child)]
    ///     child: Child,
    /// }
    /// ```
    pub fn child(_args: &[::facet::Token]) -> &'static (dyn ::core::any::Any + ::core::marker::Send + ::core::marker::Sync) {
        static __UNIT: () = ();
        &__UNIT
    }

    /// Marks a field as collecting multiple KDL children.
    pub fn children(_args: &[::facet::Token]) -> &'static (dyn ::core::any::Any + ::core::marker::Send + ::core::marker::Sync) {
        static __UNIT: () = ();
        &__UNIT
    }

    /// Specifies a short flag character.
    pub fn short(args: &[::facet::Token]) -> &'static (dyn ::core::any::Any + ::core::marker::Send + ::core::marker::Sync) {
        static CHAR: std::sync::OnceLock<char> = std::sync::OnceLock::new();
        CHAR.get_or_init(|| {
            parse_char_arg(args).expect("short requires a char argument")
        })
    }

    // ... etc
}
```

## Generated Code (Derive Macro Side)

When `#[derive(Facet)]` sees `#[facet(kdl::child)]`, it generates:

```rust
::facet::FieldAttribute::Extension({
    fn __ext_get() -> &'static (dyn ::core::any::Any + ::core::marker::Send + ::core::marker::Sync) {
        kdl::attrs::child(&[])
    }
    ::facet::ExtensionAttr {
        ns: "kdl",
        key: "child",
        get: __ext_get,
    }
})
```

For `#[facet(args::short = 'j')]`:

```rust
::facet::FieldAttribute::Extension({
    fn __ext_get() -> &'static (dyn ::core::any::Any + ::core::marker::Send + ::core::marker::Sync) {
        args::attrs::short(&[
            ::facet::Token::Punct { ch: '=', joint: false, span: ::facet::TokenSpan::DUMMY },
            ::facet::Token::Literal { kind: ::facet::LiteralKind::Char, text: "'j'", span: ::facet::TokenSpan::DUMMY },
        ])
    }
    ::facet::ExtensionAttr {
        ns: "args",
        key: "short",
        get: __ext_get,
    }
})
```

## ExtensionAttr Struct

The `ExtensionAttr` struct is simplified (no more `args` field):

```rust
pub struct ExtensionAttr {
    /// The namespace (e.g., "kdl" in `#[facet(kdl::child)]`)
    pub ns: &'static str,

    /// The key (e.g., "child" in `#[facet(kdl::child)]`)
    pub key: &'static str,

    /// Function to get the parsed/typed data from the extension crate
    pub get: fn() -> &'static (dyn core::any::Any + Send + Sync),
}

impl ExtensionAttr {
    /// Get the typed data, downcasting from `dyn Any`.
    pub fn get_as<T: core::any::Any>(&self) -> Option<&'static T> {
        (self.get)().downcast_ref()
    }
}
```

## Compile-Time Validation with Beautiful Diagnostics

When a user writes `#[facet(kdl::nonexistent)]`, we want a helpful error message like:

```
error[E0277]: `nonexistent` is not a recognized KDL attribute
  --> src/lib.rs:5:18
   |
5  |     #[facet(kdl::nonexistent)]
   |                  ^^^^^^^^^^^ unknown attribute
   |
   = help: valid attributes are: `child`, `children`, `argument`, `property`, ...
```

### The Trick

The derive macro generates code with:

1. **Fallback struct** defined in outer scope with the user's span
2. **Glob import** from `{ns}::attrs::*` that shadows the fallback if valid
3. **Trait bound check** that triggers `#[diagnostic::on_unimplemented]`

```rust
// Generated by #[derive(Facet)] for #[facet(kdl::nonexistent)]
{
    // Fallback type - uses span from user's tokens!
    struct nonexistent;

    {
        // Glob import - NEVER fails, even if nothing matches
        // But if `child` exists in attrs, it shadows the outer fallback
        use kdl::attrs::*;

        // Trait bound check - triggers on_unimplemented if invalid
        fn __check<A>() where kdl::ValidAttr<A>: kdl::IsValidAttr {}
        __check::<nonexistent>();
    }

    // Actual ExtensionAttr code...
    fn __ext_get() -> &'static (dyn ::core::any::Any + Send + Sync) {
        kdl::attrs::nonexistent(&[])
    }
    ::facet::ExtensionAttr { ns: "kdl", key: "nonexistent", get: __ext_get }
}
```

### Crate-Side Setup

The `define_extension_attrs!` macro generates:

1. A `pub mod attrs` with a **rich aggregated doc comment** (the public documentation)
2. All internal structs, functions, traits are `#[doc(hidden)]`

```rust
/// Extension attributes for KDL serialization.
///
/// Use these with `#[facet(kdl::attr)]` syntax.
///
/// # Available Attributes
///
/// ## `child`
///
/// Marks a field as a KDL child node.
///
/// ```rust
/// #[derive(Facet)]
/// struct Parent {
///     #[facet(kdl::child)]
///     child: Child,
/// }
/// ```
///
/// ## `children`
///
/// Marks a field as collecting multiple KDL children.
///
/// ```rust
/// #[derive(Facet)]
/// struct Parent {
///     #[facet(kdl::children)]
///     items: Vec<Item>,
/// }
/// ```
///
/// ## `argument`
///
/// Marks a field as a KDL positional argument.
///
/// ```rust
/// #[derive(Facet)]
/// struct Node {
///     #[facet(kdl::argument)]
///     value: String,
/// }
/// ```
///
/// ## `arguments`
///
/// Marks a field as collecting all KDL positional arguments.
///
/// ## `property`
///
/// Marks a field as a KDL property (key=value).
///
/// ## `node_name`
///
/// Captures the KDL node name into a field.
pub mod attrs {
    #[doc(hidden)]
    pub struct child;

    #[doc(hidden)]
    pub fn child(_args: &[::facet::Token]) -> &'static (dyn ::core::any::Any + Send + Sync) {
        static __UNIT: () = ();
        &__UNIT
    }

    #[doc(hidden)]
    pub struct children;

    #[doc(hidden)]
    pub fn children(_args: &[::facet::Token]) -> &'static (dyn ::core::any::Any + Send + Sync) {
        static __UNIT: () = ();
        &__UNIT
    }

    // ... etc, all #[doc(hidden)]
}

/// Marker struct for attribute validation.
#[doc(hidden)]
pub struct ValidAttr<A>(core::marker::PhantomData<A>);

/// Trait for compile-time attribute validation.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{A}` is not a recognized KDL attribute",
    label = "unknown attribute",
    note = "valid attributes are: `child`, `children`, `argument`, `arguments`, `property`, `node_name`"
)]
pub trait IsValidAttr {}

// All trait impls are also hidden
#[doc(hidden)]
impl IsValidAttr for ValidAttr<attrs::child> {}
#[doc(hidden)]
impl IsValidAttr for ValidAttr<attrs::children> {}
#[doc(hidden)]
impl IsValidAttr for ValidAttr<attrs::argument> {}
// ... etc
```

This way, users browsing the docs see:

- `facet_kdl::attrs` - one clean module page with all attributes documented
- No clutter from internal structs, functions, or trait machinery

### Why This Works

1. User writes `#[facet(kdl::nonexistent)]`
2. Macro generates `struct nonexistent;` with the **user's span**
3. Glob import `use kdl::attrs::*;` brings in `child`, `children`, etc. but NOT `nonexistent`
4. The name `nonexistent` resolves to the outer fallback struct
5. `ValidAttr<nonexistent>` does NOT implement `IsValidAttr`
6. `#[diagnostic::on_unimplemented]` kicks in with a beautiful error
7. Error points at user's code because the fallback struct has their span

### For Valid Attributes

1. User writes `#[facet(kdl::child)]`
2. Macro generates `struct child;` in outer scope
3. Glob import brings in `kdl::attrs::child` which **shadows** the outer one
4. `ValidAttr<attrs::child>` DOES implement `IsValidAttr`
5. Compiles successfully

## Parsing Attribute Arguments

For attributes that take arguments like `#[facet(args::short = 'j')]` or `#[facet(orm::index(unique, name = "idx_email"))]`, the attribute function receives raw tokens and can parse them into structured data.

### Built-in Parsing Helpers

`facet-core` provides `ParsedArgs` for common patterns:

```rust
pub struct ParsedArgs {
    /// Positional arguments (in order)
    pub positional: Vec<TokenValue>,
    /// Named arguments (key = value pairs)
    pub named: BTreeMap<String, TokenValue>,
}
```

Where `TokenValue` can be:

```rust
pub enum TokenValue {
    String(String),
    StaticStr(&'static str),
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
    Char(char),
    List(Vec<TokenValue>),
    Map(BTreeMap<String, TokenValue>),
}
```

### Example: Parsing `args::short = 'j'`

```rust
facet::define_extension_attrs! {
    /// Specifies a short flag character.
    ///
    /// # Example
    /// ```
    /// #[facet(args::short = 'v')]
    /// verbose: bool,
    /// ```
    short(args) -> &'static char {
        use std::sync::OnceLock;
        static PARSED: OnceLock<char> = OnceLock::new();
        PARSED.get_or_init(|| {
            let parsed = facet::ParsedArgs::parse(args).expect("failed to parse args::short");
            // For `short = 'j'`, there's one named arg with key "short"...
            // wait no, the `= 'j'` comes as positional after the `short` is stripped
            // Actually the tokens are just `= 'j'` since `short` is the attr name

            // Skip the '=' punct, get the char literal
            match &args[..] {
                [facet::Token::Punct { ch: '=', .. }, facet::Token::Literal { kind: facet::LiteralKind::Char, text, .. }] => {
                    text.trim_matches('\'').chars().next().expect("empty char literal")
                }
                _ => panic!("args::short expects `= 'c'` syntax"),
            }
        })
    },
}
```

### Example: Parsing `orm::index(unique, name = "idx_email")`

```rust
facet::define_extension_attrs! {
    /// Defines an index on this field.
    ///
    /// # Example
    /// ```
    /// #[facet(orm::index(unique, name = "idx_email"))]
    /// email: String,
    /// ```
    index(args) -> &'static IndexConfig {
        use std::sync::OnceLock;
        static PARSED: OnceLock<IndexConfig> = OnceLock::new();
        PARSED.get_or_init(|| {
            let parsed = facet::ParsedArgs::parse(args).expect("failed to parse orm::index");

            let unique = parsed.positional.iter().any(|v| v.as_str() == Some("unique"));
            let name = parsed.named.get("name").and_then(|v| v.as_str()).map(String::from);

            IndexConfig { unique, name }
        })
    },
}

struct IndexConfig {
    unique: bool,
    name: Option<String>,
}
```

### Retrieving Parsed Data at Runtime

```rust
// In the ORM crate
fn process_field(field: &Field) {
    if let Some(ext) = field.get_extension_attr("orm", "index") {
        if let Some(config) = ext.get_as::<IndexConfig>() {
            if config.unique {
                // Create unique index
            }
            if let Some(name) = &config.name {
                // Use custom index name
            }
        }
    }
}
```

## Runtime Usage

Extension crates query attributes at runtime:

```rust
// In facet-kdl
fn process_field(field: &Field) {
    if field.has_extension_attr("kdl", "child") {
        // This field is a KDL child
    }

    // For attributes with parsed data:
    if let Some(ext) = field.get_extension_attr("args", "short") {
        if let Some(ch) = ext.get_as::<char>() {
            // ch is 'j'
        }
    }
}
```

## Summary of Changes Required

1. **facet-core**: Remove `args` field from `ExtensionAttr`
2. **facet-core**: Add `define_extension_attrs!` macro
3. **facet-macros-emit**: Update `emit_extension_attr` to call `{ns}::attrs::{key}(&[...tokens...])`
4. **facet-kdl**: Use `define_extension_attrs!` to define KDL attributes
5. **facet-args** (if exists): Same
6. **Update all tests**
