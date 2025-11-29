# Attribute Grammar Prototype Plan

## Goal

Validate the core mechanics of the declarative attribute grammar system in isolation before integrating with facet.

**Core hypothesis**: A `macro_rules!` can capture grammar tokens, compile them into pattern-matching arms, and fall back to a proc-macro for error diagnostics.

---

## Crate Structure

```
proto-attr/
├── crates/
│   ├── proto-attr-core/     # Core types
│   ├── proto-attr-macros/   # Proc-macros
│   ├── proto-attr/          # Main crate, re-exports
│   ├── proto-ext/           # Example extension ("orm")
│   └── proto-user/          # Test the full flow
```

### Dependency Graph

```
proto-attr-core  ◄─────────────────────────────────┐
       ▲                                           │
       │ runtime                                   │
       │                                           │
proto-attr-macros (proc-macro)                     │
       ▲                                           │
       │ proc-macro                                │
       │                                           │
proto-attr ────────────────────────────────────────┤
       ▲                                           │
       │ runtime                                   │
       │                                           │
proto-ext ─────────────────────────────────────────┤
       ▲                                           │
       │ runtime                                   │
       │                                           │
proto-user ────────────────────────────────────────┘
```

---

## Test Grammar

Minimal grammar covering the three variant types:

```rust
// In proto-ext
proto_attr::define_attr_grammar! {
    pub enum Attr {
        /// Skip this field
        Skip,
        /// Rename to a different name
        Rename(&'static str),
        /// Column mapping
        Column(Column),
    }

    pub struct Column {
        /// Column name override
        pub name: Option<&'static str>,
        /// Is this a primary key?
        pub primary_key: bool,
    }
}
```

Usage in proto-user:

```rust
#[proto_attr::attr(proto_ext::skip)]
#[proto_attr::attr(proto_ext::rename("foo"))]
#[proto_attr::attr(proto_ext::column(name = "user_id", primary_key))]
struct Example;
```

---

## Macro Flow

### Step 1: `define_attr_grammar!`

A `macro_rules!` in proto-attr that forwards to the proc-macro:

```rust
#[macro_export]
macro_rules! define_attr_grammar {
    ($($grammar:tt)*) => {
        $crate::__make_parse_attr! { $($grammar)* }
    };
}
```

### Step 2: `__make_parse_attr!` (proc-macro)

Receives the grammar tokens. Emits:

1. **The types** (`pub enum Attr`, `pub struct Column`)
2. **A compiled `__parse_attr!` macro** with pattern-matching arms

```rust
// Generated output (conceptual)

pub enum Attr {
    Skip,
    Rename(&'static str),
    Column(Column),
}

pub struct Column {
    pub name: Option<&'static str>,
    pub primary_key: bool,
}

#[macro_export]
macro_rules! __parse_attr {
    // Unit variant
    (skip) => { $crate::Attr::Skip };

    // Newtype variant - parens style
    (rename($lit:literal)) => { $crate::Attr::Rename($lit) };
    // Newtype variant - equals style
    (rename = $lit:literal) => { $crate::Attr::Rename($lit) };

    // Struct variant - delegate to field parser
    (column( $($fields:tt)* )) => {
        $crate::__parse_column!{ @fields {} $($fields)* }
    };

    // Error fallback - unknown attribute
    ($name:ident $($rest:tt)*) => {
        $crate::__attr_error!{
            @known_attrs { skip, rename, column }
            @got_name { $name }
            @got_rest { $($rest)* }
        }
    };
}

#[macro_export]
macro_rules! __parse_column {
    // Terminal: all fields consumed, build the struct
    (@fields { $($parsed:tt)* } $(,)?) => {
        $crate::Attr::Column($crate::Column {
            name: None,
            primary_key: false,
            $($parsed)*
        })
    };

    // Field: name = "..."
    (@fields { $($parsed:tt)* } name = $lit:literal $($rest:tt)*) => {
        $crate::__parse_column!{ @fields { $($parsed)* name: Some($lit), } $($rest)* }
    };

    // Field: primary_key (bool flag)
    (@fields { $($parsed:tt)* } primary_key $($rest:tt)*) => {
        $crate::__parse_column!{ @fields { $($parsed)* primary_key: true, } $($rest)* }
    };

    // Field: primary_key = true/false
    (@fields { $($parsed:tt)* } primary_key = $val:literal $($rest:tt)*) => {
        $crate::__parse_column!{ @fields { $($parsed)* primary_key: $val, } $($rest)* }
    };

    // Skip comma
    (@fields { $($parsed:tt)* } , $($rest:tt)*) => {
        $crate::__parse_column!{ @fields { $($parsed)* } $($rest)* }
    };

    // Error: unknown field
    (@fields { $($parsed:tt)* } $name:ident $($rest:tt)*) => {
        $crate::__field_error!{
            @struct_name { Column }
            @known_fields { name, primary_key }
            @got_name { $name }
            @got_rest { $($rest)* }
        }
    };
}
```

