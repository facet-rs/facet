# Extension Attributes Design

Extension attributes allow third-party crates (like `facet-kdl`, `facet-args`) to define custom attributes that users can apply to their types via `#[facet(ns::attr)]` syntax.

---

# Part 1: User Perspective

## Basic Syntax

Extension attributes use a namespaced syntax to avoid conflicts:

```rust
#[derive(Facet)]
struct Config {
    #[facet(kdl::child)]
    server: Server,

    #[facet(kdl::property)]
    name: String,
}
```

The namespace (`kdl`) comes from how you import the crate:

```rust
use facet_kdl as kdl;  // Now kdl:: attributes work
```

## Multiple Attributes

You can apply multiple extension attributes to the same field:

```rust
#[derive(Facet)]
struct Args {
    #[facet(args::named, args::short = 'j')]
    concurrency: usize,
}
```

## Attribute Syntax and Parsing Rules

Multiple attributes are separated by commas. Each attribute is parsed **until the next comma at the top level** (commas inside balanced delimiters don't count as separators).

```rust
#[facet(one, two, three = blah, four(a, b, c), five)]
//      ^^^  ^^^  ^^^^^^^^^^^^  ^^^^^^^^^^^^^  ^^^^
//       1    2        3              4          5
```

### Parsing Rules

1. **Simple attribute**: Just a name
   ```rust
   #[facet(kdl::child)]
   ```

2. **Attribute with `=` value**: Everything after `=` until the next top-level comma
   ```rust
   #[facet(args::short = 'v')]
   //      ^^^^^^^^^^^^^^^^^ attr name is "short", args are "= 'v'"

   #[facet(rename = "foo_bar")]
   //      ^^^^^^^^^^^^^^^^^^^ attr name is "rename", args are "= \"foo_bar\""
   ```

3. **Attribute with parenthesized arguments**: The parens and contents are the args
   ```rust
   #[facet(orm::index(unique, name = "idx"))]
   //      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ attr name is "index"
   //                 ^^^^^^^^^^^^^^^^^^^^^ args (inside parens)
   ```

4. **Balanced delimiters**: Commas inside `()`, `[]`, or `{}` don't split attributes
   ```rust
   #[facet(thing(a, b, c), other)]
   //      ^^^^^^^^^^^^^^  ^^^^^ two attributes, not four
   ```

### What Gets Passed to the Attribute Function

The attribute function receives everything **after** the attribute name as `&[Token]`:

| Attribute syntax | `ns` | `key` | `args` tokens |
|------------------|------|-------|---------------|
| `kdl::child` | `"kdl"` | `"child"` | `[]` (empty) |
| `args::short = 'v'` | `"args"` | `"short"` | `[Punct('='), Literal('v')]` |
| `orm::index(unique)` | `"orm"` | `"index"` | `[Group(Paren, [Ident("unique")])]` |

## Attributes with Arguments

Some attributes accept arguments:

```rust
// Simple value
#[facet(args::short = 'v')]
verbose: bool,

// Parenthesized arguments
#[facet(orm::index(unique, name = "idx_email"))]
email: String,
```

## Error Messages

When you mistype an attribute name, you get a helpful compile-time error:

```
error[E0277]: `chld` is not a recognized KDL attribute
  --> src/lib.rs:5:18
   |
5  |     #[facet(kdl::chld)]
   |                  ^^^^ unknown attribute
   |
   = help: valid attributes are: `child`, `children`, `argument`, `property`, ...
```

The error points directly at your typo, not at some internal macro expansion.

## Finding Available Attributes

Each extension crate documents its attributes in the `attrs` module. Check:

- `facet_kdl::attrs` - KDL serialization attributes
- `facet_args::attrs` - CLI argument attributes
- etc.

---

# Part 2: Crate Author Guide

## Defining Attributes

Use the `define_extension_attrs!` macro to define your crate's attributes:

```rust
// In your crate's lib.rs

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
}
```

### Attributes with Custom Parsing

For attributes that need to parse their arguments:

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

The function receives `&[facet::Token]` and returns `&'static YourType`. The macro wraps this to return `&'static dyn Any`.

### Complex Argument Parsing

For attributes like `#[facet(orm::index(unique, name = "idx"))]`:

```rust
facet::define_extension_attrs! {
    /// Defines an index on this field.
    index(args) -> &'static IndexConfig {
        use std::sync::OnceLock;
        static PARSED: OnceLock<IndexConfig> = OnceLock::new();
        PARSED.get_or_init(|| {
            let parsed = facet::ParsedArgs::parse(args).expect("failed to parse");

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

### Error Reporting with Spans

Tokens include `TokenSpan` with line/column info for error reporting. The file path is captured separately using `file!()` only when an error occurs (to avoid bloating every token with a file path string).

**How spans are captured:**

1. At macro expansion time: extract `line` and `column` from proc_macro2 spans (as integer literals)
2. In generated code: tokens carry just line/column via `TokenSpan::at(line, col)`
3. On error: use `file!()` which expands to the source file path

**Generated code pattern:**

```rust
fn __ext_get() -> ::facet::AnyStaticRef {
    use ::facet::{Token::*, LiteralKind::*, TokenSpan};
    args::attrs::short(&[
        Punct { ch: '=', joint: false, span: TokenSpan::at(42, 18) },
        Literal { kind: Char, text: "'j'", span: TokenSpan::at(42, 22) },
    ])
    .unwrap_or_else(|e| panic!("{}:{}:{}: {}", file!(), e.line, e.column, e.message))
}
```

`file!()` only appears once and is only evaluated on the error path. This keeps the generated code compact while still providing accurate error locations.

**Error output:**
```
src/main.rs:42:18: args::short expects `= 'c'` syntax
```

Note: We don't use miette or other fancy diagnostics here to avoid potential circular dependencies (miette might use facet!).

### Attribute Function Return Type

Attribute functions that parse arguments should return `Result`:

```rust
// In facet-core
pub struct AttrParseError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

// Attribute function signature for parsing attrs
pub fn short(args: &[Token]) -> Result<AnyStaticRef, AttrParseError> {
    // ...
}

// Marker attrs can still just return AnyStaticRef directly
pub fn child(_args: &[Token]) -> AnyStaticRef {
    static __UNIT: () = ();
    &__UNIT
}
```

### Parsing Helper: `ParseResult` and `expect_*` methods

For better ergonomics, `facet-core` could provide parsing helpers (future enhancement):

```rust
// Hypothetical API
let mut parser = facet::AttrParser::new(args);
parser.expect_punct('=')?;
let ch = parser.expect_char_literal()?;
```

For now, use `ParsedArgs` and manual matching with spans for errors.

### Built-in Parsing Helpers

`facet-core` provides `ParsedArgs` for common patterns:

```rust
pub struct ParsedArgs {
    /// Positional arguments (in order)
    pub positional: Vec<TokenValue>,
    /// Named arguments (key = value pairs)
    pub named: BTreeMap<String, TokenValue>,
}

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

## Querying Attributes at Runtime

```rust
fn process_field(field: &Field) {
    // Check if an attribute exists
    if field.has_extension_attr("kdl", "child") {
        // This field is a KDL child
    }

    // Get parsed data from an attribute
    if let Some(ext) = field.get_extension_attr("args", "short") {
        if let Some(ch) = ext.get_as::<char>() {
            println!("Short flag: -{}", ch);
        }
    }
}
```

### `get_as<T>()` and `must_get_as<T>()`

`get_as::<T>()` uses `downcast_ref()` internally, which returns `None` if the type doesn't match - **it does not panic**.

```rust
// If the attribute stores a `char` but you ask for `String`:
let result = ext.get_as::<String>();  // Returns None, doesn't panic

// If the attribute stores a `char` and you ask for `char`:
let result = ext.get_as::<char>();    // Returns Some(&'j')
```

`must_get_as::<T>()` panics if the type doesn't match - useful when you know the type should be correct:

```rust
// Panics with a helpful message if type doesn't match
let ch = ext.must_get_as::<char>();  // Returns &'j' or panics
```

For marker attributes (no custom parsing), `get_as::<()>()` returns `Some(&())`, but typically you just use `has_extension_attr()` for those.

## What the Macro Generates

The `define_extension_attrs!` macro generates:

1. A `pub mod attrs` with a rich aggregated doc comment
2. Marker structs and functions for each attribute (all `#[doc(hidden)]`)
3. A validation trait with `#[diagnostic::on_unimplemented]`

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
/// ...etc (aggregated from all attribute doc comments)
pub mod attrs {
    #[doc(hidden)]
    pub struct child;

    #[doc(hidden)]
    pub fn child(_args: &[::facet::Token]) -> ::facet::AnyStaticRef {
        static __UNIT: () = ();
        &__UNIT
    }

    // ... etc, all #[doc(hidden)]

    // Validation machinery lives inside attrs for cleanliness
    #[doc(hidden)]
    pub struct ValidAttr<A>(core::marker::PhantomData<A>);

    #[doc(hidden)]
    #[diagnostic::on_unimplemented(
        message = "`{A}` is not a recognized KDL attribute",
        label = "unknown attribute",
        note = "valid attributes are: `child`, `children`, `argument`, `property`"
    )]
    pub trait IsValidAttr {}

    /// Validation check function - called from generated code.
    /// The trait bound triggers `on_unimplemented` for invalid attributes.
    #[doc(hidden)]
    pub fn __check_attr<A>() where ValidAttr<A>: IsValidAttr {}

    #[doc(hidden)]
    impl IsValidAttr for ValidAttr<child> {}
    #[doc(hidden)]
    impl IsValidAttr for ValidAttr<children> {}
    // ... etc
}
```

Users browsing docs see one clean `your_crate::attrs` page with all attributes documented. The internal machinery is hidden.

---

# Part 3: The Diagnostics Trick

This section explains how we achieve beautiful call-site error messages for invalid attributes.

## The Problem

When a user writes `#[facet(kdl::nonexistent)]`, we want the error to point at `nonexistent` in their code, not at some internal macro expansion.

