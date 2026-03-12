# facet-xdr

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

XDR (External Data Representation) format support via facet-format.

XDR is a binary format defined in RFC 4506 for encoding structured data.
It is primarily used in Sun RPC (ONC RPC) protocols.

Key characteristics:
- Big-endian byte order
- Fixed-size integers (4 bytes for i32/u32, 8 bytes for i64/u64)
- No support for i128/u128
- Strings are length-prefixed with 4-byte aligned padding
- Arrays have explicit length prefixes

## Serialization

```rust
use facet::Facet;
use facet_xdr::to_vec;

#[derive(Facet)]
struct Point { x: i32, y: i32 }

let point = Point { x: 10, y: 20 };
let bytes = to_vec(&point).unwrap();
```

## Deserialization

```rust
use facet::Facet;
use facet_xdr::from_slice;

#[derive(Facet, Debug, PartialEq)]
struct Point { x: i32, y: i32 }

// XDR encoding of Point { x: 10, y: 20 }
let bytes = &[0, 0, 0, 10, 0, 0, 0, 20];
let point: Point = from_slice(bytes).unwrap();
assert_eq!(point.x, 10);
assert_eq!(point.y, 20);
```

<!-- cargo-reedme: end -->
