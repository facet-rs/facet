# Facet vs Serde: Comprehensive Benchmark Report

**Generated:** 2025-12-06
**Schema:** 120 structs, 40 enums (deterministic, seed=42)

## Executive Summary

| Metric | Facet | Serde | Winner |
|--------|-------|-------|--------|
| Build time (release+json) | ~8s | ~7s | Serde |
| Binary size (release+json) | 1,472 KB | 561 KB | Serde (2.6x smaller) |
| Stripped size (release+json) | 1,188 KB | 465 KB | Serde (2.6x smaller) |
| LLVM IR lines | 299,267 | 22,925 | Serde (13x fewer) |
| Monomorphized copies | 8,363 | 984 | Serde (8.5x fewer) |

## Build Configuration

- **Toolchain:** Stable Rust
- **Profile:** Debug and Release
- **JSON variants:** with and without `--json` flag
- **Schema:** Auto-generated synthetic types from `cargo xtask schema`

## Binary Size Comparison

### Debug Builds

| Configuration | Binary Size | Stripped Size |
|--------------|-------------|---------------|
| **Facet (no JSON)** | 11,054 KB | 349 KB |
| **Serde (no JSON)** | 3,902 KB | 345 KB |
| **Facet + JSON** | 23,257 KB | 2,433 KB |
| **Serde + JSON** | 6,349 KB | 741 KB |

### Release Builds

| Configuration | Binary Size | Stripped Size |
|--------------|-------------|---------------|
| **Facet (no JSON)** | 406 KB | 341 KB |
| **Serde (no JSON)** | 406 KB | 341 KB |
| **Facet + JSON** | 1,472 KB | 1,188 KB |
| **Serde + JSON** | 561 KB | 465 KB |

### Analysis

- **Debug builds (no JSON):** Facet produces 2.8x larger binaries than Serde (11 MB vs 3.9 MB)
- **Debug builds (with JSON):** Facet produces 3.7x larger binaries (23 MB vs 6.3 MB)
- **Release builds (no JSON):** Identical sizes due to aggressive dead code elimination
- **Release builds (with JSON):** Facet is 2.6x larger (1.47 MB vs 561 KB)

The release builds without JSON have identical sizes because the types aren't actually used for serialization - only `Default::default()` and `black_box()` are called, which get optimized out.

## Build Time Comparison

| Configuration | Facet | Serde |
|---------------|-------|-------|
| Debug (no JSON) | 6s | 7s |
| Debug (with JSON) | 7s | 7s |
| Release (no JSON) | 6s | 6s |
| Release (with JSON) | 8s | 7s |

Build times are roughly comparable, with minor variations likely due to:
- Different proc-macro implementations (`unsynn` vs `syn`)
- Different dependency graphs

## LLVM IR Analysis (`cargo llvm-lines`)

### Total Counts (Release + JSON)

| Metric | Facet | Serde | Ratio |
|--------|-------|-------|-------|
| **Total LLVM IR Lines** | 299,267 | 22,925 | **13.1x** |
| **Monomorphized Copies** | 8,363 | 984 | **8.5x** |

### Top Contributors - Facet

| Lines | Copies | Function |
|-------|--------|----------|
| 63,963 (21.4%) | 2,092 | `core::ops::function::FnOnce::call_once` |
| 16,889 (5.6%) | 427 | `Option<T>::SHAPE` (constant closure) |
| 13,373 (4.5%) | 539 | `Vec<T>::SHAPE` (constant closure) |
| 10,975 (3.7%) | 176 | `Option<T>::SHAPE` (closure) |
| 10,244 (3.4%) | 370 | `core::mem::transmute_copy` |
| 8,852 (3.0%) | 248 | `facet_core::ptr::PtrUninit::put` |
| 6,958 (2.3%) | 98 | `Vec<T>::SHAPE` (closure) |

### Top Contributors - Serde

