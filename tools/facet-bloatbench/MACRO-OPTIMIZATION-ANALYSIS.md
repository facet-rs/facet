# Facet Derive Macro Optimization Analysis

This document analyzes the expanded output of the `#[derive(Facet)]` macro to identify
opportunities for reducing generated code size and improving compile times.

**Source**: `cargo expand -p facet-bloatbench --lib --features facet`

## Progress Update (2024-12-06)

After VTable restructuring and benchmark schema simplification:
- **Before**: 4086 lines expanded
- **After**: 1286 lines expanded
- **Reduction**: 68% fewer lines

The builder pattern is now in use, but many token-level optimizations remain applicable.
Cross-reference with `drafts/codesize-and-vtable-traits.md` for VTable structure details.

---

## 1. Verbose VTable Builder Chain

**Current output** (~15 lines per type):

```rust
vtable: {
    let mut vtable = const {
        ::facet::ValueVTable::builder(|f, _opts| ::core::fmt::Write::write_str(f, "Struct000"))
            .drop_in_place(::facet::ValueVTable::drop_in_place_for::<Self>())
            .display({ None })
            .debug({ None })
            .default_in_place({
                Some(|target| unsafe {
                    target.put(<Self as core::default::Default>::default())
                })
            })
            .clone_into({ None })
            .partial_eq({ None })
            .partial_ord({ None })
            .ord({ None })
            .hash({ None })
            .parse({ None })
            .markers({ ::facet::MarkerTraits::EMPTY })
            .build()
    };
    vtable
}
```

**Problems**:
- 8 method calls that just set `None` or `MarkerTraits::EMPTY`
- Unnecessary `let mut vtable = ...; vtable` pattern
- Each `.xxx({ None })` is ~15-20 tokens

**Potential fix**: Use struct literal with defaults:

```rust
vtable: const {
    ::facet::ValueVTable {
        type_name: |f, _opts| ::core::fmt::Write::write_str(f, "Struct000"),
        drop_in_place: ::facet::ValueVTable::drop_in_place_for::<Self>(),
        default_in_place: Some(|target| unsafe {
            target.put(<Self as core::default::Default>::default())
        }),
        ..::facet::ValueVTable::EMPTY
    }
}
```

**Estimated savings**: ~80 tokens per type

---

## 2. Dead Code Validation Blocks

**Current output for structs**:

```rust
const _: () = {
    #[allow(dead_code, clippy::multiple_bound_locations)]
    fn __facet_use_struct<'__facet>(__v: &Struct000) {
        let _ = __v;
    }
};
```

**Current output for enums** (much larger):

```rust
const _: () = {
    #[allow(dead_code, unreachable_code, clippy::multiple_bound_locations, clippy::diverging_sub_expression)]
    fn __facet_construct_all_variants<'__facet>() -> Enum001 {
        loop {
            let _: Enum001 = Enum001::V0(::core::panicking::panic("not yet implemented"));
            let _: Enum001 = Enum001::V1(
                ::core::panicking::panic("not yet implemented"),
                ::core::panicking::panic("not yet implemented"),
                ::core::panicking::panic("not yet implemented"),
            );
            // ... one line per variant field
        }
    }
};
```

**Problem**: These exist for compile-time validation of field types but generate significant code.

**Questions to consider**:
- Is this validation strictly necessary?
- Could it be behind a feature flag for debug builds only?
- Could a simpler pattern achieve the same validation?

**Estimated savings**: ~50 tokens per struct, ~100+ tokens per enum

---

## 3. Static SHAPE References

**Current output**:

```rust
static STRUCT000_SHAPE: &'static ::facet::Shape = <Struct000 as ::facet::Facet>::SHAPE;
```

**Problem**: Generated for every type but appears unused in user code. If this is for
forcing monomorphization, there may be lighter alternatives.

**Potential fix**: Remove entirely, or use `const _: &Shape = ...` if instantiation is needed.

**Estimated savings**: ~20 tokens per type

---

## 4. Verbose Shape Struct Initialization

**Current output**:

