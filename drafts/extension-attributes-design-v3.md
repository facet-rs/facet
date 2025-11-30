# Extension Attributes Design v3

This document supersedes v1 and v2. It consolidates learnings from the proto-attr prototype into the final design for facet's attribute system.

---

## Overview

Extension attributes allow crates like `facet-kdl`, `facet-args`, and `facet-yaml` to define custom attributes that users apply via `#[facet(ns::attr)]` syntax.

**Key insight**: Instead of hand-writing parsers in each extension crate, we compile a **grammar DSL** into type-safe parsing infrastructure.

```rust
// Extension crate defines grammar once:
facet::define_attr_grammar! {
    pub enum Attr {
        Child,
        Property,
        Column(Column),
    }

    pub struct Column {
        pub name: Option<&'static str>,
        pub primary_key: bool,
    }
}

// User writes attributes naturally:
#[derive(Facet)]
struct User {
    #[facet(orm::column(name = "user_id", primary_key))]
    id: i64,
}

// Typos get helpful errors:
// error: unknown attribute `colum`, did you mean `column`?
```

---

## Architecture

See **[extension-attributes-diagrams.dot](extension-attributes-diagrams.dot)** for Graphviz diagrams.

Render with: `dot -Tsvg extension-attributes-diagrams.dot -o extension-attributes-diagrams.svg`

Key diagrams:
1. **Grammar Definition Time** — what `define_attr_grammar!` generates
2. **Attribute Usage Flow** — what happens when user writes `#[facet(orm::column(...))]`
3. **Error Flow** — how typos get helpful suggestions
4. **Storage Model** — how everything becomes `ExtensionAttr`
5. **Crate Dependencies** — how the pieces fit together

---

## The Grammar DSL

### Supported Variant Types

| Type | Syntax | Example |
|------|--------|---------|
| **Unit** | Just the name | `Skip` → `#[facet(ns::skip)]` |
| **Newtype** | Name with payload type | `Rename(&'static str)` → `#[facet(ns::rename = "foo")]` or `#[facet(ns::rename("foo"))]` |
| **Struct** | Name with struct reference | `Column(Column)` → `#[facet(ns::column(name = "id", primary_key))]` |

### Supported Field Types (in structs)

| Grammar Type | Rust Type | Attribute Syntax |
|--------------|-----------|------------------|
| `bool` | `bool` | `flag` or `flag = true` |
| `&'static str` | `&'static str` | `name = "value"` |
| `Option<&'static str>` | `Option<&'static str>` | `name = "value"` (optional) |
| `Option<bool>` | `Option<bool>` | `flag = true` (optional) |
| `i64` | `i64` | `min = 42` or `min = -100` |
| `Option<i64>` | `Option<i64>` | `max = 100` (optional) |
| `&'static [&'static str]` | `&'static [&'static str]` | `cols = ["a", "b"]` |
| `ident` | `&'static str` | `action = cascade` (bare ident → string) |

### Example Grammar

```rust
facet::define_attr_grammar! {
    /// ORM attributes for database mapping
    pub enum Attr {
        /// Skip this field during serialization
        Skip,

        /// Rename the field
        Rename(&'static str),

        /// Column configuration
        Column(Column),

        /// Index configuration
        Index(Index),
    }

    /// Database column configuration
    pub struct Column {
        /// Override column name
        pub name: Option<&'static str>,
        /// SQL type override
        pub sql_type: Option<&'static str>,
        /// Mark as primary key
        pub primary_key: bool,
        /// Enable auto-increment
        pub auto_increment: bool,
    }

    /// Index configuration
    pub struct Index {
        /// Custom index name
        pub name: Option<&'static str>,
        /// Columns to index
        pub columns: &'static [&'static str],
        /// Unique constraint
        pub unique: bool,
    }
}
```

---

## Error Handling

### Unknown Attribute

```rust
#[facet(orm::colum(primary_key))]  // typo: "colum" instead of "column"
```

```
error: unknown attribute `colum`, did you mean `column`?
       available attributes: skip, rename, column, index
  --> src/lib.rs:5:13
   |
5  |     #[facet(orm::colum(primary_key))]
   |                  ^^^^^
```

### Unknown Field

```rust
#[facet(orm::column(primay_key))]  // typo in field name
```

```
error: unknown field `primay_key` in `Column`
       available fields: name, sql_type, primary_key, auto_increment
       did you mean `primary_key`?
  --> src/lib.rs:5:25
   |
5  |     #[facet(orm::column(primay_key))]
   |                         ^^^^^^^^^^
```