| Lines | Copies | Function |
|-------|--------|----------|
| 1,314 (5.7%) | 3 | `deserialize_struct` |
| 1,154 (5.0%) | 5 | `deserialize_number` |
| 1,117 (4.9%) | 1 | `Struct002::visit_map` |
| 1,033 (4.5%) | 12 | `SeqAccess::next_element_seed` |
| 804 (3.5%) | 12 | `SerializeMap::serialize_value` |

### Observations

1. **Facet's monomorphization explosion:** The `FnOnce::call_once` alone accounts for 21% of all LLVM IR lines with 2,092 copies
2. **Shape constants:** Facet's type shape infrastructure (`Option<T>::SHAPE`, `Vec<T>::SHAPE`) generates substantial code
3. **Serde's efficiency:** Most serde functions have very few copies (1-12), indicating better code sharing

## Interpretation

### Why Facet produces more code

1. **Runtime reflection:** Facet builds type shapes at compile time that enable runtime introspection, requiring more generated code per type
2. **Generic infrastructure:** Functions like `PtrUninit::put`, `transmute_copy`, and the shape closures get monomorphized for every type
3. **Trade-off:** This code enables features like dynamic field access and format-agnostic serialization

### When this matters

- **WASM/embedded:** Larger binaries may be problematic for size-constrained targets
- **Build times at scale:** With thousands of types, the monomorphization cost adds up
- **Cold caches:** More code means more instruction cache misses

### When this doesn't matter

- **Server applications:** Binary size is rarely a concern
- **Developer productivity:** If Facet enables better abstractions, the trade-off may be worthwhile
- **Runtime performance:** More monomorphization can mean better-optimized code paths

## Methodology

```bash
# Generate schema
cargo xtask schema

# Build all variants
cargo build -p facet-bloatbench --features facet --release
cargo build -p facet-bloatbench --features serde --release
cargo build -p facet-bloatbench --features facet,json --release
cargo build -p facet-bloatbench --features serde,json --release

# LLVM lines analysis
cargo llvm-lines -p facet-bloatbench --lib --features facet,json --release
cargo llvm-lines -p facet-bloatbench --lib --features serde,json --release
```

## Raw Data

### Facet LLVM-Lines (Top 40)

```
  Lines                 Copies              Function name
  -----                 ------              -------------
  299267                8363                (TOTAL)
   63963 (21.4%, 21.4%) 2092 (25.0%, 25.0%) core::ops::function::FnOnce::call_once
   16889 (5.6%, 27.0%)   427 (5.1%, 30.1%)  facet_core::impls_core::option::<impl facet_core::Facet for core::option::Option<T>>::SHAPE::{{constant}}::{{constant}}::{{closure}}
   13373 (4.5%, 31.5%)   539 (6.4%, 36.6%)  facet_core::impls_alloc::vec::<impl facet_core::Facet for alloc::vec::Vec<T>>::SHAPE::{{constant}}::{{constant}}::{{closure}}
   10975 (3.7%, 35.2%)   176 (2.1%, 38.7%)  facet_core::impls_core::option::<impl facet_core::Facet for core::option::Option<T>>::SHAPE::{{constant}}::{{closure}}
   10244 (3.4%, 38.6%)   370 (4.4%, 43.1%)  core::mem::transmute_copy
    8852 (3.0%, 41.5%)   248 (3.0%, 46.1%)  facet_core::ptr::PtrUninit::put
    6958 (2.3%, 43.9%)    98 (1.2%, 47.2%)  facet_core::impls_alloc::vec::<impl facet_core::Facet for alloc::vec::Vec<T>>::SHAPE::{{constant}}::{{closure}}
    5500 (1.8%, 45.7%)   250 (3.0%, 50.2%)  facet_core::ptr::PtrMut::drop_in_place
    4732 (1.6%, 47.3%)   145 (1.7%, 52.0%)  facet_core::shape_util::vtable_for_list::{{closure}}
    4112 (1.4%, 48.7%)   128 (1.5%, 53.5%)  facet_core::ptr::PtrConst::get
    3796 (1.3%, 49.9%)     1 (0.0%, 53.5%)  facet_json::deserialize::JsonDeserializer<_,A>::deserialize_dynamic_value
    3435 (1.1%, 51.1%)     1 (0.0%, 53.5%)  facet_reflect::partial::partial_api::misc::<impl facet_reflect::partial::Partial<_>>::end
    3222 (1.1%, 52.1%)     2 (0.0%, 53.5%)  alloc::collections::btree::node::BalancingContext<K,V>::bulk_steal_left
    3190 (1.1%, 53.2%)     1 (0.0%, 53.5%)  facet_json::deserialize::JsonDeserializer<_,A>::deserialize_struct_with_flatten
    3179 (1.1%, 54.3%)   289 (3.5%, 57.0%)  <core::marker::PhantomData<T> as facet_core::typeid::of::NonStaticAny>::get_type_id
    3063 (1.0%, 55.3%)   339 (4.1%, 61.1%)  core::ptr::read_unaligned
    2924 (1.0%, 56.3%)     2 (0.0%, 61.1%)  alloc::collections::btree::node::BalancingContext<K,V>::do_merge
    2923 (1.0%, 57.2%)     1 (0.0%, 61.1%)  facet_json::deserialize::JsonDeserializer<_,A>::deserialize_enum
```