```rust
::facet::Shape {
    id: ::facet::Shape::id_of::<Self>(),
    layout: ::facet::Shape::layout_of::<Self>(),
    vtable: /* ... */,
    ty: ::facet::Type::User(
        ::facet::UserType::Struct(::facet::StructType {
            repr: ::facet::Repr::default(),
            kind: ::facet::StructKind::Struct,
            fields,
        }),
    ),
    def: ::facet::Def::Undefined,
    type_identifier: "Struct000",
    type_params: &[],
    doc: &[],
    attributes: &[],
    type_tag: None,
    inner: None,
}
```

**Problem**: 6 fields are consistently empty/default:
- `def: ::facet::Def::Undefined`
- `type_params: &[]`
- `doc: &[]`
- `attributes: &[]`
- `type_tag: None`
- `inner: None`

**Potential fix**: Use struct update syntax:

```rust
::facet::Shape {
    id: ::facet::Shape::id_of::<Self>(),
    layout: ::facet::Shape::layout_of::<Self>(),
    vtable: /* ... */,
    ty: /* ... */,
    type_identifier: "Struct000",
    ..::facet::Shape::UNDEFINED
}
```

**Estimated savings**: ~60 tokens per type

---

## 5. Verbose Field Definitions

**Current output**:

```rust
::facet::Field {
    name: "field_0",
    shape: || <Vec<Cow<'static, str>> as ::facet::Facet>::SHAPE,
    offset: { builtin # offset_of(Struct000, field_0) },
    attributes: &[],
    doc: &[],
}
```

**Problem**: `attributes: &[]` and `doc: &[]` repeated for every field in every struct.

**Potential fix**: Add a constructor that defaults empty attributes/doc:

```rust
::facet::Field::new(
    "field_0",
    || <Vec<Cow<'static, str>> as ::facet::Facet>::SHAPE,
    { builtin # offset_of(Struct000, field_0) },
)
```

Or use struct update:

```rust
::facet::Field {
    name: "field_0",
    shape: || <Vec<Cow<'static, str>> as ::facet::Facet>::SHAPE,
    offset: { builtin # offset_of(Struct000, field_0) },
    ..::facet::Field::EMPTY
}
```

**Estimated savings**: ~15 tokens per field

---

## 6. Shadow Structs for Enum Variants

**Current output** (one per variant with data):

```rust
#[repr(C)]
#[allow(non_snake_case, dead_code)]
struct __Shadow_RustRepr_Tuple_for_Enum001_V0<'__facet> {
    _discriminant: u16,
    _phantom: ::core::marker::PhantomData<(*mut &'__facet ())>,
    _0: Option<bool>,
}
```

**Problem**: Each variant with data needs a shadow struct for `offset_of`. Complex enums
generate many of these.

**Potential fixes**:
- This is likely unavoidable due to how `offset_of` works
- The `_phantom` field pattern is verbose; could it be simplified?
- Consider if there's a way to share discriminant/phantom across variants

**Estimated savings**: Limited, but worth investigating for enums with many variants

---

## 7. Redundant Braces

**Current patterns**:

```rust
offset: { builtin # offset_of(Struct000, field_0) }
.display({ None })
.default_in_place({ Some(...) })
```

**Problem**: Extra braces around expressions add tokens.

**Potential fix**: Remove unnecessary braces:

```rust
offset: builtin # offset_of(Struct000, field_0),
.display(None)
.default_in_place(Some(...))
```

**Estimated savings**: ~2-3 tokens per occurrence, adds up

---

## Summary: Estimated Token Savings Per Type

| Optimization | Tokens Saved | Complexity |
|--------------|--------------|------------|
| VTable builder → struct literal | ~80 | Medium |
| Remove dead code blocks | ~50-100 | Low |
| Remove static SHAPE refs | ~20 | Low |
| Shape struct update syntax | ~60 | Low |
| Field struct update/constructor | ~15/field | Low |
| Remove redundant braces | ~20 | Low |

**For a typical struct with 5 fields**: ~300 tokens saved
**For a typical enum with 5 variants**: ~500+ tokens saved

---

## Implementation Priority

### High Impact, Low Effort
1. Remove `let mut vtable = ...; vtable` pattern (already fixed in enum code path!)
2. Use struct update syntax for `Shape` with empty defaults
3. Use struct update syntax for `Field` with empty attributes/doc
4. Remove redundant `{ }` braces around expressions

