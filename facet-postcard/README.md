# facet-postcard

Postcard serialization and deserialization for [Facet](https://github.com/facet-rs/facet) types.

[Postcard](https://github.com/jamesmunns/postcard) is a compact, `#![no_std]`-friendly binary serialization format designed for embedded systems and resource-constrained environments.

## Features

- **Compact binary format** - Uses variable-length integers and efficient encoding
- **`no_std` compatible** - Works in embedded environments without an allocator
- **Full compatibility** - Output is byte-for-byte identical to the `postcard` crate
- **Zero-copy where possible** - Efficient deserialization

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
facet = "0.1"
facet-postcard = "0.1"
```

### Serialization

```rust
use facet::Facet;
use facet_postcard::to_vec;

#[derive(Debug, Facet)]
struct Point {
    x: i32,
    y: i32,
}

let point = Point { x: 10, y: 20 };
let bytes = to_vec(&point).unwrap();
```

### Deserialization

```rust
use facet::Facet;
use facet_postcard::{from_bytes, to_vec};

#[derive(Debug, Facet, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

let original = Point { x: 10, y: 20 };
let bytes = to_vec(&original).unwrap();
let decoded: Point = from_bytes(&bytes).unwrap();
assert_eq!(original, decoded);
```

### Serialization to a slice (no_std)

```rust
use facet::Facet;
use facet_postcard::to_slice;

#[derive(Debug, Facet)]
struct Point {
    x: i32,
    y: i32,
}

let point = Point { x: 10, y: 20 };
let mut buffer = [0u8; 64];
let len = to_slice(&point, &mut buffer).unwrap();
let bytes = &buffer[..len];
```

## Feature Flags

- `std` (default) - Enables standard library support
- `alloc` - Enables allocation support for `no_std` environments with an allocator

## Compatibility

This crate produces output that is byte-for-byte compatible with the `postcard` crate. You can serialize with `facet-postcard` and deserialize with `postcard` (and vice versa).

## Sponsors

The development of Facet is made possible by these wonderful sponsors:

<!-- sponsors --><a href="https://github.com/zkat"><img src="https://github.com/zkat.png" width="60px" alt="Kat Marchán" /></a><a href="https://github.com/yotamofek"><img src="https://github.com/yotamofek.png" width="60px" alt="" /></a><a href="https://github.com/Veykril"><img src="https://github.com/Veykril.png" width="60px" alt="Lukas Wirth" /></a><a href="https://github.com/eminence"><img src="https://github.com/eminence.png" width="60px" alt="Andrew Chin" /></a><a href="https://github.com/pop"><img src="https://github.com/pop.png" width="60px" alt="Brian Olson" /></a><a href="https://github.com/mikecvet"><img src="https://github.com/mikecvet.png" width="60px" alt="Mike Cvet" /></a><a href="https://github.com/vertexclique"><img src="https://github.com/vertexclique.png" width="60px" alt="Mahmut Bulut" /></a><!-- sponsors -->