### Serde LLVM-Lines (Top 40)

```
  Lines                Copies             Function name
  -----                ------             -------------
  22925                984                (TOTAL)
   1314 (5.7%,  5.7%)    3 (0.3%,  0.3%)  <&mut serde_json::de::Deserializer<R> as serde_core::de::Deserializer>::deserialize_struct
   1154 (5.0%, 10.8%)    5 (0.5%,  0.8%)  serde_json::de::Deserializer<R>::deserialize_number
   1117 (4.9%, 15.6%)    1 (0.1%,  0.9%)  Struct002::visit_map
   1033 (4.5%, 20.1%)   12 (1.2%,  2.1%)  <serde_json::de::SeqAccess<R> as serde_core::de::SeqAccess>::next_element_seed
    804 (3.5%, 23.7%)   12 (1.2%,  3.4%)  <serde_json::ser::Compound<W,F> as serde_core::ser::SerializeMap>::serialize_value
    655 (2.9%, 26.5%)    1 (0.1%,  3.5%)  serde_json::de::Deserializer<R>::ignore_value
    621 (2.7%, 29.2%)   13 (1.3%,  4.8%)  <serde_json::de::MapAccess<R> as serde_core::de::MapAccess>::next_value_seed
    593 (2.6%, 31.8%)    1 (0.1%,  4.9%)  Struct000::visit_map
    585 (2.6%, 34.4%)    1 (0.1%,  5.0%)  Struct001::visit_map
    546 (2.4%, 36.7%)    2 (0.2%,  5.2%)  deserialize_seq
    462 (2.0%, 38.8%)    1 (0.1%,  5.3%)  Struct002::visit_seq
    357 (1.6%, 40.3%)    1 (0.1%,  5.4%)  facet_bloatbench::serde_json_roundtrip
```

---

## Root Cause Analysis

### The Triple Monomorphization Problem

The core issue is a combination of three factors:

1. **Generic Type Parameter**: Each `Option<T>`, `Vec<T>`, etc. is a unique monomorphization
2. **Closure Captures**: Each closure captures `T::SHAPE`, which is type-specific
3. **Const Block Lifetime Promotion**: To put closures in `&'static` positions, Rust must evaluate the const block, which requires instantiating all generic code

### Primary Bloat Sources

#### 1. Option<T>::SHAPE Closures (10.2% of IR)

**File:** `facet-core/src/impls_core/option.rs`

The `Option<T>` implementation defines **6+ conditional closures inside a const block**:

```rust
const SHAPE: &'static Shape = &const {
    let mut vtable = value_vtable!(...);
    vtable.debug = if T::SHAPE.is_debug() {
        Some(|this, f| { /* Debug impl */ })
    } else { None };
    vtable.hash = if T::SHAPE.is_hash() {
        Some(|this, hasher| { /* Hash impl */ })
    } else { None };
    // ... repeat for partial_eq, partial_ord, ord, parse, try_from/into/borrow
};
```