### High Impact, Medium Effort
5. Replace VTable builder chain with struct literal + defaults
6. Remove or simplify `static XXXXX_SHAPE` references

### Medium Impact, Needs Investigation
7. Simplify or gate dead code validation blocks
8. Investigate shadow struct optimization for enums

---

## 8. Fully Qualified Paths Everywhere

**Current output**:

```rust
::facet::Shape::id_of::<Self>()
::facet::Shape::layout_of::<Self>()
::facet::ValueVTable::builder(...)
::facet::ValueVTable::drop_in_place_for::<Self>()
::core::default::Default::default()
::core::fmt::Write::write_str(f, "Struct000")
::facet::StructKind::Struct
::facet::Repr::default()
```

**Problem**: Every single reference uses fully qualified paths with `::` prefix. This is
repeated hundreds of times across even a modest number of types.

**Potential fix**: The macro could emit a preamble with type aliases or use statements
inside the const block:

```rust
const SHAPE: &'static ::facet::Shape = &const {
    use ::facet::{Shape, Field, ValueVTable, StructType, StructKind, Repr, Type, UserType, Def};
    // ... much shorter references follow
    Shape {
        id: Shape::id_of::<Self>(),
        // ...
    }
};
```

**Estimated savings**: ~5-10 tokens per reference, potentially 100+ tokens per type

---

## 9. Redundant Discriminant Casting

**Current output**:

```rust
discriminant: Some(0i64 as i64),
discriminant: Some(1i64 as i64),
discriminant: Some(2i64 as i64),
```

**Problem**: `0i64 as i64` is a no-op cast. The literal is already `i64`, then cast to `i64`.

**Potential fix**:

```rust
discriminant: Some(0),
discriminant: Some(1),
discriminant: Some(2),
```

Or if explicit typing is needed:

```rust
discriminant: Some(0_i64),
```

**Estimated savings**: ~4 tokens per variant

---

## 10. Repeated `Repr::c()` Calls

**Current output** (in every variant):

```rust
data: ::facet::StructType {
    repr: ::facet::Repr::c(),
    kind: ::facet::StructKind::Tuple,
    fields,
},
```

**Problem**: `::facet::Repr::c()` is a function call repeated for every enum variant.
Even if it's const, it's verbose.

**Potential fix**: Use a const:

```rust
repr: ::facet::Repr::C,
// or
repr: REPR_C,  // with `const REPR_C: Repr = Repr::c();` in preamble
```

**Estimated savings**: ~8 tokens per variant

---

## 11. Verbose PhantomData in Shadow Structs

**Current output**:

```rust
#[repr(C)]
struct __Shadow_RustRepr_Tuple_for_Enum001_V0<'__facet> {
    _discriminant: u16,
    _phantom: ::core::marker::PhantomData<(*mut &'__facet ())>,
    _0: Option<bool>,
}
```

**Problem**: The `PhantomData<(*mut &'__facet ())>` pattern is extremely verbose and
repeated in every shadow struct. The `(*mut &'__facet ())` is an unusual variance marker.

**Potential fix**: Define a type alias in `facet` crate:

```rust
// In facet crate:
pub type __Phantom<'a> = PhantomData<(*mut &'a ())>;

// In generated code:
struct __Shadow_RustRepr_Tuple_for_Enum001_V0<'__facet> {
    _discriminant: u16,
    _phantom: ::facet::__Phantom<'__facet>,
    _0: Option<bool>,
}
```

**Estimated savings**: ~15 tokens per shadow struct

---

## 12. Nested Const Blocks

**Current output**:

```rust
const SHAPE: &'static ::facet::Shape = &const {
    let fields: &'static [::facet::Field] = &const {
        [
            ::facet::Field { ... },
            ::facet::Field { ... },
        ]
    };
    ::facet::Shape { ... fields ... }
};
```

**Problem**: Double nesting of `&const { ... &const { ... } }`. The inner const block
for fields may be unnecessary.

**Potential fix**: If the fields array can be directly inlined:

```rust
const SHAPE: &'static ::facet::Shape = &const {
    ::facet::Shape {
        // ...
        ty: ::facet::Type::User(::facet::UserType::Struct(::facet::StructType {
            fields: &[
                ::facet::Field { ... },
                ::facet::Field { ... },
            ],
            // ...
        })),
    }
};
```

