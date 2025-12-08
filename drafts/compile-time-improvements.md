# Compile Time Improvement Ideas

## Current Timing Breakdown (facet-bloatbench with facet,facet_json)

| Crate | Time | Notes |
|-------|------|-------|
| facet-bloatbench | 3.68s | 80 structs with `#[derive(Facet)]` |
| facet-macros-impl | 1.63s | The proc-macro implementation |
| facet-core | 1.03s | Core types with HRTB in VTable |
| facet-json | 0.71s | |
| facet-reflect | 0.68s | |

## Issue 1: HRTB in VTable (affects facet-core, all consumers)

**Problem**: Every VTable field has `for<'a>` higher-ranked trait bounds:

```rust
pub struct VTable<S: VTableStyle> {
    pub display: Option<for<'a> unsafe fn(S::Receiver<'a>, &mut fmt::Formatter<'_>) -> S::Output<fmt::Result>>,
    // ... 13 more fields
}
```

**Impact**: Complex trait resolution × 14 fields × every type.

**Solution**: See `vtable-no-hrtb.md` - use unlifetimed `OxPtr`/`OxPtrMut` for vtable signatures.

## Issue 2: Nested Const Blocks (affects all derived types)

**Problem**: Each derived type has 3-4 levels of nested `const { }` blocks:

```rust
const SHAPE: &'static Shape = &const {           // Level 1
    ShpB::for_sized::<Self>("Struct000")
        .vtable(const {                          // Level 2
            VtE::Direct(&const {                 // Level 3
                VtD::builder_for::<Struct000>()
                    .drop_in_place(...)
                    .build()
            })
        })
        .ty(Ty::User(UTy::Struct(
            STyB::new(Sk::Struct, &const {       // Level 4
                [/* fields */]
            })
        )))
        .build()
};
```

**Impact**: Each const block requires separate const-evaluation by rustc. Nested const blocks compound the overhead.

**Potential Solutions**:

1. **Flatten const blocks**: Move vtable and fields to separate `const` items:
   ```rust
   const VTABLE: VTableDirect = VtD::builder_for::<T>()...build();
   const FIELDS: &[Field] = &[...];
   const SHAPE: &Shape = &ShpB::for_sized::<T>("T")
       .vtable_direct(&VTABLE)
       .ty(Ty::User(UTy::Struct(STyB::new(Sk::Struct, FIELDS).build())))
       .build();
   ```

2. **Use statics where possible**: `static` items are lazily initialized, `const` items are inlined at every use site. For the `SHAPE` reference, `static` might work if we can ensure single-address semantics.

## Issue 3: Per-Field Builder Chains

**Problem**: Each field goes through a builder pattern:

```rust
𝟋FldB::new("field_0", <T as Facet>::SHAPE, offset_of(...))
    .build()
```

With O(fields) builder invocations per struct.

**Potential Solution**: Direct `Field` construction without builder:

```rust
Field {
    name: "field_0",
    shape: <T as Facet>::SHAPE,
    offset: offset_of(...),
    flags: FieldFlags::empty(),
    attributes: &[],
    // ...
}
```

This trades ergonomics for compile speed. The builder is nice for optional fields, but most fields don't use them.

## Issue 4: VTableStyle Associated Types

**Problem**: The `VTableStyle` trait with GATs adds type-level indirection:

```rust
pub trait VTableStyle {
    type Receiver<'a>;
    type ReceiverMut<'a>;
    type Output<Ret>;
}
```

Every VTable field projects through these, adding trait resolution overhead.

**Solution**: Remove `VTableStyle`, have separate `VTableDirect` and `VTableIndirect` structs with concrete types. (Part of `vtable-no-hrtb.md`)

## Issue 5: `impls!` Macro for Auto-Traits

**Problem**: When `#[facet(auto_traits)]` is used, the macro generates specialization-based trait detection:

```rust
if impls!(MyType: Display) {
    Some(display_fn)
} else {
    None
}
```

This generates multiple impl blocks per trait check.

**Current Mitigation**: The trait detection is already layered:
1. Detect from `#[derive(...)]` - fast, no specialization
2. Declared via `#[facet(traits(...))]` - fast, no specialization
3. Auto-detection - slow, uses `impls!`

