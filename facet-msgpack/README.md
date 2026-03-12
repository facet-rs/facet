# facet-msgpack

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

MsgPack binary format for facet.

This crate provides serialization and deserialization for the MessagePack binary format.

## Serialization

```rust
use facet::Facet;
use facet_msgpack::to_vec;

#[derive(Facet)]
struct Point { x: i32, y: i32 }

let point = Point { x: 10, y: 20 };
let bytes = to_vec(&point).unwrap();
```

## Deserialization

There are two deserialization functions:

- [`from_slice`](https://docs.rs/facet-msgpack/latest/facet_msgpack/fn.from_slice.html): Deserializes into owned types (`T: Facet<'static>`)
- [`from_slice_borrowed`](https://docs.rs/facet-msgpack/latest/facet_msgpack/fn.from_slice_borrowed.html): Deserializes with zero-copy borrowing from the input buffer
- [`from_slice_into`](https://docs.rs/facet-msgpack/latest/facet_msgpack/fn.from_slice_into.html): Deserializes into an existing `Partial` (type-erased, owned)
- [`from_slice_into_borrowed`](https://docs.rs/facet-msgpack/latest/facet_msgpack/fn.from_slice_into_borrowed.html): Deserializes into an existing `Partial` (type-erased, zero-copy)

```rust
use facet::Facet;
use facet_msgpack::from_slice;

#[derive(Facet, Debug, PartialEq)]
struct Point { x: i32, y: i32 }

// MsgPack encoding of {"x": 10, "y": 20}
let bytes = &[0x82, 0xa1, b'x', 0x0a, 0xa1, b'y', 0x14];
let point: Point = from_slice(bytes).unwrap();
assert_eq!(point.x, 10);
assert_eq!(point.y, 20);
```

Both functions use Tier-2 JIT for compatible types (when the `jit` feature is enabled),
with automatic fallback to Tier-0 reflection for all other types.

<!-- cargo-reedme: end -->