This eliminates the intermediate `let fields = ...` binding.

**Estimated savings**: ~15 tokens per type, plus potential compile-time improvement

---

## 13. Verbose StructType for Unit Variants

**Current output** (for unit enum variants):

```rust
::facet::Variant {
    name: "V2",
    discriminant: Some(2i64 as i64),
    attributes: &[],
    data: ::facet::StructType {
        repr: ::facet::Repr::c(),
        kind: ::facet::StructKind::Unit,
        fields: &[],
    },
    doc: &[],
}
```

**Problem**: Unit variants still get a full `StructType` with `fields: &[]`. This is
repeated verbatim for every unit variant.

**Potential fix**: Add a const for unit struct type:

```rust
// In facet crate:
pub const UNIT_STRUCT_TYPE: StructType = StructType {
    repr: Repr::c(),
    kind: StructKind::Unit,
    fields: &[],
};

// In generated code:
::facet::Variant {
    name: "V2",
    discriminant: Some(2),
    attributes: &[],
    data: ::facet::UNIT_STRUCT_TYPE,
    doc: &[],
}
```

Or with struct update syntax:

```rust
::facet::Variant {
    name: "V2",
    discriminant: Some(2),
    data: ::facet::StructType::UNIT,
    ..::facet::Variant::EMPTY
}
```

**Estimated savings**: ~25 tokens per unit variant

---

## 14. Turbofish in Shape Closures

**Current output**:

```rust
shape: || <Vec<Cow<'static, str>> as ::facet::Facet>::SHAPE,
shape: || <Option<String> as ::facet::Facet>::SHAPE,
shape: || <Struct005 as ::facet::Facet>::SHAPE,
```

**Problem**: The turbofish `<Type as ::facet::Facet>::SHAPE` syntax is verbose,
especially for generic types. The closure `|| ...` wrapper is necessary for lazy
evaluation but adds overhead.

**Potential fix**: Consider a helper macro or function in the facet crate:

```rust
// In facet crate:
#[macro_export]
macro_rules! shape_of {
    ($t:ty) => { || <$t as $crate::Facet>::SHAPE }
}

// In generated code:
shape: ::facet::shape_of!(Vec<Cow<'static, str>>),
```

Or a const fn (though this may have limitations):

```rust
shape: ::facet::shape_fn::<Vec<Cow<'static, str>>>(),
```

**Estimated savings**: ~10 tokens per field (for complex types)

---

## Updated Summary: All Optimization Opportunities

| # | Optimization | Tokens Saved | Complexity |
|---|--------------|--------------|------------|
| 1 | VTable builder → struct literal | ~80/type | Medium |
| 2 | Remove dead code blocks | ~50-100/type | Low |
| 3 | Remove static SHAPE refs | ~20/type | Low |
| 4 | Shape struct update syntax | ~60/type | Low |
| 5 | Field struct update/constructor | ~15/field | Low |
| 6 | Shadow structs (limited) | ~10/variant | High |
| 7 | Remove redundant braces | ~20/type | Low |
| 8 | Use imports instead of FQ paths | ~100+/type | Medium |
| 9 | Remove redundant i64 casts | ~4/variant | Low |
| 10 | Repr::C const instead of fn call | ~8/variant | Low |
| 11 | PhantomData type alias | ~15/shadow struct | Low |
| 12 | Eliminate nested const blocks | ~15/type | Low |
| 13 | Unit variant StructType const | ~25/unit variant | Low |
| 14 | shape_of! macro for closures | ~10/field | Medium |

**Cumulative estimate for a crate with 50 structs (avg 5 fields) and 20 enums (avg 5 variants)**:
- Current: ~50,000+ tokens
- After optimizations: ~25,000-30,000 tokens (40-50% reduction)

---

## Implementation Priority (Updated)

### Quick Wins (Low effort, immediate impact)
1. Remove `i64 as i64` redundant casts
2. Remove redundant `{ }` braces
3. Remove `let mut vtable = ...; vtable` pattern
4. Eliminate nested const blocks where possible

### Medium Effort, High Impact
5. Use struct update syntax for `Shape`, `Field`, `Variant`
6. Add `UNIT_STRUCT_TYPE` and similar constants
7. Replace VTable builder with struct literal
8. Add `Repr::C` const

