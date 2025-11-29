# Extension Attributes Design v2

Extension attributes allow third-party crates (like `facet-kdl`, `facet-args`) to define custom attributes that users can apply to their types via `#[facet(ns::attr)]` syntax.

This design replaces the previous approach with a much simpler one: **expand to macro invocations**.

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
error: unknown kdl attribute `chld`. expected one of: child, children, property, argument
  --> src/lib.rs:5:18
   |
5  |     #[facet(kdl::chld)]
   |                  ^^^^
```

The error points directly at your typo and suggests valid alternatives.

## Finding Available Attributes

Each extension crate documents its attributes. Check the crate's documentation for available options.

---

# Part 2: Crate Author Guide

## How It Works

When a user writes `#[facet(kdl::child)]`, the derive macro expands it to a macro invocation:

```rust
kdl::__attr!(child { server: Server })
```

Your crate provides:
1. A **dispatcher macro** (`__attr!`) that routes to individual attribute macros
2. **Attribute macros** that return `ExtensionAttr` values

## Defining the Dispatcher

The dispatcher macro routes attribute names to their implementations and provides error messages for unknown attributes:

```rust
// In your crate's lib.rs

#[macro_export]
macro_rules! __attr {
    (child { $($tt:tt)* }) => { $crate::__child!{ $($tt)* } };
    (children { $($tt:tt)* }) => { $crate::__children!{ $($tt)* } };
    (property { $($tt:tt)* }) => { $crate::__property!{ $($tt)* } };
    (argument { $($tt:tt)* }) => { $crate::__argument!{ $($tt)* } };

    ($unknown:ident $($tt:tt)*) => {
        ::core::compile_error!(::core::concat!(
            "unknown kdl attribute `", ::core::stringify!($unknown), "`. ",
            "expected one of: child, children, property, argument"
        ))
    };
}
```

## Defining Attribute Macros

Each attribute macro receives field info and returns an `ExtensionAttr`:

### Marker Attributes (no arguments)

```rust
#[macro_export]
#[doc(hidden)]
macro_rules! __child {
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "child", &__UNIT)
    }};
}
```

### Attributes with Arguments

Arguments come after a `|` separator:

```rust
#[macro_export]
#[doc(hidden)]
macro_rules! __short {
    // With argument: #[facet(args::short = 'v')]
    { $field:ident : $ty:ty | = $ch:literal } => {{
        static __VAL: char = $ch;
        ::facet::ExtensionAttr::new("args", "short", &__VAL)
    }};

    // Error case: missing argument
    { $field:ident : $ty:ty } => {
        ::core::compile_error!("args::short requires a character, e.g., args::short = 'v'")
    };
}
```

### Complex Arguments

For attributes like `#[facet(orm::index(unique, name = "idx"))]`:

```rust
#[macro_export]
#[doc(hidden)]
macro_rules! __index {
    { $field:ident : $ty:ty | ( $($args:tt)* ) } => {{
        // Parse the arguments however you like
        static __CONFIG: $crate::IndexConfig = $crate::__parse_index_args!($($args)*);
        ::facet::ExtensionAttr::new("orm", "index", &__CONFIG)
    }};
}
```

## What the Macro Receives

The derive macro passes field information in this format:

```
field_name : FieldType
field_name : FieldType | args...
```

| User writes | Macro receives |
|-------------|----------------|
| `#[facet(kdl::child)]` | `child { server: Server }` |
| `#[facet(args::short = 'v')]` | `short { verbose: bool \| = 'v' }` |
| `#[facet(orm::index(unique))]` | `index { email: String \| (unique) }` |

The `|` separator only appears when there are arguments after the attribute name.

## The ExtensionAttr Struct

```rust
pub struct ExtensionAttr {
    /// The namespace (e.g., "kdl" in `#[facet(kdl::child)]`)
    pub ns: &'static str,

    /// The key (e.g., "child" in `#[facet(kdl::child)]`)
    pub key: &'static str,

    /// Pointer to the static data
    pub data: *const (),

    /// Shape of the data (for introspection)
    pub shape: &'static Shape,
}

