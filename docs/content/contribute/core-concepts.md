+++
title = "Core Concepts"
weight = 4
insert_anchor_links = "heading"
+++

## The `Facet` Trait

Every reflectable type implements `Facet`:

```rust
pub unsafe trait Facet<'facet>: 'facet {
    const SHAPE: &'static Shape;
}
```

The trait is `unsafe` because incorrect implementations break safety guarantees throughout the ecosystem.

## `Shape`

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

## `Type`

Structural classification following the [Rust Reference](https://doc.rust-lang.org/reference/types.html):

- `Type::Primitive` — numeric, boolean, textual, never
- `Type::Sequence` — tuple, array, slice
- `Type::User` — struct, enum, union, opaque
- `Type::Pointer` — references, raw pointers, function pointers

## `Def`

Semantic definition — *how* to interact with a type:

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

## `ValueVTable`

Function pointers for runtime operations:

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

The `value_vtable!` macro auto-detects which traits a type implements using autoderef specialization.

## `Characteristic`

Query whether a shape implements certain traits:

```rust
pub enum Characteristic {
    Send, Sync, Copy, Eq, Unpin,
    Clone, Display, Debug,
    PartialEq, PartialOrd, Ord, Hash,
    Default, FromStr,
}

// Usage
if shape.is(Characteristic::Clone) {
    // Safe to call clone_into
}
```