## The Solution: Shadowing + `on_unimplemented`

The derive macro generates code that:

1. Defines a **fallback struct** with the user's span
2. Uses a **glob import** to shadow it if a valid attribute exists
3. Checks a **trait bound** that triggers `#[diagnostic::on_unimplemented]`

### Generated Code

For `#[facet(kdl::nonexistent)]`:

```rust
{
    // Fallback type - uses span from user's tokens!
    struct nonexistent;

    {
        // Glob import - NEVER fails, even if nothing matches
        // If `child` exists in attrs, it shadows the outer `nonexistent`
        // Also brings in __check_attr, ValidAttr, IsValidAttr
        use kdl::attrs::*;

        // Trait bound check - triggers on_unimplemented if invalid
        __check_attr::<nonexistent>();
    }

    // Actual ExtensionAttr code...
    fn __ext_get() -> ::facet::AnyStaticRef {
        kdl::attrs::nonexistent(&[])
    }
    ::facet::ExtensionAttr { ns: "kdl", key: "nonexistent", get: __ext_get }
}
```

### Why This Works

**For invalid attributes:**

1. User writes `#[facet(kdl::nonexistent)]`
2. Macro generates `struct nonexistent;` with the **user's span**
3. Glob import `use kdl::attrs::*;` brings in `child`, `children`, `ValidAttr`, `IsValidAttr`, etc. but NOT `nonexistent`
4. The name `nonexistent` resolves to the outer fallback struct
5. `ValidAttr<nonexistent>` does NOT implement `IsValidAttr`
6. `#[diagnostic::on_unimplemented]` kicks in with a beautiful error
7. Error points at user's code because the fallback struct has their span

