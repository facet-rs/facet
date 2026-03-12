# facet-postcard

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

Postcard binary format for facet.

This crate provides serialization and deserialization for the postcard binary format.

## Serialization

Serialization supports all types that implement [`facet_core::Facet`](https://docs.rs/facet_core/latest/facet_core/trait.Facet.html):

```rust
use facet::Facet;
use facet_postcard::to_vec;

#[derive(Facet)]
struct Point { x: i32, y: i32 }

let point = Point { x: 10, y: 20 };
let bytes = to_vec(&point).unwrap();
```

## Deserialization

There is a configurable [`Deserializer`](https://docs.rs/facet-postcard/latest/facet_postcard/struct.Deserializer.html) API plus convenience functions:

- [`from_slice`](https://docs.rs/facet-postcard/latest/facet_postcard/fn.from_slice.html): Deserializes into owned types (`T: Facet<'static>`)
- [`from_slice_borrowed`](https://docs.rs/facet-postcard/latest/facet_postcard/fn.from_slice_borrowed.html): Deserializes with zero-copy borrowing from the input buffer
- [`from_slice_with_shape`](https://docs.rs/facet-postcard/latest/facet_postcard/shape_deser/fn.from_slice_with_shape.html): Deserializes into `Value` using runtime shape information
- [`from_slice_into`](https://docs.rs/facet-postcard/latest/facet_postcard/fn.from_slice_into.html): Deserializes into an existing `Partial` (type-erased, owned)
- [`from_slice_into_borrowed`](https://docs.rs/facet-postcard/latest/facet_postcard/fn.from_slice_into_borrowed.html): Deserializes into an existing `Partial` (type-erased, zero-copy)

```rust
use facet_postcard::from_slice;

// Postcard encoding: [length=3, true, false, true]
let bytes = &[0x03, 0x01, 0x00, 0x01];
let result: Vec<bool> = from_slice(bytes).unwrap();
assert_eq!(result, vec![true, false, true]);
```

Both functions automatically select the best deserialization tier:
- **Tier-2 (Format JIT)**: Fastest path for compatible types (primitives, structs, vecs, simple enums)
- **Tier-0 (Reflection)**: Fallback for all other types (nested enums, complex types)

This ensures all `Facet` types can be deserialized.

<!-- cargo-reedme: end -->
