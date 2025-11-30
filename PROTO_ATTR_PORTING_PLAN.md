# Proto-Attr → Facet Porting Plan

This document maps techniques from `proto-attr` to their destinations in `facet-core`, `facet-macros`, etc.

## Overview

Proto-attr is a **declarative attribute grammar system** that enables extension crates to define type-safe, self-documenting attribute grammars. The key insight from issue #971 is that "every extension crate must effectively re-implement a parser for its attribute syntax" - proto-attr solves this with **grammar-as-types**.

---

## Component Mapping

### 1. Grammar Compiler (Copy As-Is)

**Source**: `proto-attr/crates/proto-attr-macros/src/make_parse_attr.rs`

**What it does**:
- Parses a grammar DSL using `unsynn`
- Generates type definitions (enum + structs)
- Generates `__parse_attr!` dispatcher macro
- Generates re-exports for proc-macros

**Target**: `facet-macros/src/make_parse_attr.rs` (new file)

**Changes needed**:
- Rename references from `proto_attr_macros::*` → `facet::*` or `facet_macros::*`
- Update re-exports in generated code (line 367-381)

**Grammar DSL example**:
```rust
facet::define_attr_grammar! {
    pub enum Attr {
        /// Skip this field
        Skip,
        /// Rename to a different name
        Rename(&'static str),
        /// Database column configuration
        Column(Column),
    }

    pub struct Column {
        /// Override the database column name
        pub name: Option<&'static str>,
        /// Mark as primary key
        pub primary_key: bool,
    }
}
```

---

### 2. Unified Dispatcher

**Source**: `proto-attr/crates/proto-attr-macros/src/dispatch_attr.rs`

**What it does**:
- Routes parsed attribute names to variant handlers
- Handles 3 variant kinds:
  - **Unit**: `skip` → `Attr::Skip`
  - **Newtype**: `rename("name")` → `Attr::Rename("name")`
  - **Struct**: `column(name = "id", primary_key)` → `Attr::Column(Column { ... })`
- Generates helpful error messages with suggestions

**Target**: `facet-macros/src/dispatch_attr.rs` (new file)

**Integration point**:
- Currently: `facet-macros-emit/src/extension.rs:20-65` calls `__ext!` proc-macro
- New: Extension crates call their own `__parse_attr!` which routes to `__dispatch_attr!`

**Key types** (lines 140-152):
```rust
enum FieldKind {
    Bool,           // bool
    String,         // &'static str
    OptString,      // Option<&'static str>
    OptBool,        // Option<bool>
    I64,            // i64
    OptI64,         // Option<i64>
    ListString,     // &'static [&'static str]
    ListI64,        // &'static [i64]
    Ident,          // bare identifiers as &'static str
}
```

---

### 3. Struct Field Builder

**Source**: `proto-attr/crates/proto-attr-macros/src/build_struct_fields.rs`

**What it does**:
- Parses struct field assignments in one shot (avoids recursive macro_rules)
- Validates field names against known fields
- Type-checks values (string → String, bool → Bool, i64 → I64, lists, idents)
- Extracts doc comments for error help text
- Generates struct initialization code

**Target**: `facet-macros/src/build_struct_fields.rs` (new file)

**Input format**:
```
@krate { $crate }
@enum_name { Attr }
@variant_name { Column }
@struct_name { Column }
@fields { name: opt_string, primary_key: bool }
@input { name = "id", primary_key }
```

**Output**:
```rust
orm::Attr::Column(orm::Column {
    name: Some("id"),
    primary_key: true,
    ...defaults...
})
```

---

### 4. Error Handling System

**Source**: `proto-attr/crates/proto-attr-macros/src/`
- `attr_error.rs` - Unknown attribute with suggestions
- `field_error.rs` - Unknown field with suggestions
- `spanned_error.rs` - Generic spanned error

**Target**: `facet-macros/src/errors/` (new directory)

**Key techniques**:
1. **Levenshtein distance** for typo suggestions (uses `strsim` crate)
2. **Span preservation** through macro expansion
3. **Doc comment extraction** for contextual help
4. **Nightly diagnostics** with stable fallback