**Recommendation**: Document that `auto_traits` is slow and encourage explicit trait declaration.

## Issue 6: Repeated Type Projections

**Problem**: Each field access does `<FieldType as Facet>::SHAPE`, which requires trait resolution.

```rust
𝟋FldB::new("field_0", <Vec<Cow<'static, str>> as 𝟋Fct>::SHAPE, ...)
```

For complex generic types like `Vec<Cow<'static, str>>`, this triggers `Facet` impl resolution.

**Potential Solution**: None obvious - this is inherent to the design. The projections are necessary.

## Issue 7: Macro-Generated Code Volume

**Current**: ~255 lines of expanded code per struct (for simple structs without auto_traits).

**Breakdown**:
- `static SHAPE_REF` - 1 line
- `const _: () = { fn check... }` - 5 lines (use check)
- `unsafe impl Facet` with nested const - ~50+ lines
- Standard derives (Default) - varies

**Potential Optimizations**:

1. **Remove use-check boilerplate**: The `const _: () = { fn __facet_use_struct... }` is for error messages. Consider making it optional or cfg'd.

2. **Compress field generation**: Current format is very verbose. Could use a more compact representation.

## Quick Wins (Low-Hanging Fruit)

1. **Remove HRTB** - High impact, moderate effort (see vtable-no-hrtb.md)
2. **Flatten const blocks** - Medium impact, low effort
3. **Direct Field construction** - Low impact per field, but O(fields) savings
4. **Document auto_traits cost** - Zero code change, helps users

## Measurements Needed

To validate these ideas:

1. **Isolate const-eval time**: Use `-Z time-passes` or similar to measure const-eval specifically
2. **Compare with/without HRTB**: Create a branch removing HRTB, measure compile time delta
3. **Profile trait resolution**: Check if the bottleneck is trait resolution vs const-eval
4. **Compare nested vs flat const blocks**: Benchmark the difference

## Issue 8: Per-Type Static SHAPE Reference

**Problem**: Each derived type generates a static reference alongside the impl:

```rust
static STRUCT000_SHAPE: &'static ::facet::Shape = <Struct000 as ::facet::Facet>::SHAPE;
```

This adds a symbol per type and triggers `<T as Facet>::SHAPE` resolution.

**Potential Solution**: Consider making this opt-in or removing it entirely. The `SHAPE` is already accessible via `<T as Facet>::SHAPE`.

## Issue 9: Dead Code Suppression Boilerplate

**Problem**: Each type generates a use-check function (from facet-macros-impl/src/process_struct.rs):

```rust
const _: () = {
    #[allow(dead_code, clippy::multiple_bound_locations)]
    fn __facet_use_struct<'ʄ>(__v: &Struct000) {
        let _ = __v;
    }
};
```