### Step 3: Error Proc-Macros

`__attr_error!` and `__field_error!` are proc-macros that:

1. Extract the identifier span from `@got_name`
2. Compare against known names using strsim
3. Emit `compile_error!` with suggestions

```rust
// __attr_error! receives:
// @known_attrs { skip, rename, column }
// @got_name { colum }
// @got_rest { (...) }

// Emits:
compile_error!("unknown attribute `colum`, did you mean `column`?")
// With span pointing to `colum` in the source
```

---

## Implementation Phases

### Phase 1: Manual Prototype

Write the generated code by hand to validate the macro_rules patterns work:

- [ ] Hand-write `Attr` enum and `Column` struct
- [ ] Hand-write `__parse_attr!` macro with all arms
- [ ] Hand-write `__parse_column!` macro
- [ ] Test happy paths compile and produce correct values
- [ ] Test error cases (unknown attr, unknown field)

**Success criteria**: The hand-written macros work as expected.

### Phase 2: Error Proc-Macros

Implement the error-reporting proc-macros:

- [ ] `__attr_error!` - unknown attribute with suggestions
- [ ] `__field_error!` - unknown field with suggestions
- [ ] Span preservation for accurate error locations
- [ ] strsim integration for "did you mean?"

**Success criteria**: Errors point to the right place with helpful suggestions.

### Phase 3: Grammar Parser

Parse the grammar DSL in `__make_parse_attr!`:

- [ ] Parse enum definition (name, variants)
- [ ] Parse struct definitions (name, fields)
- [ ] Parse field types (bool, &'static str, Option<T>)
- [ ] Handle doc comments

**Success criteria**: Can parse the test grammar into an AST.

### Phase 4: Code Generation

Generate the macro_rules from the parsed grammar:

- [ ] Generate type definitions
- [ ] Generate `__parse_attr!` with variant arms
- [ ] Generate `__parse_<struct>!` for each struct variant
- [ ] Generate error fallback arms with known names

**Success criteria**: `define_attr_grammar!` produces working macros.

### Phase 5: Cross-Crate Validation

Test the full flow across crate boundaries:

- [ ] Define grammar in proto-ext
- [ ] Use attributes in proto-user
- [ ] Verify types are accessible
- [ ] Verify errors work cross-crate

**Success criteria**: The system works across crates exactly as designed.

---

## Open Questions to Resolve

### 1. Struct Field Parsing Order

If someone writes `column(primary_key, name = "id")` vs `column(name = "id", primary_key)`, both should work. The recursive macro approach handles this but we need to track which fields are already set to catch duplicates.

**Proposed**: Track parsed fields in the accumulator, emit error on duplicate.

### 2. Nested Structs

For `column(index(unique))` where `index` is itself a struct—can we handle this with pure macro_rules or do we need proc-macro help?

**Proposed**: Defer to Phase 2 of the main implementation. Prototype focuses on flat structs.

### 3. Type Validation

How do we validate that a literal matches the expected type? `name = 42` should error because `name` expects `&'static str`.

**Proposed**: macro_rules can use `:literal` but can't distinguish string vs number. Fall back to proc-macro for type errors, or let rustc catch it when the struct is constructed.

### 4. Enum-Typed Fields

For `on_delete = cascade` where `cascade` is an enum variant—macro_rules would need to enumerate all valid variants.

**Proposed**: Generate arms for each enum variant. `on_delete = cascade` → `on_delete: OnDelete::Cascade`.

---

## Testing Strategy

### Compile-Pass Tests

```rust
// tests/pass/unit_variant.rs
let attr = proto_ext::__parse_attr!(skip);
assert!(matches!(attr, proto_ext::Attr::Skip));
```

### Compile-Fail Tests (trybuild)

```rust
// tests/fail/unknown_attr.rs
let attr = proto_ext::__parse_attr!(skp);

// tests/fail/unknown_attr.stderr
error: unknown attribute `skp`, did you mean `skip`?
 --> tests/fail/unknown_attr.rs:1:35
  |
1 | let attr = proto_ext::__parse_attr!(skp);
  |                                     ^^^
```

---

## Success Metrics

The prototype is successful if:

1. **Correctness**: All valid attribute syntaxes parse to the right values
2. **Errors**: Invalid attributes produce helpful, accurately-located errors
3. **Performance**: No proc-macro invocation on the happy path
4. **Ergonomics**: The `define_attr_grammar!` DSL is pleasant to write
5. **Cross-crate**: Works seamlessly across crate boundaries

---

## Next Steps

1. Create the crate skeleton (Cargo workspace + empty crates)
2. Start Phase 1: hand-write the macros
3. Validate the approach before investing in codegen
