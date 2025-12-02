+++
title = "Contribute"
sort_by = "weight"
weight = 3
+++

Help improve facet! This guide covers the project architecture and how to contribute.

## Development Setup

### Prerequisites

Install [just](https://github.com/casey/just) (a command runner):

```bash
# macOS
brew install just

# or with cargo
cargo install just
```

### Quick Start

Clone and verify your setup:

```bash
git clone https://github.com/facet-rs/facet
cd facet
just
```

If `just` runs successfully, CI will most likely pass.

### Running Tests

facet uses [cargo-nextest](https://nexte.st/) for testing:

```bash
# Run all tests
just test

# Run tests for a specific crate
cargo nextest run -p facet-json

# Run with logging
RUST_LOG=debug cargo nextest run -p facet-reflect
```

### Other Commands

```bash
# Check for undefined behavior with Miri
just miri

# Check no_std compatibility
just nostd-ci

# Run clippy
cargo clippy --workspace --all-features

# Build documentation
just docs
```

## Architecture Overview

### Crate Graph

```
┌─────────────────────────────────────────────────────────────────┐
│                         User Code                               │
│                    #[derive(Facet)]                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                          facet                                  │
│              Re-exports from core + macros + reflect            │
└─────────────────────────────────────────────────────────────────┘
          │                   │                   │
          ▼                   ▼                   ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│   facet-core    │  │  facet-macros   │  │ facet-reflect   │
│                 │  │                 │  │                 │
│ • Facet trait   │  │ • #[derive]     │  │ • Peek (read)   │
│ • Shape         │  │ • Proc macros   │  │ • Partial (build)│
│ • Def, Type     │  │                 │  │                 │
│ • VTables       │  │                 │  │                 │
│ • no_std        │  │                 │  │                 │
└─────────────────┘  └─────────────────┘  └─────────────────┘
          │                   │
          │                   ▼
          │          ┌─────────────────┐
          │          │facet-macros-impl│
          │          │                 │
          │          │ • unsynn parser │
          │          │ • Code gen      │
          │          └─────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Format Crates                              │
│  facet-json, facet-yaml, facet-kdl, facet-toml, facet-args...   │
└─────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Utility Crates                             │
│  facet-pretty, facet-diff, facet-assert, facet-value...         │
└─────────────────────────────────────────────────────────────────┘
```

### Key Crates

| Crate | Purpose |
|-------|---------|
| [`facet-core`](https://docs.rs/facet-core) | Core types: `Facet` trait, `Shape`, `Def`, vtables. Supports `no_std`. |
| [`facet-macros`](https://docs.rs/facet-macros) | The `#[derive(Facet)]` proc macro (thin wrapper). |
| `facet-macros-impl` | Actual derive macro implementation using [unsynn](https://docs.rs/unsynn). |
| [`facet-reflect`](https://docs.rs/facet-reflect) | Safe reflection APIs: `Peek` for reading, `Partial` for building. |
| [`facet`](https://docs.rs/facet) | Umbrella crate that re-exports everything. |

## Core Concepts

### The `Facet` Trait

Every reflectable type implements `Facet`, which exposes a single associated constant:

```rust
pub unsafe trait Facet<'facet>: 'facet {
    const SHAPE: &'static Shape;
}
```

The trait is `unsafe` because incorrect implementations break safety guarantees throughout the ecosystem.

### `Shape` — The Central Type

`Shape` describes everything about a type at runtime:

```rust
pub struct Shape {
    pub id: ConstTypeId,           // Unique type identifier
    pub layout: ShapeLayout,       // Size and alignment
    pub vtable: ValueVTable,       // Function pointers for operations
    pub ty: Type,                  // Structural classification
    pub def: Def,                  // Semantic definition
    pub type_identifier: &'static str,
    pub type_params: &'static [TypeParam],
    pub doc: &'static [&'static str],
    pub attributes: &'static [ShapeAttribute],
    // ...
}
```

### `Type` — Structural Classification

`Type` follows the [Rust Reference type categories](https://doc.rust-lang.org/reference/types.html):

- `Type::Primitive` — numeric, boolean, textual, never
- `Type::Sequence` — tuple, array, slice
- `Type::User` — struct, enum, union, opaque
- `Type::Pointer` — references, raw pointers, function pointers

### `Def` — Semantic Definition

`Def` describes *how* to interact with a type:

```rust
pub enum Def {
    Undefined,           // Interact via Type and ValueVTable only
    Scalar,              // Atomic values (u32, String, bool, etc.)
    Map(MapDef),         // HashMap<K, V>, BTreeMap<K, V>
    Set(SetDef),         // HashSet<T>, BTreeSet<T>
    List(ListDef),       // Vec<T>
    Array(ArrayDef),     // [T; N]
    Slice(SliceDef),     // [T]
    Option(OptionDef),   // Option<T>
    Pointer(PointerDef), // Arc<T>, Box<T>, Rc<T>
    // ...
}
```

### `ValueVTable` — Operations

`ValueVTable` contains function pointers for runtime operations:

```rust
pub struct ValueVTable {
    pub type_name: TypeNameFn,
    pub marker_traits: MarkerTraits,
    pub drop_in_place: Option<DropInPlaceFn>,
    pub display: Option<DisplayFn>,
    pub debug: Option<DebugFn>,
    pub default_in_place: Option<DefaultInPlaceFn>,
    pub clone_into: Option<CloneIntoFn>,
    pub partial_eq: Option<PartialEqFn>,
    pub partial_ord: Option<PartialOrdFn>,
    pub ord: Option<OrdFn>,
    pub hash: Option<HashFn>,
    pub parse: Option<ParseFn>,
    // ...
}
```

The `value_vtable!` macro auto-detects which traits a type implements and populates the vtable accordingly using autoderef specialization.

### `Characteristic` — Trait Detection

`Characteristic` lets you query whether a shape implements certain traits:

```rust
pub enum Characteristic {
    Send, Sync, Copy, Eq, Unpin,  // Marker traits
    Clone, Display, Debug,         // Functionality traits
    PartialEq, PartialOrd, Ord, Hash,
    Default, FromStr,
}

// Usage
if shape.is(Characteristic::Clone) {
    // Safe to call clone_into
}
```

## The Derive Macro

### How It Works

The `#[derive(Facet)]` macro:

1. Parses the type definition using [unsynn](https://docs.rs/unsynn) (fast, lightweight parser)
2. Collects field information, attributes, and doc comments
3. Generates a `Facet` impl with a `SHAPE` constant
4. Processes `#[facet(...)]` attributes (both built-in and extension)

### Generated Code Example

For this input:

```rust
#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}
```

The macro generates (simplified):

```rust
unsafe impl Facet<'_> for Person {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(value_vtable!(Person, |f, _| write!(f, "Person")))
            .type_identifier("Person")
            .ty(Type::User(UserType::Struct(StructType {
                kind: StructKind::Named,
                fields: &[
                    Field {
                        name: "name",
                        shape: || <String as Facet>::SHAPE,
                        offset: offset_of!(Person, name),
                        // ...
                    },
                    Field {
                        name: "age",
                        shape: || <u32 as Facet>::SHAPE,
                        offset: offset_of!(Person, age),
                        // ...
                    },
                ],
                // ...
            })))
            .build()
    };
}
```

### Extension Attributes

The derive macro supports namespaced extension attributes like `#[facet(kdl::property)]`. See the [Extend guide](/extend/) for how these work.

## Adding Support for New Types

### Standard Library Types

To add `Facet` support for a new standard library type, add an implementation in the appropriate `impls_*` module in `facet-core`:

```rust
// In facet-core/src/impls_core/scalar.rs (for example)

unsafe impl Facet<'_> for MyType {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(value_vtable!(MyType, |f, _opts| {
                write!(f, "MyType")
            }))
            .type_identifier("MyType")
            .def(Def::Scalar)  // or appropriate Def variant
            .ty(Type::User(UserType::Opaque))
            .build()
    };
}
```

### External Crate Types

For types from external crates, add a new feature-gated module:

1. Add the dependency to `facet-core/Cargo.toml`:
   ```toml
   [dependencies]
   my-crate = { version = "1.0", optional = true }

   [features]
   my-crate = ["dep:my-crate"]
   ```

2. Create `facet-core/src/impls_my_crate.rs`

3. Add to `facet-core/src/lib.rs`:
   ```rust
   #[cfg(feature = "my-crate")]
   mod impls_my_crate;
   ```

### Collection Types

For collection types, you need to implement the appropriate `Def` variant with vtable functions:

```rust
unsafe impl<T: Facet<'static>> Facet<'_> for MyVec<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(value_vtable!(MyVec<T>, |f, opts| {
                write!(f, "MyVec<")?;
                (T::SHAPE.vtable.type_name)(f, opts)?;
                write!(f, ">")
            }))
            .type_identifier("MyVec")
            .def(Def::List(ListDef {
                vtable: &ListVTable {
                    init_empty: |target| { /* ... */ },
                    push: |list, value| { /* ... */ },
                    len: |list| { /* ... */ },
                    get: |list, index| { /* ... */ },
                },
                item_shape: T::SHAPE,
            }))
            .ty(Type::User(UserType::Opaque))
            .type_params(&[TypeParam { name: "T", shape: T::SHAPE }])
            .build()
    };
}
```

## Testing

### Test Organization

- Unit tests live alongside the code
- Integration tests are in `tests/` directories
- Showcases in `examples/` generate documentation

### Running Specific Tests

```bash
# Test a specific crate
cargo nextest run -p facet-json

# Test with a filter
cargo nextest run -p facet-reflect partial

# Run with logging
RUST_LOG=trace cargo nextest run -p facet-reflect -- --nocapture
```

### Miri for Unsafe Code

Always run Miri when modifying unsafe code:

```bash
just miri
```

### Snapshot Testing

Some crates use [insta](https://docs.rs/insta) for snapshot testing. Update snapshots with:

```bash
cargo insta review
```

## Pull Request Process

1. **Create a branch** — never commit directly to `main`
2. **Write tests** — ensure your changes are covered
3. **Run checks locally**:
   ```bash
   just           # Full test suite
   just miri      # Memory safety
   just nostd-ci  # no_std compatibility
   ```
4. **Push and open a PR** with `gh pr create`
5. **CI must pass** — the test matrix includes:
   - Tests (Linux, macOS, Windows)
   - no_std build
   - Miri
   - MSRV check
   - Clippy
   - Documentation build

### Generated Files

Do **not** edit `README.md` files directly. Edit `README.md.in` in the respective folders instead — READMEs are generated.

## Next Steps

- Join the [Discord](https://discord.gg/JhD7CwCJ8F) to discuss changes before starting
- Browse [open issues](https://github.com/facet-rs/facet/issues) for things to work on
- Check the [Extend guide](/extend/) if you're building a format crate
- Read the [API documentation](https://docs.rs/facet) for the full type reference