**Example error flow** (dispatch_attr.rs:428-449):
```rust
let suggestion = find_closest(&attr_name_str, &known_names);
let msg = if let Some(s) = suggestion {
    format!("unknown attribute `{}`; did you mean `{}`?", attr_name_str, s)
} else {
    format!("unknown attribute `{}`; expected one of: {}", attr_name_str, known_names.join(", "))
};
```

---

### 5. Core Trait

**Source**: `proto-attr/crates/proto-attr-core/src/lib.rs`

```rust
pub trait AttrEnum: 'static + Sized {
    const NAME: &'static str;
}
```

**Target**: `facet-core/src/types/attr.rs` (new file) or add to existing types

**Purpose**: Allows generic code to work with any attribute grammar. Extension crates implement this on their generated `Attr` enum.

---

### 6. Public API Macro

**Source**: `proto-attr/crates/proto-attr/src/lib.rs`

```rust
#[macro_export]
macro_rules! define_attr_grammar {
    ($($grammar:tt)*) => {
        $crate::__make_parse_attr! { $($grammar)* }
    };
}
```

**Target**: `facet/src/lib.rs` or `facet-attr/src/lib.rs` (new crate?)

**Exports needed**:
- `define_attr_grammar!` - User-facing macro
- `AttrEnum` trait - For generic constraints
- Re-exports from `facet-macros` for proc-macro implementations

---

## Integration with Existing Facet Architecture

### Current Extension Flow (facet-macros-emit/extension.rs:20-65)

```
#[facet(orm::column(...))]
    ↓
emit_extension_attr_for_field()
    ↓
::facet::__ext!(orm::column { field : Type | args })
    ↓
Extension crate's __attr! macro (ad-hoc parsing)
```

### New Extension Flow (with proto-attr)

```
#[facet(orm::column(...))]
    ↓
emit_extension_attr_for_field() (unchanged)
    ↓
::facet::__ext!(orm::column { field : Type | args })
    ↓
orm::__parse_attr!(column(...))  // Generated by define_attr_grammar!
    ↓
facet::__dispatch_attr!(...)     // Centralized dispatcher
    ↓
facet::__build_struct_fields!()  // For struct variants
    ↓
orm::Attr::Column(Column { ... })
```

---

## File-by-File Porting Checklist

### Files Created/Updated

| Source (proto-attr) | Destination (facet) | Status |
|---------------------|---------------------|--------|
| `proto-attr-core/src/lib.rs` | `facet-core/src/types/attr_grammar.rs` | ✅ Done |
| `proto-attr-macros/src/make_parse_attr.rs` | `facet-macros-impl/src/attr_grammar/make_parse_attr.rs` | ✅ Done |
| `proto-attr-macros/src/dispatch_attr.rs` | `facet-macros-impl/src/attr_grammar/dispatch_attr.rs` | ✅ Done |
| `proto-attr-macros/src/build_struct_fields.rs` | `facet-macros-impl/src/attr_grammar/build_struct_fields.rs` | ✅ Done |
| `proto-attr-macros/src/attr_error.rs` | `facet-macros-impl/src/attr_grammar/attr_error.rs` | ✅ Done |
| `proto-attr-macros/src/field_error.rs` | `facet-macros-impl/src/attr_grammar/field_error.rs` | ✅ Done |
| `proto-attr-macros/src/spanned_error.rs` | `facet-macros-impl/src/attr_grammar/spanned_error.rs` | ✅ Done |
| `proto-attr/src/lib.rs` | `facet/src/lib.rs` (`define_attr_grammar!`) | ✅ Done |

### Crate Structure (Simplified)

The macro crates have been consolidated:
- `facet-macros-parse` + `facet-macros-emit` → `facet-macros-impl` (merged)
- `facet-macros` remains as the thin proc-macro wrapper

### Proc-Macros Registered