### Requires Facet Crate Changes
9. Add `::facet::__Phantom<'a>` type alias
10. Add `shape_of!` macro
11. Add `Shape::UNDEFINED`, `Field::EMPTY`, `Variant::EMPTY` constants
12. Consider adding import preamble generation

### Needs Investigation
13. Simplify or conditionally compile dead code validation blocks
14. Investigate if shadow structs can be optimized further

---

## 15. Ultra-Short Prelude Module (ʬ)

**Idea from the boss**: Create a module with a short unicode name that re-exports
everything with minimal names.

### Usage Frequency Analysis

From expanded bloatbench output (6 types):

| Current Path | Count | Short |
|--------------|-------|-------|
| `::facet::Field` | 57 | `F` |
| `::facet::Facet` | 55 | `Fc` |
| `::facet::Shape` | 30 | `S` |
| `::facet::Repr` | 23 | `R` |
| `::facet::Variant` | 20 | `V` |
| `::facet::StructType` | 20 | `ST` |
| `::facet::StructKind` | 20 | `SK` |
| `::facet::ValueVTable` | 12 | `VT` |
| `::facet::UserType` | 6 | `UT` |
| `::facet::Type` | 6 | `T` |
| `::facet::MarkerTraits` | 6 | `M` |
| `::facet::Def` | 6 | `D` |
| `::facet::EnumType` | 3 | `ET` |
| `::facet::EnumRepr` | 3 | `ER` |

Associated functions/constants:

| Current Path | Count | Short |
|--------------|-------|-------|
| `::facet::Repr::c()` | 17 | `R::C` (const) |
| `::facet::StructKind::Struct` | 10 | `SK::S` |
| `::facet::ValueVTable::drop_in_place_for` | 6 | `VT::drop` |
| `::facet::ValueVTable::builder` | 6 | `VT::b` |
| `::facet::Type::User` | 6 | `T::U` |
| `::facet::StructKind::Unit` | 6 | `SK::U` |
| `::facet::Shape::layout_of` | 6 | `S::lo` |
| `::facet::Shape::id_of` | 6 | `S::id` |
| `::facet::Repr::default()` | 6 | `R::D` (const) |
| `::facet::MarkerTraits::EMPTY` | 6 | `M::E` |
| `::facet::Def::Undefined` | 6 | `D::U` |
| `::facet::StructKind::Tuple` | 4 | `SK::T` |
| `::facet::UserType::Struct` | 3 | `UT::S` |
| `::facet::UserType::Enum` | 3 | `UT::E` |
| `::facet::EnumRepr::U16` | 3 | `ER::U16` |

### Complete ʬ Module Definition

```rust
// In facet-core/src/lib.rs or facet/src/lib.rs:

/// Ultra-compact prelude for derive macro codegen.
/// Using ʬ (U+02AC, Latin Letter Bilabial Percussive) for minimal token size.
pub mod ʬ {
    // === Types (frequency-ordered) ===
    pub use crate::Field as F;
    pub use crate::Facet as Fc;
    pub use crate::Shape as S;
    pub use crate::Repr as R;
    pub use crate::Variant as V;
    pub use crate::StructType as ST;
    pub use crate::StructKind as SK;
    pub use crate::ValueVTable as VT;
    pub use crate::UserType as UT;
    pub use crate::Type as T;
    pub use crate::MarkerTraits as M;
    pub use crate::Def as D;
    pub use crate::EnumType as ET;
    pub use crate::EnumRepr as ER;

    // === Constants for common values ===
    /// Empty attributes slice
    pub const A: &[crate::Attribute] = &[];
    /// Empty doc slice
    pub const DC: &[&str] = &[];
    /// Repr::c() as const
    pub const RC: crate::Repr = crate::Repr::c();
    /// Repr::default() as const (= Repr::Rust)
    pub const RD: crate::Repr = crate::Repr::RUST;
    /// MarkerTraits::EMPTY
    pub const ME: crate::MarkerTraits = crate::MarkerTraits::EMPTY;
    /// Def::Undefined
    pub const DU: crate::Def = crate::Def::Undefined;
    /// StructType for unit variants
    pub const STU: crate::StructType = crate::StructType {
        repr: crate::Repr::c(),
        kind: crate::StructKind::Unit,
        fields: &[],
    };

    // === Shape with defaults for struct update ===
    impl crate::Shape {
        pub const UNDEF: Self = Self {
            id: crate::ShapeId(0), // placeholder, must be overwritten
            layout: crate::ShapeLayout::Sized(core::alloc::Layout::new::<()>()),
            vtable: crate::ValueVTable::EMPTY,
            ty: crate::Type::User(crate::UserType::Unit),
            def: crate::Def::Undefined,
            type_identifier: "",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
        };
    }

    // === Field with defaults ===
    impl crate::Field {
        pub const EMPTY: Self = Self {
            name: "",
            shape: || panic!("shape not set"),
            offset: 0,
            attributes: &[],
            doc: &[],
        };
    }

    // === Variant with defaults ===
    impl crate::Variant {
        pub const EMPTY: Self = Self {
            name: "",
            discriminant: None,
            attributes: &[],
            data: crate::StructType::UNIT,
            doc: &[],
        };
    }
}
```