Each closure gets monomorphized for every `Option<T>` instantiation, creating:
- 427 copies of inner const closures
- 176 copies of outer closures
- ~28 IR lines per copy

#### 2. Vec<T>::SHAPE Closures (6.8% of IR)

**File:** `facet-core/src/impls_alloc/vec.rs`

Similar pattern to Option, plus calls to `vtable_for_list::<T, Self>()` which generates additional closures for list operations (push, get, iter, etc.).

#### 3. FnOnce::call_once Explosion (21.4% of IR)

**Root cause:** Every closure in Rust generates an `impl FnOnce`. With 6+ closures per Option/Vec × ~160 types = 2,000+ unique closures, each requiring its own `FnOnce::call_once` instance.

#### 4. Pointer Utilities (7.4% of IR)

**File:** `facet-core/src/ptr.rs`

`PtrUninit::put` and `transmute_copy` are called from every type's initialization code:

```rust
pub const unsafe fn put<T>(self, value: T) -> PtrMut<'mem> {
    core::ptr::write(self.ptr.to_ptr::<T>(), value);
    self.assume_init()
}
```

With 370 copies of `transmute_copy` and 248 copies of `put`, these small functions contribute significant bloat.

---

## Proposed Solutions

### High Impact

#### 1. Replace Closures with Function Pointers

Instead of conditional closures:
```rust
// Current (bloated)
vtable.debug = if T::SHAPE.is_debug() {
    Some(|this, f| { /* impl */ })
} else { None };
```

Use extern functions:
```rust
// Proposed
unsafe extern "C" fn debug_option<T: Facet>(ptr: PtrConst, f: &mut Formatter) -> Result { ... }

vtable.debug = if T::SHAPE.is_debug() {
    Some(debug_option::<T>)
} else { None };
```

**Impact:** Reduces closure overhead; single monomorphization per type instead of per-closure.

#### 2. Gate Comparison/Hash Behind Feature Flags

```rust
#[cfg(feature = "facet-cmp")]
pub struct CmpVTable { partial_eq: Option<...>, ... }

#[cfg(not(feature = "facet-cmp"))]
pub struct CmpVTable; // ZST - zero code generated
```

**Impact:** Users who don't need comparison traits save ~30% binary size.

#### 3. Type-Erase Collection Implementations

Create shared implementations that don't require full monomorphization:

```rust
// Instead of full generic impl
pub struct ListShapeImpl<T: Facet> { ... }

// Use type-erased core with thin generic wrapper
struct ListShapeCore { /* type-erased operations */ }
pub struct ListShapeImpl<T> { core: &'static ListShapeCore, _t: PhantomData<T> }
```

**Impact:** Single implementation for list operations, parametric over T.

### Medium Impact

#### 4. Hoist Conditional Logic Out of Const Blocks

Move closure creation to regular functions that can be better optimized:

```rust
fn make_option_vtable<'a, T: Facet<'a>>() -> ValueVTable {
    ValueVTable {
        debug: if T::SHAPE.is_debug() { Some(/* closure */) } else { None },
        ...
    }
}

const SHAPE: &'static Shape = &const {
    let vtable = make_option_vtable::<T>();
    ...
};
```

#### 5. Inline vtable_for_list

Replace the helper function with direct struct construction to avoid the extra monomorphization boundary.

### Estimated Reduction

| Optimization | Current | Est. Reduction |
|--------------|---------|----------------|
| Function pointers vs closures | 2,092 FnOnce copies | -20% |
| Feature-gate CmpVTable | 1,240 copies | -30% (if disabled) |
| Type-erase list helpers | 637 copies | -40% |
| Hoist conditional logic | - | -15% |
| **Total** | 299K lines | **-20-30%** |

**Realistic Target:** Reduce from 13x to ~8-10x of Serde's code size.