| Proc-macro | Location | Status |
|------------|----------|--------|
| `__make_parse_attr` | `facet-macros/src/lib.rs` | ✅ Done |
| `__dispatch_attr` | `facet-macros/src/lib.rs` | ✅ Done |
| `__build_struct_fields` | `facet-macros/src/lib.rs` | ✅ Done |
| `__attr_error` | `facet-macros/src/lib.rs` | ✅ Done |
| `__field_error` | `facet-macros/src/lib.rs` | ✅ Done |
| `__spanned_error` | `facet-macros/src/lib.rs` | ✅ Done |

---

## Supported Field Types (Extended)

Proto-attr supports these field types in struct variants:

| Type | Syntax | Generated Type |
|------|--------|----------------|
| `bool` | `flag` or `flag = true` | `bool` |
| `string` | `name = "value"` | `&'static str` |
| `opt_string` | `name = "value"` (optional) | `Option<&'static str>` |
| `opt_bool` | `flag = true` (optional) | `Option<bool>` |
| `i64` | `min = 42` or `min = -100` | `i64` |
| `opt_i64` | `max = 100` (optional) | `Option<i64>` |
| `list_string` | `cols = ["a", "b"]` | `&'static [&'static str]` |
| `list_i64` | `vals = [1, 2, 3]` | `&'static [i64]` |
| `ident` | `action = cascade` | `&'static str` (bare ident) |

---

## Error Message Examples

### Unknown Attribute (with typo suggestion)
```
error: unknown attribute `colum`; did you mean `column`?
 --> src/lib.rs:5:9
  |
5 | #[facet(orm::colum(...))]
  |         ^^^^^^^^^^
```

### Unknown Field (with suggestion)
```
error: unknown field `primay_key` in `Column`
  |
  = help: expected one of: name, primary_key, nullable, auto_increment
  = note: did you mean `primary_key`?
```

### Wrong Type
```
error: `name` expects a string literal: `name = "value"`
  |
5 |     name = users
  |            ^^^^^ help: wrap in quotes: `name = "users"`
```

---

## Multi-Attribute Support

Proto-attr supports multiple attributes on a single item:

```rust
#[derive(Facet)]
struct User {
    #[facet(orm::column(name = "user_id", primary_key))]
    #[facet(orm::index(unique))]
    id: i64,
}
```

This is already supported by facet's current architecture via multiple `#[facet(...)]` attributes.

---

## Implementation Order

1. **Phase 1: Core Infrastructure**
   - [ ] Copy `AttrEnum` trait to `facet-core`
   - [ ] Copy grammar compiler (`make_parse_attr.rs`) to `facet-macros`
   - [ ] Add `unsynn` and `strsim` dependencies

2. **Phase 2: Dispatcher & Field Builder**
   - [ ] Copy `dispatch_attr.rs` to `facet-macros`
   - [ ] Copy `build_struct_fields.rs` to `facet-macros`
   - [ ] Register new proc-macros in `facet-macros/src/lib.rs`

3. **Phase 3: Error Handling**
   - [ ] Copy error handling files to `facet-macros/src/errors/`
   - [ ] Wire up error macros

4. **Phase 4: Public API**
   - [ ] Add `define_attr_grammar!` to `facet`
   - [ ] Add re-exports
   - [ ] Update documentation

5. **Phase 5: Migration**
   - [ ] Convert existing extension crates to use new grammar system
   - [ ] Update tests
   - [ ] Deprecate old extension patterns

---

## Dependencies to Add

```toml
# facet-macros/Cargo.toml
[dependencies]
unsynn = "0.3"      # Grammar parsing
strsim = "0.11"     # Levenshtein distance for suggestions
```

---

## Benefits After Porting

1. **Consistency**: All extension crates use the same parsing infrastructure
2. **Error Quality**: Unified, helpful error messages with typo suggestions
3. **Type Safety**: Grammar defined declaratively, parsed uniformly
4. **Doc Integration**: Doc comments flow through to error help text
5. **Maintainability**: One parser to maintain instead of N per extension
6. **Span Preservation**: Errors point to exact source locations