### Generated Code Comparison

**Before** (~95 tokens for Shape):
```rust
::facet::Shape {
    id: ::facet::Shape::id_of::<Self>(),
    layout: ::facet::Shape::layout_of::<Self>(),
    vtable: { let mut vtable = const { ::facet::ValueVTable::builder(...).build() }; vtable },
    ty: ::facet::Type::User(::facet::UserType::Struct(::facet::StructType {
        repr: ::facet::Repr::default(),
        kind: ::facet::StructKind::Struct,
        fields,
    })),
    def: ::facet::Def::Undefined,
    type_identifier: "Struct000",
    type_params: &[],
    doc: &[],
    attributes: &[],
    type_tag: None,
    inner: None,
}
```

**After** (~35 tokens):
```rust
use ::facet::ʬ::*;
S {
    id: S::id_of::<Self>(),
    layout: S::layout_of::<Self>(),
    vtable: VT::b(...).build(),
    ty: T::U(UT::S(ST { repr: RD, kind: SK::S, fields })),
    type_identifier: "Struct000",
    ..S::UNDEF
}
```

**Before** (~25 tokens per field):
```rust
::facet::Field {
    name: "field_0",
    shape: || <Vec<Cow<'static, str>> as ::facet::Facet>::SHAPE,
    offset: { builtin # offset_of(Struct000, field_0) },
    attributes: &[],
    doc: &[],
}
```

**After** (~15 tokens):
```rust
F { name: "field_0", shape: || <Vec<Cow<'static, str>> as Fc>::SHAPE,
    offset: builtin # offset_of(Struct000, field_0), ..F::EMPTY }
```

### Token Savings Estimate

For the bloatbench crate (6 types, ~57 fields, ~20 variants):
- Shape paths: 30 × 10 chars saved = ~300 chars
- Field paths: 57 × 8 chars saved = ~456 chars
- Other paths: ~200 × 5 chars saved = ~1000 chars
- Struct update syntax: 6 × 50 tokens = ~300 tokens
- **Total: ~60-65% token reduction in facet:: references**

**Estimated savings**: 50-70% reduction in path tokens, potentially 200+ tokens per type

---

## 16. Shorter Shadow Struct Names

**Current output**:

```rust
struct __Shadow_RustRepr_Tuple_for_Enum003_V4<'__facet> { ... }
struct __Shadow_RustRepr_Struct_for_Enum006_V0<'__facet> { ... }
```

**Problem**: These names are 35-40 characters each, repeated in struct definition
and every `offset_of` call.

**Potential fix**: Use minimal indexed names since they're scoped to the const block:

```rust
struct _T4<'ʃ> { ... }  // Tuple variant 4
struct _S0<'ʃ> { ... }  // Struct variant 0
```

Or with ASCII:

```rust
struct __4<'a> { ... }
struct __0<'a> { ... }
```

**Estimated savings**: ~30 tokens per shadow struct (definition + all offset_of references)

---

## 17. Shorter Lifetime Name

**Current output**:

```rust
unsafe impl<'__facet> ::facet::Facet<'__facet> for Enum007 {
    const SHAPE: &'static ::facet::Shape = &const {
        struct __Shadow<'__facet> {
            _phantom: ::core::marker::PhantomData<(*mut &'__facet ())>,
            // ...
        }
    };
}
```

**Problem**: `'__facet` is 8 characters, appears many times per type.

**Potential fix**: Use a single-character lifetime:

```rust
unsafe impl<'ʃ> ::facet::Facet<'ʃ> for Enum007 { ... }
// or ASCII:
unsafe impl<'a> ::facet::Facet<'a> for Enum007 { ... }
```

**Estimated savings**: ~5-6 tokens per occurrence, 20-50 tokens per type

---

## 18. Consolidated Allow Attributes

**Current output**:

```rust
#[allow(dead_code, unreachable_code, clippy::multiple_bound_locations, clippy::diverging_sub_expression)]
fn __facet_construct_all_variants<'__facet>() -> Enum004 { ... }

#[allow(non_snake_case, dead_code)]
struct __Shadow_RustRepr_Tuple_for_Enum004_V1<'__facet> { ... }

#[allow(non_snake_case, dead_code)]
struct __Shadow_RustRepr_Struct_for_Enum004_V3<'__facet> { ... }
```

**Problem**: Same allow attributes repeated on every shadow struct and validation function.

**Potential fix**: Define a macro for common attribute combinations:

```rust
// In facet crate:
#[macro_export]
macro_rules! __allow_shadow {
    ($($item:item)*) => {
        #[allow(non_snake_case, dead_code)]
        $($item)*
    };
}

// In generated code:
::facet::__allow_shadow! {
    struct _S0<'a> { ... }
    struct _S1<'a> { ... }
    struct _T2<'a> { ... }
}
```

Or use a single module-level attribute.

**Estimated savings**: ~15 tokens per shadow struct

---

## 19. Type Name Writer Simplification

**Current output**:

```rust
::facet::ValueVTable::builder(|f, _opts| ::core::fmt::Write::write_str(f, "Struct000"))
```

**Problem**: This closure pattern is identical for every type except the string literal.
The `_opts` parameter is always ignored. Very verbose.

**Potential fix**: Add a helper that takes just the name:

```rust
// In facet crate:
impl ValueVTable {
    pub const fn named(name: &'static str) -> ValueVTableBuilder {
        Self::builder(|f, _| ::core::fmt::Write::write_str(f, name))
    }
}

// In generated code:
::facet::ValueVTable::named("Struct000")
```

Or even shorter with the ʬ prelude:

```rust
VT::n("Struct000")
```

**Estimated savings**: ~25 tokens per type

---

## 20. Static Empty Slices

**Current output**:

```rust
::facet::Field {
    name: "field_0",
    shape: || ...,
    offset: ...,
    attributes: &[],
    doc: &[],
}
::facet::Variant {
    name: "V0",
    discriminant: Some(0),
    attributes: &[],
    data: ...,
    doc: &[],
}
```

**Problem**: `&[]` appears twice per field and twice per variant. While the compiler
likely deduplicates these, it's still parsed and processed many times.

**Potential fix**: Use named constants:

```rust
// In facet crate:
pub const NO_ATTRS: &[Attribute] = &[];
pub const NO_DOC: &[&str] = &[];

// Or in ʬ prelude:
pub const A: &[Attribute] = &[];  // empty Attributes
pub const D: &[&str] = &[];       // empty Doc

// In generated code:
F {
    name: "field_0",
    shape: || ...,
    offset: ...,
    attributes: ʬ::A,
    doc: ʬ::D,
}
```

**Estimated savings**: ~4 tokens per field/variant (small but very frequent)

---

## 21. Validation Panic Simplification

**Current output** (in enum validation blocks):

```rust
fn __facet_construct_all_variants<'__facet>() -> Enum004 {
    loop {
        let _: Enum004 = Enum004::V1(
            ::core::panicking::panic("not yet implemented"),
        );
        let _: Enum004 = Enum004::V3 {
            f0: ::core::panicking::panic("not yet implemented"),
            f1: ::core::panicking::panic("not yet implemented"),
            f2: ::core::panicking::panic("not yet implemented"),
        };
    }
}
```

**Problem**: `::core::panicking::panic("not yet implemented")` is 50+ characters,
repeated for every field in every variant.

