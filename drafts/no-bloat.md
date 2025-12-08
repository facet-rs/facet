# VTable Migration Status

## Completed: facet-core

All files in `facet-core/src/impls/` have been migrated to the new VTable system.

### Patterns Used

| Pattern | Use Case | Example |
|---------|----------|---------|
| `vtable_direct!` | Sized types with compile-time known traits | `scalar.rs`, `nonzero.rs`, `path.rs` (PathBuf) |
| `vtable_indirect!` | Unsized types (str, Path, [T]) | `char_str.rs` (str), `path.rs` (Path) |
| `VTableIndirect::builder()` | Generic containers | `option.rs`, `vec.rs`, `hashmap.rs` |

### Files Migrated

```
facet-core/src/impls/
├── alloc/
│   ├── arc.rs ✓
│   ├── boxed.rs ✓
│   ├── btreemap.rs ✓
│   ├── btreeset.rs ✓
│   ├── cow.rs ✓
│   ├── rc.rs ✓
│   └── vec.rs ✓
├── core/
│   ├── array.rs ✓
│   ├── char_str.rs ✓
│   ├── fn_ptr.rs ✓
│   ├── nonnull.rs ✓
│   ├── ops.rs ✓
│   ├── option.rs ✓
│   ├── pointer.rs ✓
│   ├── reference.rs ✓
│   ├── result.rs ✓
│   ├── scalar.rs ✓
│   ├── slice.rs ✓
│   └── tuple.rs ✓
└── std/
    ├── hashmap.rs ✓
    ├── hashset.rs ✓
    └── path.rs ✓
```

---

## Completed: facet-reflect Migration

All files in `facet-reflect/src/` have been migrated to the new VTable system.

### Migration Summary

| Change | Description |
|--------|-------------|
| `shape.vtable.drop_in_place()` | → `shape.call_drop_in_place(ptr)` |
| `shape.vtable.default_in_place()` | → `shape.call_default_in_place(ptr)` |
| `shape.vtable.try_from()` | → `shape.call_try_from(src, dst)` |
| `shape.vtable.debug()` | → `shape.call_debug(ptr, f)` |
| `shape.vtable.display()` | → `shape.call_display(ptr, f)` |
| `shape.vtable.parse()` | → `shape.call_parse(s, ptr)` |
| `shape.vtable.partial_eq()` | → `shape.call_partial_eq(a, b)` |
| `shape.vtable.partial_cmp()` | → `shape.call_partial_cmp(a, b)` |
| `shape.vtable.hash()` | → `shape.call_hash(ptr, hasher)` |
| `shape.vtable.try_borrow_inner()` | → `shape.call_try_borrow_inner(ptr)` |
| `vtable.has_try_from()` | Check via `VTableErased::has_try_from()` |
| `vtable.has_invariants()` | Check via `VTableErased::has_invariants()` |
| `ThinPtr` | Removed - use `PtrConst` / `PtrMut` directly |
| `VTableRef` | Removed - use `VTableErased` |
| `vtable_ref!` macro | Removed - use `vtable_direct!` or `VTableIndirect::builder()` |
| `NonNull` in Ptr constructors | Use `.as_ptr()` to convert to raw pointer |

### Files Migrated

```
facet-reflect/src/
├── partial/
│   ├── mod.rs ✓
│   ├── heap_value.rs ✓
│   └── partial_api/
│       ├── build.rs ✓
│       ├── fields.rs ✓
│       ├── internal.rs ✓
│       ├── lists.rs ✓
│       ├── maps.rs ✓
│       ├── misc.rs ✓
│       ├── option.rs ✓
│       ├── ptr.rs ✓
│       ├── result.rs ✓
│       ├── set.rs ✓
│       └── sets.rs ✓
├── peek/
│   ├── value.rs ✓
│   ├── result.rs ✓
│   ├── option.rs ✓
│   ├── list_like.rs ✓
│   ├── owned.rs ✓
│   ├── fields.rs ✓
│   └── enum_.rs ✓
├── spanned.rs ✓
└── error.rs ✓
```

### Key Patterns

**Using vtable_direct! for concrete types:**
```rust
const VTABLE: VTableDirect = vtable_direct!(MyType => Debug, Default, PartialEq);
Shape::builder_for_sized::<MyType>("MyType")
    .vtable_direct(&VTABLE)
    // ...
```

**Using VTableIndirect for generic types:**
```rust
const fn build_vtable<'a, T: Facet<'a>>() -> VTableIndirect {
    VTableIndirect::builder()
        .drop_in_place(|ox| { /* ... */ })
        .build()
}

Shape::builder_for_sized::<Container<T>>("Container")
    .vtable_indirect(&const { build_vtable::<T>() })
    // ...
```

**Calling vtable methods through Shape:**
```rust
// Old: shape.vtable.drop_in_place()(ptr)
// New: 
unsafe { shape.call_drop_in_place(ptr) };

// Old: if let Some(f) = shape.vtable.partial_eq { f(a, b) }
// New:
unsafe { shape.call_partial_eq(a, b) }
```

---

## Next Steps

1. Run full test suite to verify migration
2. Check facet-json, facet-yaml, facet-toml for any remaining issues
3. Consider removing dead code / unused functions flagged by warnings

---

## Quick Reference

See doc comments in `facet-core/src/types/vtable.rs` for:
- `VTableDirect` / `VTableIndirect` 
- `vtable_direct!` / `vtable_indirect!` macros
- `OxRef` / `OxMut` wrapper types
- `Shape::call_*` methods for unified vtable access
