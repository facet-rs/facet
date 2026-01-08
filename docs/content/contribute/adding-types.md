+++
title = "Implementing Facet for third-party types"
weight = 6
insert_anchor_links = "heading"
+++

This guide is for contributing `Facet` implementations to the facet repository. If you just want to use a type that doesn't implement `Facet`, see [When a type doesn't implement Facet](@/guide/ecosystem.md#when-a-type-doesn-t-implement-facet).

## Why we implement from the facet side

In Rust, you can only implement a trait in one of two places:
1. The crate that defines the trait
2. The crate that defines the type

Ideally, crates like `chrono` or `uuid` would implement `Facet` for their types directly. But facet isn't stable yet — the `Facet` trait and `Shape` structure are still evolving.

So we implement `Facet` for third-party types from the facet side, using optional features in `facet-core` (re-exported through `facet`). When facet stabilizes, crate authors can implement `Facet` themselves, and we'll deprecate our implementations.

## Adding support for a new crate

1. Add the dependency to `facet-core/Cargo.toml`:
   ```toml
   [dependencies]
   my-crate = { version = "1.0", optional = true }

   [features]
   my-crate = ["dep:my-crate"]
   ```

2. Create `facet-core/src/impls_my_crate.rs`

3. Add to `facet-core/src/lib.rs`:
   ```rust,noexec
   #[cfg(feature = "my-crate")]
   mod impls_my_crate;
   ```

4. Re-export the feature from `facet/Cargo.toml`:
   ```toml
   [features]
   my-crate = ["facet-core/my-crate"]
   ```

## Implementing Facet

Most third-party types are scalars (atomic values like UUIDs, timestamps, paths).
Use `ShapeBuilder` for a cleaner implementation:

```rust,noexec
unsafe impl Facet<'_> for my_crate::MyType {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>("MyType")
            .module_path("my_crate")
            .decl_id(DeclId::new(decl_id_hash("@my_crate#struct#MyType")))
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&MY_TYPE_VTABLE)
            .build()
    };
}
```

Look at existing implementations in `facet-core/src/impls_*` for patterns:
- `impls_uuid.rs` — simple scalar
- `impls_chrono.rs` — multiple related types
- `impls_camino.rs` — path types with borrowed variants
- `impls_bytes.rs` — byte buffer types

## Collection types

Collections need vtable functions for their operations (push, get, len, etc.):

```rust,noexec
unsafe impl<T: Facet<'static>> Facet<'_> for MyVec<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>("MyVec")
            .module_path("my_crate")
            // For generic types, the decl_id is the same for all instantiations
            .decl_id(DeclId::new(decl_id_hash("@my_crate#struct#MyVec")))
            .ty(Type::User(UserType::Opaque))
            .def(Def::List(ListDef {
                vtable: &ListVTable {
                    init_empty: |target| { /* ... */ },
                    push: |list, value| { /* ... */ },
                    len: |list| { /* ... */ },
                    get: |list, index| { /* ... */ },
                },
                item_shape: T::SHAPE,
            }))
            .type_params(&[TypeParam { name: "T", shape: T::SHAPE }])
            .vtable_indirect(&MY_VEC_VTABLE)
            .build()
    };
}
```

## Testing

Add tests in the same file or in `facet-core/tests/`. Make sure to test:
- Round-trip through at least one format (JSON is easiest)
- Edge cases for the type (empty values, max values, etc.)
