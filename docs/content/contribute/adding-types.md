+++
title = "Adding Type Support"
weight = 6
insert_anchor_links = "heading"
+++

## Standard Library Types

Add implementations in the appropriate `impls_*` module in `facet-core`:

```rust
// In facet-core/src/impls_core/scalar.rs

unsafe impl Facet<'_> for MyType {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(value_vtable!(MyType, |f, _opts| {
                write!(f, "MyType")
            }))
            .type_identifier("MyType")
            .def(Def::Scalar)
            .ty(Type::User(UserType::Opaque))
            .build()
    };
}
```

## External Crate Types

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

## Collection Types

Collections need vtable functions:

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