### Wrong Syntax

```rust
#[facet(orm::rename)]  // missing required value
```

```
error: `rename` expects a string value: `rename = "name"` or `rename("name")`
  --> src/lib.rs:5:13
   |
5  |     #[facet(orm::rename)]
   |                  ^^^^^^
```

---

## What Gets Generated

When you call `define_attr_grammar!`, it generates:

### 1. Type Definitions

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Attr {
    Skip,
    Rename(&'static str),
    Column(Column),
    Index(Index),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Column {
    pub name: Option<&'static str>,
    pub sql_type: Option<&'static str>,
    pub primary_key: bool,
    pub auto_increment: bool,
}

// ... etc
```

### 2. Proc-Macro Re-exports

```rust
#[doc(hidden)]
pub use ::facet::__dispatch_attr;
#[doc(hidden)]
pub use ::facet::__build_struct_fields;
#[doc(hidden)]
pub use ::facet::__attr_error as __attr_error_proc_macro;
#[doc(hidden)]
pub use ::facet::__field_error as __field_error_proc_macro;
```

### 3. The `__parse_attr!` Macro

A declarative macro containing the "compiled grammar":

```rust
#[macro_export]
macro_rules! __parse_attr {
    ($name:ident $($rest:tt)*) => {
        $crate::__dispatch_attr!{
            @namespace { $crate }
            @enum_name { Attr }
            @variants {
                skip: unit,
                rename: newtype,
                column: rec Column { name: opt_string, primary_key: bool, ... },
                index: rec Index { name: opt_string, columns: list_string, unique: bool }
            }
            @name { $name }
            @rest { $($rest)* }
        }
    };

    () => {
        compile_error!("expected an attribute name")
    };
}
```

---

## The Derive Macro's Role

The `#[derive(Facet)]` macro is intentionally simple. When it sees `#[facet(orm::column(...))]`:

1. **Parse** the attribute to extract namespace (`orm`) and rest (`column(...)`)
2. **Generate** a call to `orm::__parse_attr!(column(...))`
3. **Done** — all parsing logic lives in the extension crate

```rust
// User writes:
#[facet(orm::column(primary_key))]
id: i64,

// Derive macro generates (simplified):
.attributes(&[
    FieldAttribute::Extension(orm::__parse_attr!(column(primary_key)))
])
```

The derive macro doesn't need to understand ORM attributes. It just forwards to the namespace's parser.

---

## Runtime Access

Parsed attributes are stored in the `Field` and can be queried:

```rust
fn process_field(field: &Field) {
    // Check for existence
    if field.has_extension_attr("orm", "skip") {
        return; // Skip this field
    }

    // Get typed data
    for attr in field.attributes {
        if let FieldAttribute::Extension(ext) = attr {
            if ext.ns == "orm" && ext.key == "column" {
                // ext.data points to the Attr value
                // ext.shape allows introspection
            }
        }
    }
}
```

---

## Implementation Status

### Already Ported (from proto-attr to facet-macros-impl)

| Component | File | Status |
|-----------|------|--------|
| Grammar compiler | `attr_grammar/make_parse_attr.rs` | ✅ Done |
| Dispatcher | `attr_grammar/dispatch_attr.rs` | ✅ Done |
| Struct field builder | `attr_grammar/build_struct_fields.rs` | ✅ Done |
| Attribute error | `attr_grammar/attr_error.rs` | ✅ Done |
| Field error | `attr_grammar/field_error.rs` | ✅ Done |
| Spanned error | `attr_grammar/spanned_error.rs` | ✅ Done |
| Public API macro | `facet/src/lib.rs` (`define_attr_grammar!`) | ✅ Done |

### Not Yet Connected

| Task | Status |
|------|--------|
| Derive macro generates `ns::__parse_attr!` calls | ❌ Not done |
| Extension crates use `define_attr_grammar!` | ❌ Not done |
| Built-in attrs use the grammar system | ❌ Not done |

---

## Migration Plan

### Phase 1: Connect the Derive Macro

Update `facet-macros-impl` to generate `namespace::__parse_attr!(...)` calls instead of the current `__attr!` dispatch.

### Phase 2: Migrate Extension Crates

Convert `facet-kdl`, `facet-args`, `facet-yaml` to use `define_attr_grammar!`:

```rust
// Before (hand-written in each crate):
#[macro_export]
macro_rules! __attr {
    (child { $($tt:tt)* }) => { $crate::__child!{ $($tt)* } };
    (property { $($tt:tt)* }) => { $crate::__property!{ $($tt)* } };
    // ... etc, ~100 lines per crate
}

// After (one grammar definition):
facet::define_attr_grammar! {
    pub enum Attr {
        Child,
        Children,
        Property,
        Argument,
        Arguments,
        NodeName,
    }
}
```

### Phase 3: Built-in Attributes

Use the grammar system for facet's built-in attributes too:

```rust
// In facet-core or facet-macros-impl
facet::define_attr_grammar! {
    pub enum BuiltinAttr {
        Sensitive,
        Skip,
        SkipSerializing,
        SkipDeserializing,
        Flatten,
        Default,
        Rename(&'static str),
        RenameAll(&'static str),
        Tag(&'static str),
        Content(&'static str),
        Untagged,
        // ... etc
    }
}
```

This would replace the hand-written `FacetInner` enum (~200 lines of unsynn grammar).

---

## Files to Delete After Migration

Once fully migrated, these can be removed:

- `proto-attr/` (entire directory — prototype served its purpose)
- Hand-written `__attr!` macros in extension crates
- `FacetInner` enum in `facet-macros-impl/src/lib.rs`
- Various `*Inner` structs for attribute parsing

---

## Storage Model

**Everything becomes `ExtensionAttr`.** This is the key insight.

### Current (to be eliminated)

```rust
pub enum ShapeAttribute {
    DenyUnknownFields,           // ← specific variant, dies
    Default,                      // ← specific variant, dies
    Transparent,                  // ← specific variant, dies
    RenameAll(&'static str),      // ← specific variant, dies
    Untagged,                     // ← specific variant, dies
    Tag(&'static str),            // ← specific variant, dies
    Content(&'static str),        // ← specific variant, dies
    Extension(ExtensionAttr),     // ← ONLY this remains
}
```

### Future

```rust
// ShapeAttribute and FieldAttribute both become just ExtensionAttr
pub type ShapeAttribute = ExtensionAttr;
pub type FieldAttribute = ExtensionAttr;

// Or we keep the enum wrapper for future extensibility:
pub enum ShapeAttribute {
    Attr(ExtensionAttr),
}
```

### Built-in Attrs as ExtensionAttr

Built-in facet attributes use empty namespace:

```rust
// #[facet(rename_all = "camelCase")]
ExtensionAttr {
    ns: "",  // empty string = built-in facet attribute
    key: "rename_all",
    data: &"camelCase" as *const (),
    shape: <&'static str>::SHAPE,
}

// #[facet(deny_unknown_fields)]
ExtensionAttr {
    ns: "",
    key: "deny_unknown_fields",
    data: &() as *const (),
    shape: <()>::SHAPE,
}

// #[facet(kdl::child)]
ExtensionAttr {
    ns: "kdl",
    key: "child",
    data: &() as *const (),
    shape: <()>::SHAPE,
}
```

### The Grammar Enum is for Parsing, Not Storage

The `Attr` enum generated by `define_attr_grammar!` is used:

1. **At parse time** — to validate syntax and produce good error messages
2. **To generate ExtensionAttr** — each variant knows its `key` and `data` type
3. **For convenience** — extension crates can match on typed enum in their code

But the **final storage** is always `ExtensionAttr` in `Shape.attributes` / `Field.attributes`.

---

## Open Questions

### 1. FieldFlags Integration

Built-in attrs like `skip`, `sensitive` map to `FieldFlags` bitflags. How to connect?

**Proposed**: Grammar annotations:

```rust
facet::define_attr_grammar! {
    pub enum Attr {
        #[flags(SENSITIVE)]
        Sensitive,

        #[flags(SKIP_SERIALIZING, SKIP_DESERIALIZING)]
        Skip,

        Rename(&'static str),  // No flags
    }
}
```

Generate a `flags()` method on the enum.

---

## Summary

| Aspect | Old (current) | New (with grammar) |
|--------|---------------|-------------------|
| Parsing logic | Hand-written per crate | Generated from grammar |
| Error messages | Basic `compile_error!` | Typo suggestions via strsim |
| Type safety | Stringly-typed | Enum + struct types |
| Boilerplate | ~100 lines per crate | ~10 lines grammar |
| Maintenance | Fix bugs in N places | Fix once in generator |