impl ExtensionAttr {
    /// Create a new extension attribute from static data.
    /// The type must implement `Facet` for introspection support.
    pub const fn new<T: Facet>(ns: &'static str, key: &'static str, data: &'static T) -> Self {
        Self {
            ns,
            key,
            data: data as *const T as *const (),
            shape: T::SHAPE,
        }
    }

    /// Get the typed data if the type matches.
    pub fn get_as<T: Facet>(&self) -> Option<&'static T> {
        (self.shape == T::SHAPE).then(|| unsafe { &*(self.data as *const T) })
    }

    /// Get a Peek for introspection (debug printing, etc.)
    pub fn peek(&self) -> Peek<'static> {
        unsafe { Peek::unchecked(self.data, self.shape) }
    }
}
```

Because we store the `Shape`, you can:
- Debug-print any extension attribute's value via `facet-pretty`
- Serialize extension attribute data
- Diff values between structs
- Full introspection without knowing the concrete type

## Querying Attributes at Runtime

```rust
fn process_field(field: &Field) {
    // Check if an attribute exists
    if field.has_extension_attr("kdl", "child") {
        // This field is a KDL child
    }

    // Get typed data from an attribute
    if let Some(ext) = field.get_extension_attr("args", "short") {
        if let Some(ch) = ext.get_as::<char>() {
            println!("Short flag: -{}", ch);
        }
    }

    // Debug print any attribute's value
    if let Some(ext) = field.get_extension_attr("orm", "index") {
        println!("Index config: {:?}", facet_pretty::to_string(ext.peek()));
    }
}
```

## Helper Macro for Crate Authors

To reduce boilerplate, `facet` provides a helper macro:

```rust
facet::extension_crate! {
    // Crate name for error messages
    name: "kdl",

    // List of attributes (dispatcher is auto-generated)
    attrs: [child, children, property, argument],
}
```

This generates the `__attr!` dispatcher macro. You still write the individual attribute macros yourself, giving you full control over parsing and error messages.

---

# Part 3: Implementation Details

## Derive Macro Output

For `#[facet(kdl::child)]` on a field `server: Server`:

```rust
::facet::FieldAttribute::Extension(
    kdl::__attr!(child { server: Server })
)
```

For `#[facet(args::short = 'j')]` on a field `verbose: bool`:

```rust
::facet::FieldAttribute::Extension(
    args::__attr!(short { verbose: bool | = 'j' })
)
```

That's it. The derive macro just emits a macro call. All the logic lives in the extension crate's macros.

## Why This Design

### Simplicity

The previous design required:
- Token types (`Token`, `TokenSpan`, `LiteralKind`) in facet-core
- A proc macro (`define_extension_attrs!`) to generate validation machinery
- A shadowing trick with glob imports for compile-time validation
- `#[diagnostic::on_unimplemented]` for error messages
- `AnyStaticRef` and runtime downcasting

This design requires:
- A dispatcher macro per extension crate
- Individual attribute macros
- That's it

### Better Error Messages

Extension crate authors control the error messages completely. They can:
- Use `compile_error!` with custom messages
- Suggest valid alternatives
- Explain expected syntax
- Point at the exact problem

### Full Flexibility

Macros can do anything:
- Declarative macros for simple cases
- Proc macros for complex parsing
- Arbitrary compile-time validation
- Custom data structures

### Full Introspection

By storing `Shape` alongside the data pointer:
- `facet-pretty` can print any extension attribute value
- No need to know the concrete type at the query site
- Extension attribute data is a first-class facet citizen

## Migration from v1

1. Remove `define_extension_attrs!` usage
2. Create a `__attr!` dispatcher macro
3. Convert attribute functions to `__attr_name!` macros
4. Update return type from `AnyStaticRef` to `ExtensionAttr::new(...)`

The query API (`has_extension_attr`, `get_extension_attr`, `get_as`) remains the same.

---

# Summary

| Aspect | Old Design | New Design |
|--------|-----------|------------|
| Validation | Trait bounds + `on_unimplemented` | Dispatcher macro with `compile_error!` |
| Error messages | Generated by proc macro | Written by crate author |
| Token handling | facet-core Token types | Standard macro_rules patterns |
| Data storage | `fn() -> &'static dyn Any` | `*const () + &'static Shape` |
| Introspection | Downcast only | Full Shape-based reflection |
| Complexity | High | Low |