**For valid attributes:**

1. User writes `#[facet(kdl::child)]`
2. Macro generates `struct child;` in outer scope
3. Glob import brings in `kdl::attrs::child` which **shadows** the outer one
4. `ValidAttr<child>` (where `child` is now `kdl::attrs::child`) DOES implement `IsValidAttr`
5. Compiles successfully

### Key Insight: Glob Imports Never Fail

The critical trick is that `use kdl::attrs::*;` **never fails**, even if the module is empty or doesn't contain the name we're looking for. It just imports whatever exists and shadows matching names.

This is different from `use kdl::attrs::nonexistent;` which would fail immediately with "not found in `attrs`".

---

# Part 4: Implementation Details

## Type Alias for Smaller Codegen

`facet-core` exports a type alias to reduce verbosity:

```rust
/// Type-erased static data returned by extension attribute getters.
pub type AnyStaticRef = &'static (dyn core::any::Any + Send + Sync);
```

This means generated code can use `::facet::AnyStaticRef` instead of the full `&'static (dyn ::core::any::Any + ::core::marker::Send + ::core::marker::Sync)`.

## ExtensionAttr Struct

The struct is simplified (no `args` field - tokens go to the getter):

```rust
pub struct ExtensionAttr {
    /// The namespace (e.g., "kdl" in `#[facet(kdl::child)]`)
    pub ns: &'static str,

    /// The key (e.g., "child" in `#[facet(kdl::child)]`)
    pub key: &'static str,

    /// Function to get the parsed/typed data
    pub get: fn() -> AnyStaticRef,
}

