+++
title = "Payload Encoding"
description = "Postcard binary encoding for message payloads"
weight = 20
+++

This document defines how Rapace encodes message payloads using the [Postcard](https://postcard.jamesmunns.com/) binary format.

## Overview

Rapace uses Postcard for message payload encoding on **CALL and STREAM channels**. Postcard is:

- **Non-self-describing**: No type information encoded in the wire format
- **Compact**: Variable-length integers, no padding
- **Fast**: Simple state machine, minimal allocations

For supported types, see [Data Model](@/spec/data-model.md).

> **Exception**: TUNNEL channel payloads are **raw bytes**, not Postcard-encoded. See [Core Protocol: TUNNEL Channels](@/spec/core.md#tunnel-channels) for details.

## Key Properties

### Variable-Length Integers (Varint)

Most integers use [LEB128](https://en.wikipedia.org/wiki/LEB128) encoding:

- Each byte has 7 data bits + 1 continuation bit
- Continuation bit = 1 means "more bytes follow"
- Little-endian byte order

**Types using varint**: `u16`, `i16`, `u32`, `i32`, `u64`, `i64`, `u128`, `i128`

**Types using direct encoding**: `u8`, `i8` (single byte, as-is)

**Example encodings**:
- `0u32` → `[0x00]` (1 byte)
- `128u32` → `[0x80, 0x01]` (2 bytes)
- `65535u32` → `[0xFF, 0xFF, 0x03]` (3 bytes)

### Varint Canonicalization

Varints MUST be encoded in **canonical form**: the shortest possible encoding for the value.

**Non-canonical examples** (MUST be rejected):
- `0u32` encoded as `[0x80, 0x00]` (2 bytes instead of 1)
- `1u32` encoded as `[0x81, 0x00]` (2 bytes instead of 1)
- `127u32` encoded as `[0xFF, 0x00]` (2 bytes instead of 1)

**Validation rules**:
1. If a varint has trailing bytes with only the continuation bit set and zero data bits, it is non-canonical
2. Receivers MUST reject non-canonical varints as malformed
3. This prevents ambiguity and ensures consistent hashing/comparison of encoded payloads

**Implementation note**: Most LEB128 encoders naturally produce canonical form. Decoders should check that the final byte contributes meaningful bits (i.e., the value would not fit in fewer bytes).

### Zigzag Encoding for Signed Integers

Signed integers are [zigzag-encoded](https://en.wikipedia.org/wiki/Variable-length_quantity#Zigzag_encoding) before varint:

```
 0 → 0
-1 → 1
 1 → 2
-2 → 3
 2 → 4
...
```

This makes small negative numbers compact (e.g., `-1` → `0x01`, not `0xFF 0xFF ...`).

**Example**:
- `-1i32` → zigzag: `1` → varint: `[0x01]`
- `1i32` → zigzag: `2` → varint: `[0x02]`

### Maximum Encoded Sizes

Each integer type has a predictable worst-case size:

| Type | Max Bytes |
|------|-----------|
| `u8`, `i8` | 1 |
| `u16`, `i16` | 3 |
| `u32`, `i32` | 5 |
| `u64`, `i64` | 10 |
| `u128`, `i128` | 19 |

## Encoding Rules by Type

### Primitives

| Type | Encoding |
|------|----------|
| `bool` | Single byte: `0x00` (false), `0x01` (true) |
| `u8`, `i8` | Single byte, as-is |
| `u16`-`u128` | Varint (LEB128) |
| `i16`-`i128` | Zigzag + varint |
| `f32` | 4 bytes, IEEE 754 little-endian |
| `f64` | 8 bytes, IEEE 754 little-endian |
| `char` | Varint encoding of Unicode scalar value (u32) |

**Note on `char`**: The Rust `char` type is a Unicode scalar value (U+0000 to U+D7FF or U+E000 to U+10FFFF). It is encoded as a varint of its u32 value, NOT as UTF-8 bytes. This differs from `String` encoding.

### Float Canonicalization

All NaN values MUST be canonicalized before encoding:

- `f32` NaN: `0x7FC00000` (quiet NaN, zero payload)
- `f64` NaN: `0x7FF8000000000000` (quiet NaN, zero payload)

Implementations MUST replace any NaN bit pattern with the canonical form. This ensures consistent encoding across platforms.

Negative zero (`-0.0`) is NOT canonicalized and encodes as its IEEE 754 bit pattern.

### Strings and Byte Arrays

```
varint(length) + data
```

**Example** (`"hello"`):
```
[0x05, 0x68, 0x65, 0x6C, 0x6C, 0x6F]
 └─┬─┘  └──────────┬──────────────┘
  len       "hello" (5 bytes)
```

### Option Types

```
None: [0x00]
Some(T): [0x01] + encode(T)
```

### Unit Types

```
(): zero bytes
unit_struct: zero bytes
unit_variant: varint(discriminant)
```

### Sequences (Vec, slices)

```
varint(element_count) + encode(elem0) + encode(elem1) + ...
```

**Example** (`vec![1u32, 2, 3]`):
```
[0x03, 0x01, 0x02, 0x03]
 └─┬─┘  └─┬─┘ └─┬─┘ └─┬─┘
  len    1     2     3
```

### Tuples and Tuple Structs

Elements encoded in order, **no length prefix**:

```
(T1, T2, T3): encode(field0) + encode(field1) + encode(field2)
```

### Structs

Fields encoded in **declaration order**, **no field names or tags**:

```rust
struct Point { x: i32, y: i32 }
```

Encoded as:
```
encode(x) + encode(y)
```

**Critical**: Field order is part of the schema. Reordering fields breaks compatibility.

### Enums

```
varint(discriminant) + encode(variant_data)
```

**Unit variant**:
```rust
enum Color { Red, Green, Blue }
Color::Green
```
→ `[0x01]` (discriminant only)

**Tuple variant**:
```rust
enum Shape { Circle(f64) }
Shape::Circle(10.5)
```
→ `[0x00, <f64 bytes>]`

**Struct variant**:
```rust
enum Shape { Rectangle { w: f64, h: f64 } }
Shape::Rectangle { w: 10.0, h: 20.0 }
```
→ `[<discriminant>, <f64 for w>, <f64 for h>]`

### Maps

```
varint(pair_count) + (encode(key0), encode(val0)) + (encode(key1), encode(val1)) + ...
```

**Warning**: Map encoding is NOT deterministic. Iteration order may vary between implementations, runs, and languages. Do NOT rely on byte-for-byte equality for values containing maps.

## Stability

**Rapace freezes the postcard v1 wire format as specified in this document.**

Implementations MUST follow the encoding rules defined here. The [postcard crate](https://postcard.jamesmunns.com/) is a reference implementation, not an authority.

If postcard changes in the future, Rapace does not. The rules in this document are the canonical definition of Rapace payload encoding.

For additional context on the postcard wire format, see: https://postcard.jamesmunns.com/wire-format

## Next Steps

- [Data Model](@/spec/data-model.md) – What types can be encoded
- [Frame Format](@/spec/frame-format.md) – How payloads are wrapped in frames
- [Transport Bindings](@/spec/transport-bindings.md) – How frames are sent over different transports