**Potential fix**: Use a shorter path or helper:

```rust
// In facet crate:
#[inline(always)]
pub fn __unreachable<T>() -> T { unreachable!() }

// In generated code:
let _: Enum004 = Enum004::V3 {
    f0: ::facet::__unreachable(),
    f1: ::facet::__unreachable(),
    f2: ::facet::__unreachable(),
};
```

Or with ʬ prelude:

```rust
f0: ʬ::x(),
f1: ʬ::x(),
```

Or even better - use `loop {}` which requires no arguments:

```rust
fn __validate<'a>() -> Enum004 {
    #[allow(unreachable_code)]
    loop {
        let _ = Enum004::V3 { f0: loop {}, f1: loop {}, f2: loop {} };
    }
}
```

**Estimated savings**: ~40 tokens per variant field

---

## Final Summary: All 21 Optimization Opportunities

| # | Optimization | Tokens Saved | Complexity |
|---|--------------|--------------|------------|
| 1 | VTable builder → struct literal | ~80/type | Medium |
| 2 | Remove dead code blocks | ~50-100/type | Low |
| 3 | Remove static SHAPE refs | ~20/type | Low |
| 4 | Shape struct update syntax | ~60/type | Low |
| 5 | Field struct update/constructor | ~15/field | Low |
| 6 | Shadow structs (limited) | ~10/variant | High |
| 7 | Remove redundant braces | ~20/type | Low |
| 8 | Use imports instead of FQ paths | ~100+/type | Medium |
| 9 | Remove redundant i64 casts | ~4/variant | Low |
| 10 | Repr::C const instead of fn call | ~8/variant | Low |
| 11 | PhantomData type alias | ~15/shadow struct | Low |
| 12 | Eliminate nested const blocks | ~15/type | Low |
| 13 | Unit variant StructType const | ~25/unit variant | Low |
| 14 | shape_of! macro for closures | ~10/field | Medium |
| **15** | **ʬ prelude module** | **~200+/type** | **Medium** |
| **16** | **Shorter shadow struct names** | **~30/shadow** | **Low** |
| **17** | **Shorter lifetime ('__facet → 'a)** | **~30/type** | **Low** |
| **18** | **Consolidated allow attributes** | **~15/shadow** | **Low** |
| **19** | **Type name writer helper** | **~25/type** | **Low** |
| **20** | **Static empty slices** | **~4/field** | **Low** |
| **21** | **Validation panic → loop {}** | **~40/variant field** | **Low** |

**Cumulative estimate for a crate with 50 structs (avg 5 fields) and 20 enums (avg 5 variants)**:
- Current: ~50,000+ tokens
- After all optimizations: ~15,000-20,000 tokens (**60-70% reduction**)

---

## Implementation Priority (Final)

### Tier 1: Quick Wins (change macro only)
1. Remove `i64 as i64` redundant casts
2. Remove redundant `{ }` braces
3. Remove `let mut vtable = ...; vtable` pattern
4. Shorter lifetime name (`'__facet` → `'a`)
5. Shorter shadow struct names
6. Use `loop {}` instead of `panic!` in validation
7. Eliminate nested const blocks

### Tier 2: Add Constants/Helpers to Facet Crate
8. Add `Shape::UNDEFINED`, `Field::EMPTY`, `Variant::EMPTY` constants
9. Add `StructType::UNIT` constant
10. Add `Repr::C` constant
11. Add `ValueVTable::named()` helper
12. Add `__Phantom<'a>` type alias
13. Add empty slice constants

### Tier 3: The ʬ Prelude (biggest win)
14. Create `::facet::ʬ` module with ultra-short re-exports
15. Update macro to use `use ::facet::ʬ::*;` preamble
16. Combine with struct update syntax for maximum compression

### Tier 4: Structural Changes
17. Consolidate allow attributes with wrapper macro
18. Consider if validation blocks can be simplified or removed
19. Investigate if static SHAPE refs are needed

---

## Next Steps

1. Locate the derive macro source code (likely in `facet-derive` or similar)
2. Identify the code generation functions for each pattern above
3. Implement changes incrementally, measuring compile time impact
4. Consider adding a `bloat-check` CI job that monitors expanded code size