impl ExtensionAttr {
    /// Get the typed data, downcasting from `dyn Any`.
    /// Returns `None` if the type doesn't match.
    pub fn get_as<T: core::any::Any>(&self) -> Option<&'static T> {
        (self.get)().downcast_ref()
    }

    /// Get the typed data, panicking if the type doesn't match.
    pub fn must_get_as<T: core::any::Any>(&self) -> &'static T {
        self.get_as().unwrap_or_else(|| {
            panic!(
                "ExtensionAttr {}::{} - expected type {}, got different type",
                self.ns,
                self.key,
                core::any::type_name::<T>()
            )
        })
    }
}
```

## Derive Macro Output

For `#[facet(kdl::child)]`:

```rust
::facet::FieldAttribute::Extension({
    fn __ext_get() -> ::facet::AnyStaticRef {
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
    fn __ext_get() -> ::facet::AnyStaticRef {
        use ::facet::{Token::*, LiteralKind::*, TokenSpan};
        args::attrs::short(&[
            Punct { ch: '=', joint: false, span: TokenSpan::at(42, 18) },
            Literal { kind: Char, text: "'j'", span: TokenSpan::at(42, 22) },
        ])
        .unwrap_or_else(|e| panic!("{}:{}:{}: {}", file!(), e.line, e.column, e.message))
    }
    ::facet::ExtensionAttr {
        ns: "args",
        key: "short",
        get: __ext_get,
    }
})
```

## Summary of Changes Required

1. **facet-core**: Remove `args` field from `ExtensionAttr`
2. **facet-core**: Add `pub type AnyStaticRef = &'static (dyn core::any::Any + Send + Sync);`
3. **facet-core**: Add `TokenSpan::at(line, column)` constructor (file defaults to `"<unknown>"`)
4. **facet-core**: Add `AttrParseError { line, column, message }` for parse errors
5. **facet-core**: Add `define_extension_attrs!` macro
6. **facet-macros-emit**: Update `emit_extension_attr` to:
   - Generate the fallback struct with user's span
   - Generate the glob import + `__check_attr` call
   - Call `{ns}::attrs::{key}(&[...tokens...])` with `TokenSpan::at(line, col)` from proc_macro2 spans
   - Add `.unwrap_or_else(|e| panic!("{}:{}:{}: {}", file!(), e.line, e.column, e.message))` for Result-returning attrs
7. **facet-kdl**: Use `define_extension_attrs!` to define KDL attributes
8. **facet-args** (if exists): Same
9. **Update all tests**
