# facet-asn1

<!-- cargo-reedme: start -->

<!-- cargo-reedme: info-start

    Do not edit this region by hand
    ===============================

    This region was generated from Rust documentation comments by `cargo-reedme` using this command:

        cargo +nightly reedme --workspace

    for more info: https://github.com/nik-rev/cargo-reedme

cargo-reedme: info-end -->

ASN.1 DER/BER serialization and deserialization for facet.

This crate provides ASN.1 DER (Distinguished Encoding Rules) support via the
`FormatParser` and `FormatSerializer` traits.

## ASN.1 Overview

ASN.1 (Abstract Syntax Notation One) is a standard interface description language
for defining data structures that can be serialized and deserialized in a
cross-platform way. DER is a specific encoding rule that ensures canonical encoding.

## Serialization

```rust
use facet::Facet;
use facet_asn1::to_vec;

#[derive(Facet)]
struct Point { x: i32, y: i32 }

let point = Point { x: 10, y: 20 };
let bytes = to_vec(&point).unwrap();
```

## Deserialization

```rust
use facet::Facet;
use facet_asn1::from_slice;

#[derive(Facet)]
struct Point { x: i32, y: i32 }

// DER encoding of Point { x: 10, y: 20 }
let bytes = &[0x30, 0x06, 0x02, 0x01, 0x0A, 0x02, 0x01, 0x14];
let point: Point = from_slice(bytes).unwrap();
```

## Type Mapping

| Rust Type | ASN.1 Type |
|-----------|------------|
| `bool` | BOOLEAN |
| `i8`, `i16`, `i32`, `i64` | INTEGER |
| `u8`, `u16`, `u32`, `u64` | INTEGER |
| `f32`, `f64` | REAL |
| `String`, `&str` | UTF8String |
| `Vec<u8>`, `&[u8]` | OCTET STRING |
| struct | SEQUENCE |
| `Vec<T>` | SEQUENCE OF |
| `Option<T>` | Optional field |
| `()` | NULL |

<!-- cargo-reedme: end -->
