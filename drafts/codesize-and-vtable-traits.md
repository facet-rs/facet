# Facet Code Size & VTable Traits

This document consolidates our work on reducing compile times, code size, and the vtable traits detection system.

## Current Status (2024-12-06)

### What's Done

**VTable Structure:**
- `ValueVTable` reorganized with grouped sub-structs (`format`, `cmp`, `hash`, `markers`)
- Feature gates removed - all vtable fields are always present, no `#[cfg]` conditionals
- Builder pattern restored using `ValueVTable::builder()` and `ShapeBuilder`

**Trait Detection:**
- `#[facet(traits(Debug, PartialEq, ...))]` - Manual trait declaration attribute
- `#[facet(auto_traits)]` - Opt-in automatic detection attribute
- Static assertions ensure declared traits are actually implemented
- Default is no detection - derive macro defaults to `None` for traits not in derives/declared
- `#[facet(default)]` now implies Default trait (don't need both)

**Code Size Wins:**
- Disabled `miette` default features (drops `derive`/`syn` dependency)
- Flattened some closures in `facet-json` entry points
- Fixed contradictory inline attributes

### In Progress

**Builder Migration:**
- Converting manual impls in `impls_*.rs` to use `ShapeBuilder`
- `scalar.rs`, `fn_ptr.rs`, `impls_num_complex.rs` converted to builders
- Remaining files pending conversion

## Trait Detection Design

### The Problem

Facet's `ValueVTable` contains function pointers for various trait implementations. Previously, we used Rust's specialization trick (`impls!` macro) to automatically detect which traits a type implements.

**Problem**: This specialization code is expensive to compile. For every type deriving Facet, the compiler had to generate and evaluate specialization code for ~15+ traits.

### The Solution: Three Modes

| Mode | Attribute | Compile Time | Traits Detected | Use Case |
|------|-----------|--------------|-----------------|----------|
| Default | (none) | Fast | None | Maximum compile speed |
| Manual | `#[facet(traits(...))]` | Fast | Declared only | Production code |
| Auto | `#[facet(auto_traits)]` | Slower | All implemented | Prototyping |

#### Default: No Detection

```rust
#[derive(Facet)]
struct MyType { ... }
```

All vtable trait fields are `None`. Fast compile times.

#### Manual Declaration

```rust
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(traits(Debug, PartialEq, Clone))]
struct MyType { ... }
```

For each declared trait:
1. Generate vtable function pointer
2. Set corresponding bit in `MarkerTraits`
3. Emit compile-time assertion

#### Auto Detection (Opt-in)

```rust
#[derive(Facet, Debug)]
#[facet(auto_traits)]
struct MyType { ... }
```

Uses `impls!` macro for detection. Slower compile time.

**Note:** `traits(...)` and `auto_traits` are mutually exclusive.

## VTable Structure

```rust
pub struct ValueVTable {
    pub type_name: TypeNameFn,
    pub drop_in_place: Option<DropInPlaceFn>,
    pub invariants: Option<InvariantsFn>,
    pub default_in_place: Option<DefaultInPlaceFn>,
    pub clone_into: Option<CloneIntoFn>,
    pub parse: Option<ParseFn>,
    pub try_from: Option<TryFromFn>,
    pub try_into_inner: Option<TryIntoInnerFn>,
    pub try_borrow_inner: Option<TryBorrowInnerFn>,
    pub format: FormatVTable,
    pub cmp: CmpVTable,
    pub hash: HashVTable,
    pub markers: MarkerTraits,
}

pub struct FormatVTable {
    pub display: Option<DisplayFn>,
    pub debug: Option<DebugFn>,
}

pub struct CmpVTable {
    pub partial_eq: Option<PartialEqFn>,
    pub partial_ord: Option<PartialOrdFn>,
    pub ord: Option<OrdFn>,
}

pub struct HashVTable {
    pub hash: Option<HashFn>,
}

// Hand-rolled bitflags
#[repr(transparent)]
pub struct MarkerTraits(u8);

impl MarkerTraits {
    pub const EMPTY: Self = Self(0);
    pub const COPY: Self = Self(0b0000_0001);
    pub const SEND: Self = Self(0b0000_0010);
    // etc.
}
```

## Builder Pattern

We use builders instead of struct literals because:
- Fewer tokens to parse
- Default values implicit
- Conditional fields easier
- Forward compatible
- Better compile times

```rust
// Flat builder pattern (preferred) - vtable setters directly on ShapeBuilder
ShapeBuilder::for_sized::<MyType>(type_name_fn, "MyType")
    .ty(Type::User(UserType::Struct(...)))
    .debug(Some(debug_fn))
    .partial_eq(Some(eq_fn))
    .markers(MarkerTraits::EMPTY.with_eq())
    .build()
```

### TODO: Builder Ergonomics

Current vtable setters take `Option<Fn>`:
```rust
.debug(Some(debug_fn))
```

Should take the function directly for ergonomics:
```rust
.debug(debug_fn)
```

Options:
1. `f: impl Into<Option<DebugFn>>` - works with both `Some(f)` and `f`
2. `f: DebugFn` directly - simpler, better compile times

When adding new builders (Field, Variant), use option 2.

## Trait Visibility Tracking (Future)

When a vtable field like `default_fn` is `None`, it could mean:
1. The type definitively does NOT implement the trait
2. We don't know if the type implements the trait

This ambiguity makes diagnostics difficult. Future work may encode three states:

```rust
pub enum TraitAvailability<F> {
    Available(F),
    NotImplemented,
    Unknown,
}
```

This would enable better error messages like "Type `Foo` may implement `Default` but facet cannot see it - consider adding `#[facet(traits(Default))]`"

**Priority:** Nice-to-have, not blocking.

## Code Size Notes

### Promising Leads

- Emit field shapes directly instead of per-field projection closures
- Inline discipline: remove default `#[inline]` on vtable closures, add `#[inline(never)] #[cold]` to error paths
- Gate tracing behind a cargo feature
- Keep `miette`/diagnostic features off in benchmarks

### Rejected Approaches

- Hoisting vtable closures into shared helpers: breaks `spez::impls!` specialization
- Using plain `type_name_fn::<T>()`: want pretty generic-aware names
- Trait-object rewrites for vtables: risky, less targeted
- Static/blob shape representation: too complex
- Caching `offset_of!` calls: just moves them around

## Migration Guide

Existing `#[derive(Facet)]` code continues to compile but has empty vtable trait fields by default.

To restore previous behavior:
1. **Quick fix**: Add `#[facet(auto_traits)]`
2. **Recommended**: Explicitly declare traits with `#[facet(traits(...))]`