**Purpose**: Suppresses dead code warnings for structs constructed via reflection (see issue #996). When structs are built via `facet_args::from_std_args()` etc., the compiler doesn't see them being used directly.

**Potential Solution**: Could make this opt-in, but it's probably not a significant compile-time cost - it's a trivial function that likely gets optimized away.

## Comparative Analysis: Simple vs Complex Types

For a simple struct with 3 fields and no special attributes:
- ~70 lines of expanded code
- 1 `const SHAPE` with ~4 nested const blocks
- 1 static reference
- 1 use-check
- 3 field builder calls

For a struct with `#[facet(auto_traits)]`:
- ~250+ lines of expanded code
- Each trait generates an `unsafe fn` with `impls!()` check
- The `impls!` macro expands to specialization boilerplate per trait
- 10+ traits checked = 10+ specialization expansions

**Recommendation**: The `auto_traits` path is known to be slow. Users should be strongly guided toward using `#[derive(Debug, Clone, ...)]` with Facet, which gives the macro direct visibility into implemented traits.

## Type Size Analysis

Using `-Z print-type-sizes` on nightly:

| Type | Size | Notes |
|------|------|-------|
| **Shape** | 304 bytes | The big one - per-type metadata |
| **Field** | 152 bytes | Per-field metadata |
| **Variant** | 96 bytes | Per-enum-variant metadata |
| VTableDirect | 112 bytes | 14 Option<fn ptr> × 8 bytes |
| VTableIndirect | 112 bytes | Same structure |
| **VTableErased** | 16 bytes | Enum with two `&'static` variants |
| Type | 48 bytes | Type category enum |
| Def | 48 bytes | Definition enum |
| TypeOps | 24 bytes | 3 Option<fn ptr> |
| StructType | 32 bytes | |
| EnumType | 32 bytes | |

### Shape Breakdown (304 bytes)

```
.id: 8 bytes (ConstTypeId)
.layout: 16 bytes (size + align)
.vtable: 16 bytes (VTableErased - enum discriminant + ptr)
.type_ops: 8 bytes
.marker_traits: 1 byte
padding: 7 bytes
.ty: 48 bytes (Type enum)
.def: 48 bytes (Def enum)
.type_identifier: 16 bytes (&str)
.type_params: 16 bytes (&[TypeParam])
.doc: 16 bytes (&[&str])
.attributes: 16 bytes (&[Attr])
.type_tag: 16 bytes (Option<&str>)
.inner: 8 bytes (Option<&Shape>)
.type_name: 8 bytes (Option<fn>)
.proxy: 8 bytes (Option<&ProxyDef>)
.variance: 8 bytes (fn ptr)
.flags: 2 bytes (ShapeFlags)
padding: 6 bytes
.tag: 16 bytes (Option<&str>)
.content: 16 bytes (Option<&str>)
```

### VTableErased Niche Optimization

`VTableErased` is 16 bytes (8 byte pointer + 8 byte discriminant), not 8 bytes.

**Why?** Niche optimization only works when there's an invalid bit pattern to encode the discriminant:
- `Option<&T>` → 8 bytes (null = None)
- `enum { A(&T), B(&T) }` → 16 bytes (no niche - both need valid pointers!)

To make `VTableErased` 8 bytes, would need pointer tagging (steal a low bit since VTable is aligned). Since `VTable` has at least 8-byte alignment, the low 3 bits are always 0 and could store a discriminant.

```rust
// Current: 16 bytes
pub enum VTableErased {
    Direct(&'static VTableDirect),
    Indirect(&'static VTableIndirect),
}

// Potential: 8 bytes with pointer tagging
pub struct VTableErased(TaggedPtr); // bit 0: 0=Direct, 1=Indirect
```

Savings: 8 bytes per Shape × N types. For 80 types = 640 bytes.

### Shape Size Reduction Ideas

Shape is 304 bytes. For 80 types, that's ~24KB of static data just for Shape structs (not counting Field arrays, VTables, etc.).

**Potential reductions:**

1. **Pointer-tag VTableErased** (16 → 8 bytes): Save 8 bytes
2. **Compress Option<&str> fields**: `type_tag`, `tag`, `content` are 16 bytes each but rarely used. Could use indices into a string table.
3. **Merge ty + def**: Both are 48-byte enums. Some combinations are invalid. Could use a single enum.
4. **Remove padding**: Reorder fields to minimize padding (currently 13 bytes wasted)
5. **Use NonZero for id**: Could enable niche optimization in Option<&Shape>
6. **Lazy type_name**: Currently 8 bytes for Option<fn>. Rarely used.

Realistically, Shape could probably be shrunk to ~200-220 bytes with effort.

## Binary Size Analysis

### ⚠️ CRITICAL: vtable-code-sharing branch is LARGER than main!

| Binary | Size | vs Main |
|--------|------|---------|
| **main (facet)** | 409KB | baseline |
| **serde** | 599KB | +46% |
| **vtable-code-sharing** | 1.6MB | **+291%** |

**The refactor made binaries 4x larger, not smaller!**

### ELF Section Comparison: main vs vtable-code-sharing

| Section        | Main (old) | vtable (new) | Delta     | Change    |
|----------------|------------|--------------|-----------|-----------|
| .text (code)   | 248,981    | 917,772      | +668,791  | 3.7x MORE |
| .rodata        | 22,166     | 103,049      | +80,883   | 4.7x MORE |
| .data.rel.ro   | 9,640      | 84,048       | +74,408   | 8.7x MORE |
| .eh_frame      | 22,456     | 86,080       | +63,624   | 3.8x MORE |
| .rela.dyn      | 18,216     | 86,712       | +68,496   | 4.8x MORE |
|----------------|------------|--------------|-----------|-----------|
| **TOTAL**      | 338,110    | 1,321,506    | +983,396  | **3.9x MORE** |

### Where Did the Bloat Come From?

1. **`.data.rel.ro` +74KB (8.7x)**: More static Shape/Field/VTable data with relocations
2. **`.text` +669KB (3.7x)**: More code - possibly more vtable function implementations?
3. **`.rodata` +81KB (4.7x)**: More read-only data (strings, type names?)

### Comparison: vtable-code-sharing vs serde

| Section        | Facet vtable | Serde      | Diff       | Notes                    |
|----------------|--------------|------------|------------|--------------------------|
| .text (code)   | 917,772      | 348,396    | +569,376   | 2.6x more code           |
| .rodata        | 103,049      | 49,829     | +53,220    | 2.1x more read-only data |
| .data.rel.ro   | 84,048       | 14,016     | +70,032    | 6x more relocatable!     |
| .eh_frame      | 86,080       | 32,336     | +53,744    | 2.7x more unwind info    |

### Largest Functions (vtable branch)

- `deserialize_enum`: ~22KB
- `deserialize_struct`: ~22KB
- `deserialize_dynamic_value`: ~20KB
- `serialize_value`: ~13KB
- `Partial::end`: ~9KB

**Note**: These are **single functions** (not per-type monomorphizations) - the reflection-based approach is working. Each function is large because it handles all cases inline.

### Why LLVM IR Down, Compile Time Up

The LLVM IR went *down* while compile time went *up*, suggesting:
- Less codegen (good for binary size)
- More front-end work (type checking, const eval)

This is the expected tradeoff from the vtable refactor - it shifts work from LLVM to rustc.

### Opportunities for Further Size Reduction

1. **JSON deserializer monomorphization**: The deserialize functions are large because they handle all cases (structs, enums, maps, etc.) inline. Could potentially split into smaller specialized functions.

2. **Shape data deduplication**: Many shapes share similar structure. Could explore interning or compression.

3. **Feature-gate verbose error messages**: Some size comes from error message strings.

## Progress Update (2025-12-09)

### ✅ COMPLETED - Significant Wins!

| Optimization | Status | Commit |
|-------------|--------|--------|
| **Hoisted `__SHAPE_DATA` const** | ✅ Done | `7b73183f3` |
| **Direct Field construction** | ✅ Done | `a5d266f52` |
| **Lazy field shape refs** (`fn() -> &'static Shape`) | ✅ Done | `ed78ed5d0` |
| **Removed static SHAPE refs** | ✅ Done | `5449ff1ae` |
| **Flattened const blocks** | ✅ Done | `5449ff1ae` |
| **VTable/TypeOps split** | ✅ Done | `ce8a2769` |

**What these changes did:**
- `__SHAPE_DATA` is now a const in an inherent impl, not inside `&const {}`
- Field shapes are now `fn() -> &'static Shape` instead of `&'static Shape` (lazy evaluation)
- No more per-type `static FOO_SHAPE: &Shape = ...` boilerplate
- `__FIELDS` and `__VTABLE` hoisted out of nested const blocks

**Current generated code structure:**
```rust
impl Struct000 {
    const __FIELDS: &[Field] = &[...];
    const __VTABLE: VTableDirect = ...;
    const __SHAPE_DATA: Shape = ShapeBuilder::for_sized::<Self>("Struct000")
        .vtable(...)
        .ty(...)
        .build();
}

unsafe impl Facet<'_> for Struct000 {
    const SHAPE: &'static Shape = &Self::__SHAPE_DATA;
}
```

### Current Results (facet_json vs serde_json, separate features)

| Build | facet_json | serde_json | Winner |
|-------|------------|------------|--------|
| **Debug** | 5.24s | 5.95s | **facet 12% faster** |
| **Release** | 2.12s | 3.57s | **facet 40% faster** |

facet already beats serde on compile time! But there's room for more improvement.

### Remaining Opportunities

1. ~~**VTable/TypeOps split**~~ ✅ **DONE** (`ce8a2769`)

   VTableDirect and VTableIndirect no longer have `drop_in_place`, `default_in_place`, `clone_into`.
   These are now in `TypeOpsDirect` / `TypeOpsIndirect` on Shape.

   **Results:**
   - Compile time: 15.02s (similar to lazy-shape-fn baseline of 15.06s)
   - Binary size: 1577 KB unstripped, 1246 KB stripped (down from 1633KB - **3.4% smaller**)
   - LLVM lines: 122402 (down from 122646)
   - LLVM copies: 2328 (down from 2379 - fewer monomorphizations)

2. **Update facet-core impls** (~125ms savings)
   - 166 occurrences of `&const {}` across 50 files in `facet-core/src/impls/`
   - Could apply same lazy `fn() -> &'static Shape` pattern
   - **Effort**: High (many files to touch)

   **Current pattern** (e.g., `impls/core/scalar.rs`):
   ```rust
   unsafe impl Facet<'_> for bool {
       const SHAPE: &'static Shape = &const {
           const VTABLE: VTableDirect = vtable_direct!(bool => ...);
           ShapeBuilder::for_sized::<bool>("bool")
               .vtable_direct(&VTABLE)
               .build()
       };
   }
   ```

   **Could become**:
   ```rust
   fn bool_shape() -> &'static Shape {
       static SHAPE: Shape = /* ... */;
       &SHAPE
   }

   unsafe impl Facet<'_> for bool {
       const SHAPE: fn() -> &'static Shape = bool_shape;
   }
   ```

   **For generic types** (e.g., `Option<T>`, `Vec<T>`), use generic functions:
   ```rust
   // Current (impls/core/option.rs):
   unsafe impl<'a, T: Facet<'a>> Facet<'a> for Option<T> {
       const SHAPE: &'static Shape = &const {
           // ... complex nested const blocks with T::SHAPE references
       };
   }

   // Could become:
   fn option_shape<'a, T: Facet<'a>>() -> &'static Shape {
       static SHAPE: OnceLock<Shape> = OnceLock::new();
       SHAPE.get_or_init(|| {
           // Build shape using T::SHAPE() - now a function call
       })
   }

   unsafe impl<'a, T: Facet<'a>> Facet<'a> for Option<T> {
       const SHAPE: fn() -> &'static Shape = option_shape::<T>;
   }
   ```

   **Note**: Generic shapes can't use plain `static` because each `T` needs its own shape.
   Need `OnceLock` or similar for lazy per-monomorphization initialization.

   **High-value targets** (heavily instantiated):
   - `Option<T>` - `impls/core/option.rs` (267 lines, complex)
   - `Vec<T>` - `impls/alloc/vec.rs` ✅ Already has `VEC_LIST_VTABLE` shared static!
   - `Box<T>` - `impls/alloc/boxed.rs`
   - `Arc<T>`, `Rc<T>` - `impls/alloc/arc.rs`, `impls/alloc/rc.rs`
   - `HashMap<K,V>`, `HashSet<T>` - `impls/std/hashmap.rs`, `impls/std/hashset.rs`
   - `Result<T,E>` - `impls/core/result.rs`
   - `Cow<'a, T>` - `impls/alloc/cow.rs`

   These generic types are instantiated many times per crate, so reducing their const-eval
   cost has multiplicative benefits.

   **⚠️ The VTable/TypeOps split is incomplete!**

   Shape already has TWO places for vtable-like things:
   - `shape.vtable: VTableErased` → `VTableDirect` or `VTableIndirect`
   - `shape.type_ops: Option<&'static TypeOps>`

   **TypeOps** is meant for per-T operations:
   ```rust
   pub struct TypeOps {
       pub drop_in_place: unsafe fn(PtrMut),
       pub default_in_place: Option<unsafe fn(PtrUninit) -> PtrMut>,
       pub clone_into: Option<unsafe fn(src: PtrConst, dst: PtrUninit) -> PtrMut>,
   }
   ```

   **BUT VTableIndirect ALSO has these same fields:**
   ```rust
   pub struct VTableIndirect {
       pub drop_in_place: Option<unsafe fn(OxPtrMut) -> Option<()>>,  // DUPLICATED!
       pub default_in_place: Option<unsafe fn(OxPtrMut) -> Option<()>>,  // DUPLICATED!
       pub clone_into: Option<unsafe fn(OxPtrConst, OxPtrMut) -> Option<()>>,  // DUPLICATED!
       // ... plus display, debug, hash, partial_eq, etc.
   }
   ```

   **The fix**: Remove `drop_in_place`, `default_in_place`, `clone_into` from VTableIndirect.
   These belong in TypeOps (per-T). VTableIndirect should only have the type-erased trait impls
   that can be shared across all `Option<T>`, `Vec<T>`, etc.

   Then Option can have:
   ```rust
   // Shared across ALL Option<T>
   static OPTION_VTABLE: VTableIndirect = VTableIndirect::builder()
       .display(option_display)
       .debug(option_debug)
       .hash(option_hash)
       .partial_eq(option_partial_eq)
       .partial_cmp(option_partial_cmp)
       .cmp(option_cmp)
       .build();

   // Per-T (in shape.type_ops)
   const fn option_type_ops<T>() -> TypeOps {
       TypeOps::builder()
           .drop_in_place(option_drop::<T>)
           .default_in_place(option_default::<T>)
           .build()
   }
   ```

   Vec already does vtable sharing for `ListVTable` - see `VEC_LIST_VTABLE` static.
   Same pattern should apply to `Arc<T>`, `Rc<T>`, `Box<T>`, `Result<T,E>`, etc.

2. **Replace StructTypeBuilder with struct literal** (~small savings)
   - `STyB::new(...).repr(...).build()` → `StructType { repr, kind, fields }`
   - `StructType` only has 3 fields: `repr`, `kind`, `fields`
   - **Location**: `facet-macros-impl/src/process_struct.rs:1487,1497`
   - **Effort**: Low - straightforward replacement

3. **ShapeBuilder** - KEEP AS-IS
   - Has ~30 methods, uses `..EMPTY_VESSEL` for defaults
   - `build()` has logic to infer `ty` from `def`
   - Too complex to replace with struct literal
   - **Effort**: Would require rewriting all call sites

4. **VTableDirect builder** - KEEP AS-IS
   - `TypedVTableDirectBuilder` uses `transmute` for type-safe fn ptr conversion
   - Each method converts `fn(&T, ...)` → `unsafe fn(*const (), ...)`
   - Can't easily do this with struct literals
   - **Effort**: Would require unsafe blocks at each call site

5. **Lazy fields array** (~107ms potential savings)
   - `__FIELDS` array construction is ~107ms
   - Could make lazy like SHAPE, but offset_of! still needs const eval
   - **Effort**: Medium - need to change Field.shape from `fn()` to support lazy fields

### Easiest Wins: Replace Builders with Struct Literals

#### 1. StructTypeBuilder → StructType literal

**Location**: `facet-macros-impl/src/process_struct.rs:1487,1497` and `process_enum.rs:25`

**Current**:
```rust
𝟋STyB::new(#kind, &Self::__FIELDS).repr(#repr).build()
```

**Could become**:
```rust
𝟋STy {
    kind: #kind,
    fields: &Self::__FIELDS,
    repr: #repr,
}
```

`StructType` only has 3 fields (`facet-core/src/types/ty/struct_.rs:6-15`):
```rust
pub struct StructType {
    pub repr: Repr,
    pub kind: StructKind,
    pub fields: &'static [Field],
}
```

#### 2. VariantBuilder → Variant literal

**Location**: `facet-macros-impl/src/process_enum.rs:22-31, 47-52`

**Current**:
```rust
𝟋VarB::new(#name, struct_type)
    .discriminant(#discriminant)
    .attributes(#attrs)
    .doc(#doc)
    .build()
```

**Could become**:
```rust
𝟋Var {
    name: #name,
    discriminant: Some(#discriminant),
    attributes: #attrs,
    data: struct_type,
    doc: #doc,
}
```

`Variant` has 5 fields (`facet-core/src/types/ty/enum_.rs:20-37`):
```rust
pub struct Variant {
    pub name: &'static str,
    pub discriminant: Option<i64>,
    pub attributes: &'static [VariantAttribute],
    pub data: StructType,
    pub doc: &'static [&'static str],
}
```

Note: Unit variants already use `𝟋STy::UNIT` constant - good optimization already in place!

#### What to keep as builders

- **ShapeBuilder** - Too many fields (304 bytes!), has default logic in `build()`
- **VTableDirectBuilder** - Uses `transmute` for type-safe fn ptr conversion

#### Prelude aliases already available

```rust
// Already in facet-core/src/lib.rs prelude:
pub use crate::StructType as 𝟋STy;      // Use directly instead of STyB
pub use crate::Variant as 𝟋Var;         // Use directly instead of VarB
pub use crate::StructKind as 𝟋Sk;
pub use crate::Repr as 𝟋Repr;
```

#### Exact code locations to change

**process_struct.rs** - 2 occurrences:
- Line 1487: `𝟋STyB::new(#kind, &[]).repr(#repr).build()` (empty fields case)
- Line 1497: `𝟋STyB::new(#kind, &const {[#(#fields_vec),*]}).repr(#repr).build()` (non-empty fields)

**process_enum.rs** - 2 functions to modify:
- `gen_variant()` (lines 22-31): Uses `𝟋VarB::new(...).discriminant(...).build()`
- `gen_unit_variant()` (lines 47-52): Uses `𝟋VarB::new(#name, 𝟋STy::UNIT).discriminant(...).build()`

Both functions take optional `attributes` and `doc` parameters - when converting to struct literal,
use `&[]` as default when None:
```rust
// Current pattern:
let attributes_call = attributes.map(|a| quote! { .attributes(#a) });

// New pattern - always emit the field:
let attrs = attributes.unwrap_or(quote! { &[] });
```

## Summary of Actionable Items

### High Impact
1. ~~**Remove HRTB from VTable**~~ - See `vtable-no-hrtb.md` (still relevant)
2. ~~**Remove VTableStyle trait**~~ - Use concrete `VTableDirect`/`VTableIndirect` structs

### ✅ DONE - Medium Impact
3. ~~**Flatten nested const blocks**~~ - ✅ Done via hoisting
4. ~~**Direct Field construction**~~ - ✅ Done
5. ~~**Lazy shape references**~~ - ✅ Done via `fn() -> &'static Shape`

### Low Impact / Documentation
6. **Document auto_traits cost** - Guide users away from it
7. **Make use-check opt-in** - Reduce boilerplate for advanced users
8. ~~**Remove static SHAPE reference**~~ - It's redundant

### Needs Investigation
9. ~~**Profile const-eval specifically**~~ - ✅ Done with `-Z self-profile`
10. **Benchmark HRTB removal impact** - Create branch to test

## Profiling Commands

### Feature Combinations for Fair Comparison

```bash
# facet + facet-json (no serde) - THE FAIR COMPARISON
cargo build -p facet-bloatbench --features facet_json

# serde + serde_json (no facet) - THE FAIR COMPARISON
cargo build -p facet-bloatbench --features serde_json

# Both together (includes overhead from both)
cargo build -p facet-bloatbench --features json
```

### Collecting Self-Profile Data

```bash
# Clean and profile facet_json
rm -rf profile && mkdir -p profile
cargo clean -p facet-bloatbench
RUSTFLAGS="-Zself-profile=profile -Zself-profile-events=default,args" \
    cargo +nightly build -p facet-bloatbench --features facet_json

# Convert to chrome trace format
crox profile/facet_bloatbench-*.mm_profdata
gzip -f chrome_profiler.json

# Open in https://ui.perfetto.dev/
```

### Querying Profile Data with DuckDB

```bash
# Top items by time
zcat chrome_profiler.json.gz | duckdb -c "
SELECT name, count(*) as cnt, sum(dur)/1000 as total_ms
FROM read_json_auto('/dev/stdin')
GROUP BY name
ORDER BY total_ms DESC
LIMIT 20
"

# Const eval breakdown by source
zcat chrome_profiler.json.gz | duckdb -c "
SELECT
  CASE
    WHEN args.arg0 LIKE '%facet_core%' THEN 'facet_core'
    WHEN args.arg0 LIKE '%facet_bloatbench%' THEN 'bloatbench'
    ELSE 'other'
  END as source,
  COUNT(*) as count,
  SUM(dur)/1000 as total_ms
FROM read_json_auto('/dev/stdin')
WHERE name = 'eval_to_const_value_raw'
GROUP BY source
ORDER BY total_ms DESC
"
```
