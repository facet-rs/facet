+++
title = "Wire Format"
description = "Postcard encoding and Rapace frames"
weight = 20
+++

This document defines how Rapace encodes data on the wire, covering both the **postcard binary format** for message payloads and the **Rapace framing protocol** for transport.

## Overview

Rapace uses a two-layer encoding:

1. **Message payload encoding**: [Postcard](https://postcard.jamesmunns.com/) — compact, deterministic binary format
2. **Frame encoding**: Rapace-specific framing for multiplexing, flow control, and metadata

```text
Rapace Frame

┌───────────────────────────────────────────────┐
│ MsgDescHot (64 bytes)                          │
│   msg_id: u64                                  │
│   channel_id: u32                              │
│   method_id: u32                               │
│   payload_slot: u32                            │
│   payload_generation: u32                      │
│   payload_offset: u32                          │
│   payload_len: u32                             │
│   flags: u32                                   │
│   credit_grant: u32                            │
│   deadline_ns: u64                             │
│   inline_payload: [u8; 16]                     │
├───────────────────────────────────────────────┤
│ Payload (postcard-encoded)                     │
│   User data (args/results)                     │
│   Control messages                             │
└───────────────────────────────────────────────┘
```

## Postcard Encoding

Rapace uses the [postcard v1.x wire format](https://postcard.jamesmunns.com/wire-format) for all message payloads. Postcard is:

- **Non-self-describing**: No type information encoded
- **Compact**: Variable-length integers, no padding
- **Deterministic**: Same value always encodes identically
- **Fast**: Simple state machine, minimal allocations

### Key Properties

#### Variable-Length Integers (Varint)

Most integers use [LEB128](https://en.wikipedia.org/wiki/LEB128) encoding:

- Each byte has 7 data bits + 1 continuation bit
- Continuation bit = 1 means "more bytes follow"
- Little-endian byte order

**Types using varint**: `u16`, `i16`, `u32`, `i32`, `u64`, `i64`, `u128`, `i128`, `usize`, `isize`

**Types using direct encoding**: `u8`, `i8` (single byte, as-is)

**Example encodings**:
- `0u32` → `[0x00]` (1 byte)
- `128u32` → `[0x80, 0x01]` (2 bytes)
- `65535u32` → `[0xFF, 0xFF, 0x03]` (3 bytes)

#### Zigzag Encoding for Signed Integers

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

#### Maximum Encoded Sizes

Each integer type has a predictable worst-case size:

| Type | Max Bytes |
|------|-----------|
| `u8`, `i8` | 1 |
| `u16`, `i16` | 3 |
| `u32`, `i32` | 5 |
| `u64`, `i64` | 10 |
| `u128`, `i128` | 19 |

### Encoding Rules by Type

#### Primitives

| Type | Encoding |
|------|----------|
| `bool` | Single byte: `0x00` (false), `0x01` (true) |
| `u8`, `i8` | Single byte, as-is |
| `u16`-`u128` | Varint (LEB128) |
| `i16`-`i128` | Zigzag + varint |
| `f32` | 4 bytes, IEEE 754 little-endian (no varint) |
| `f64` | 8 bytes, IEEE 754 little-endian (no varint) |
| `char` | UTF-8 encoded (1-4 bytes) |

#### Strings and Byte Arrays

```
varint(length) + data
```

**Example** (`"hello"`):
```
[0x05, 0x68, 0x65, 0x6C, 0x6C, 0x6F]
 └─┬─┘  └──────────┬──────────────┘
  len       "hello" (5 bytes)
```

#### Option Types

```
None: [0x00]
Some(T): [0x01] + encode(T)
```

#### Unit Types

```
(): zero bytes
unit_struct: zero bytes
unit_variant: varint(discriminant)
```

#### Sequences (Vec, slices)

```
varint(element_count) + encode(elem0) + encode(elem1) + ...
```

**Example** (`vec![1u32, 2, 3]`):
```
[0x03, 0x01, 0x02, 0x03]
 └─┬─┘  └─┬─┘ └─┬─┘ └─┬─┘
  len    1     2     3
```

#### Tuples and Tuple Structs

Elements encoded in order, **no length prefix**:

```
(T1, T2, T3): encode(field0) + encode(field1) + encode(field2)
```

#### Structs

Fields encoded in **declaration order**, **no field names or tags**:

```rust
struct Point { x: i32, y: i32 }
```

Encoded as:
```
encode(x) + encode(y)
```

**Critical**: Field order is part of the schema. Reordering fields breaks compatibility.

#### Enums

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

#### Maps

```
varint(pair_count) + (encode(key0), encode(val0)) + (encode(key1), encode(val1)) + ...
```

### Platform Considerations

**usize and isize**: Map to platform width (`u32`/`i32` on 32-bit, `u64`/`i64` on 64-bit).

⚠️ **Cross-platform compatibility**: Values must fit in the smaller platform's range. Rapace **prohibits** `usize`/`isize` in public APIs (see [Data Model](@/spec/data-model.md#explicitly-unsupported)).

## Postcard Specification Reference

The complete postcard wire format is specified at:

**https://postcard.jamesmunns.com/wire-format**

The format is **stable as of postcard v1.0.0**. Breaking changes require a v2.0.0 release.

Rapace uses postcard v1.x and follows the official specification exactly. For edge cases, ambiguities, or implementation details not covered here, the official postcard specification is authoritative.

## Rapace Framing

While postcard handles message payload encoding, Rapace adds a **framing layer** for:

- **Multiplexing**: Multiple concurrent RPC calls over one connection
- **Flow control**: Per-channel credit-based backpressure
- **Deadlines**: Request timeouts
- **Cancellation**: Explicit request cancellation
- **Metadata**: Tracing, priority, etc.

### Frame Structure

Every Rapace frame consists of:

```
┌─────────────────────────────────────────┐
│ MsgDescHot (64 bytes, one cache line)   │
├─────────────────────────────────────────┤
│ Payload (0+ bytes, postcard-encoded)    │
└─────────────────────────────────────────┘
```

### MsgDescHot (Hot-Path Descriptor)

The descriptor is **64 bytes** (one cache line) for performance:

```text
MsgDescHot (64 bytes)

┌────────────────────────────────────────┐  16B
│ Identity                               │
│   msg_id: u64                          │
│   channel_id: u32                      │
│   method_id: u32                       │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐  16B
│ Payload Location                       │
│   payload_slot: u32                    │
│   payload_generation: u32              │
│   payload_offset: u32                  │
│   payload_len: u32                     │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐  16B
│ Flow Control                           │
│   flags: u32                           │
│   credit_grant: u32                    │
│   deadline_ns: u64                     │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐  16B
│ Inline Payload                         │
│   inline_payload: [u8; 16]             │
└────────────────────────────────────────┘
```

```pikchr
scale = 0.9
bh = 0.45in
u32w = 0.75in
u64w = 1.5in
fullw = 3.0in
gap = 0.06in

# Row 0: Identity (bytes 0-15)
L0: text "IDENTITY" mono bold at (0, 0)
R0F0: box "msg_id" "u64" mono width u64w height bh fill 0xCFE7F3 \
  with .w at L0.e + (0.2in, 0)
R0F1: box "channel_id" "u32" mono width u32w height bh fill 0xCFE7F3 with .w at R0F0.e
R0F2: box "method_id" "u32" mono width u32w height bh fill 0xCFE7F3 with .w at R0F1.e
# Separators
line from R0F0.ne to R0F0.se
line from R0F1.ne to R0F1.se
# Byte offsets above first row
text "0" mono small with .s at R0F0.nw + (0, 0.08in)
text "8" mono small with .s at R0F1.nw + (0, 0.08in)
text "12" mono small with .s at R0F2.nw + (0, 0.08in)
text "16" mono small with .s at R0F2.ne + (0, 0.08in)

# Row 1: Payload location (bytes 16-31)
L1: text "LOCATION" mono bold with .e at L0.e + (0, -bh - gap)
R1F0: box "payload_slot" "u32" mono width u32w height bh fill 0xC9EFC2 \
  with .nw at R0F0.sw + (0, -gap)
R1F1: box "payload_gen" "u32" mono width u32w height bh fill 0xC9EFC2 with .w at R1F0.e
R1F2: box "payload_off" "u32" mono width u32w height bh fill 0xC9EFC2 with .w at R1F1.e
R1F3: box "payload_len" "u32" mono width u32w height bh fill 0xC9EFC2 with .w at R1F2.e
# Separators
line from R1F0.ne to R1F0.se
line from R1F1.ne to R1F1.se
line from R1F2.ne to R1F2.se
text "32" mono small with .w at R1F3.e + (0.08in, 0)

# Row 2: Flow control (bytes 32-47)
L2: text "FLOW CTRL" mono bold with .e at L1.e + (0, -bh - gap)
R2F0: box "flags" "u32" mono width u32w height bh fill 0xFFF2B6 \
  with .nw at R1F0.sw + (0, -gap)
R2F1: box "credit_grant" "u32" mono width u32w height bh fill 0xFFF2B6 with .w at R2F0.e
R2F2: box "deadline_ns" "u64" mono width u64w height bh fill 0xFFF2B6 with .w at R2F1.e
# Separators
line from R2F0.ne to R2F0.se
line from R2F1.ne to R2F1.se
text "48" mono small with .w at R2F2.e + (0.08in, 0)

# Row 3: Inline payload (bytes 48-63)
L3: text "INLINE" mono bold with .e at L2.e + (0, -bh - gap)
R3F0: box "inline_payload" "[u8; 16]" mono width fullw height bh fill 0xF1B5B5 \
  with .nw at R2F0.sw + (0, -gap)
text "64" mono small with .w at R3F0.e + (0.08in, 0)

# Variable payload section
L4: text "PAYLOAD" mono bold with .e at L3.e + (0, -bh - gap - 0.1in)
PL: box "postcard-encoded data" "0+ bytes" mono width fullw height 0.5in \
  fill 0xDFF5E1 dashed with .nw at R3F0.sw + (0, -gap - 0.1in)
```

```rust
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    pub msg_id: u64,         // Unique message ID (monotonic)
    pub channel_id: u32,     // Logical RPC channel (0 = control)
    pub method_id: u32,      // Method to invoke (or control verb)

    // Payload location (16 bytes)
    pub payload_slot: u32,   // Slot index (0xFFFFFFFF = inline)
    pub payload_generation: u32,  // ABA safety counter
    pub payload_offset: u32, // Offset within slot
    pub payload_len: u32,    // Payload byte length

    // Flow control & timing (16 bytes)
    pub flags: u32,          // FrameFlags (see below)
    pub credit_grant: u32,   // Flow control credits granted
    pub deadline_ns: u64,    // Absolute deadline (0xFFFFFFFFFFFFFFFF = none)

    // Inline payload (16 bytes)
    pub inline_payload: [u8; 16],  // Used when payload_slot == 0xFFFFFFFF
}
```

**Size assertion**: `sizeof(MsgDescHot) == 64` (4 × 16-byte blocks = one cache line)

### Frame Flags

```rust
pub struct FrameFlags: u32 {
    DATA          = 0b0000_0001;  // Regular data frame
    CONTROL       = 0b0000_0010;  // Control message (channel 0)
    EOS           = 0b0000_0100;  // End of stream (half-close)
    CANCEL        = 0b0000_1000;  // Cancel this channel
    ERROR         = 0b0001_0000;  // Error response
    HIGH_PRIORITY = 0b0010_0000;  // Priority scheduling hint
    CREDITS       = 0b0100_0000;  // Contains credit grant
    METADATA_ONLY = 0b1000_0000;  // Headers/trailers, no body
    NO_REPLY      = 0b0001_0000_0000;  // Fire-and-forget, no response
}
```

### Payload Placement

Payloads can be stored in multiple ways:

**Inline** (≤16 bytes):
- `payload_slot = 0xFFFFFFFF`
- Data in `inline_payload` field
- Zero-copy, no allocation

**Out-of-line** (>16 bytes):
- For **stream/WebSocket transports**: owned heap buffer (`Vec<u8>` or `bytes::Bytes`)
- For **SHM transport**: slot reference (`payload_slot`, `payload_offset`, `payload_len`)

SHM slots use **generation counters** for ABA safety (prevent use-after-free if slot is reused).

### Channel Multiplexing

- **Channel 0**: Reserved for control messages (handshake, ping/pong, flow control)
- **Channels 1+**: User RPC calls
- **Channel ownership**: Peers use disjoint ranges (e.g., client: odd, server: even)

Each channel is an independent logical stream. Frames from different channels can interleave.

### Control Channel (Channel 0)

Control messages use `method_id` as a verb:

| method_id | Verb | Payload |
|-----------|------|---------|
| 1 | OpenChannel | `{ channel_id, service_name, method_name, metadata }` |
| 2 | CloseChannel | `{ channel_id, reason }` |
| 3 | CancelChannel | `{ channel_id, reason }` |
| 4 | GrantCredits | `{ channel_id, bytes }` |
| 5 | Ping | `{ payload: [u8; 8] }` |
| 6 | Pong | `{ payload: [u8; 8] }` |

Control payloads are **postcard-encoded** like regular messages.

See [Core Protocol](@/spec/core.md) for detailed channel lifecycle and control semantics.

## Transport-Specific Encoding

Different transports encode frames differently:

### Stream Transports (TCP, Unix sockets)

**Frame encoding**:
```
┌────────────────────────────────────────────┐
│ Length (varint, 1-10 bytes)                │
├────────────────────────────────────────────┤
│ MsgDescHot (64 bytes)                      │
├────────────────────────────────────────────┤
│ Payload (N bytes, postcard-encoded)       │
└────────────────────────────────────────────┘
```

- Length prefix includes descriptor + payload
- Enables framing over byte streams

### WebSocket Transport

Each WebSocket **binary message** = one Rapace frame:

```
┌────────────────────────────────────────────┐
│ MsgDescHot (64 bytes)                      │
├────────────────────────────────────────────┤
│ Payload (N bytes, postcard-encoded)       │
└────────────────────────────────────────────┘
```

No additional length prefix needed (WebSocket provides framing).

### Shared Memory (SHM) Transport

**Descriptor rings** (SPSC queues):
```
┌────────────────────────────────────────────┐
│ Ring of MsgDescHot (64 bytes each)         │
└────────────────────────────────────────────┘
```

**Payload storage** (slab allocator):
```
┌────────────────────────────────────────────┐
│ Slot 0: [payload data]                     │
│ Slot 1: [payload data]                     │
│ ...                                        │
│ Slot N: [payload data]                     │
└────────────────────────────────────────────┘
```

Descriptors reference payload slots by index + generation counter. Inline payloads (≤16 bytes) skip slab entirely.

See [Transport Considerations](@/spec/transports.md) for transport-specific details.

## Determinism and Stability

Rapace's wire format is **deterministic and stable**:

✅ **Same value always encodes to same bytes** (within a postcard version)
✅ **Byte-for-byte equality** can be used for caching, deduplication, etc.
✅ **Structural hashing** is used for schema compatibility (see [Schema Evolution](@/spec/schema-evolution.md))

**Postcard stability guarantee**: Wire format is stable as of v1.0.0. Breaking changes require major version bump.

**Rapace framing stability**: MsgDescHot layout is fixed. Extensions use reserved fields or new frame types.

## Next Steps

- [Data Model](@/spec/data-model.md) – What types can be encoded
- [Schema Evolution](@/spec/schema-evolution.md) – How schemas change over time
- [Core Protocol](@/spec/core.md) – Channel lifecycle and control messages
- [Transport Considerations](@/spec/transports.md) – Transport-specific behaviors
