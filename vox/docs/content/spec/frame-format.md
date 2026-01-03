+++
title = "Frame Format"
description = "MsgDescHot descriptor and payload abstraction"
weight = 25
+++

This document defines the Rapace frame structure: the `MsgDescHot` descriptor and the `PayloadBuffer` abstraction.

## Overview

r[frame.structure]
A Rapace frame MUST consist of:

1. **MsgDescHot**: A 64-byte descriptor containing routing, flow control, and payload location info
2. **PayloadBuffer**: The Postcard-encoded payload bytes (location varies by transport)

```
+-----------------------------------------------+
| MsgDescHot (64 bytes)                         |
|   Identity: msg_id, channel_id, method_id     |
|   Location: payload reference                 |
|   Control: flags, credits, deadline           |
|   Inline: small payloads (<=16 bytes)         |
+-----------------------------------------------+
| PayloadBuffer                                 |
|   Location varies by transport                |
|   - Inline: in descriptor                     |
|   - Stream: heap-allocated                    |
|   - SHM: slot-backed, zero-copy borrowable    |
+-----------------------------------------------+
```

## MsgDescHot (Hot-Path Descriptor)

r[frame.desc.size]
The descriptor MUST be exactly **64 bytes** (one cache line):

```
MsgDescHot Layout (64 bytes, cache-line aligned)

Byte:  0               8               16
       +---------------+---------------+---------------+---------------+
    0  |        msg_id (u64)           |  channel_id   |   method_id   |  Identity
       |                               |     (u32)     |     (u32)     |  (16 bytes)
       +---------------+---------------+---------------+---------------+
   16  |  payload_slot | payload_gen   | payload_off   |  payload_len  |  Payload Location
       |     (u32)     |    (u32)      |    (u32)      |     (u32)     |  (16 bytes)
       +---------------+---------------+---------------+---------------+
   32  |     flags     | credit_grant  |          deadline_ns          |  Flow Control
       |     (u32)     |    (u32)      |             (u64)             |  (16 bytes)
       +---------------+---------------+---------------+---------------+
   48  |                       inline_payload                          |  Inline Payload
       |                          [u8; 16]                             |  (16 bytes)
       +---------------------------------------------------------------+
   64
```

```rust
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    pub msg_id: u64,              // Message ID (see msg_id Semantics below)
    pub channel_id: u32,          // Logical channel (0 = control)
    pub method_id: u32,           // Method to invoke (or control verb)

    // Payload location (16 bytes)
    pub payload_slot: u32,        // Slot index (0xFFFFFFFF = inline)
    pub payload_generation: u32,  // ABA safety counter (SHM only)
    pub payload_offset: u32,      // Offset within slot
    pub payload_len: u32,         // Payload byte length

    // Flow control & timing (16 bytes)
    pub flags: u32,               // FrameFlags
    pub credit_grant: u32,        // Credits granted (if CREDITS flag set)
    pub deadline_ns: u64,         // Absolute deadline (0xFFFFFFFFFFFFFFFF = none)

    // Inline payload (16 bytes)
    pub inline_payload: [u8; 16], // Used when payload_slot == 0xFFFFFFFF
}
```

r[frame.desc.sizeof]
Implementations MUST ensure `sizeof(MsgDescHot) == 64` (4 × 16-byte blocks = one cache line).

### Field Details

#### Identity Fields

| Field | Size | Description |
|-------|------|-------------|
| `msg_id` | 8 bytes | Message ID, scoped per connection (see below). |
| `channel_id` | 4 bytes | Logical channel. 0 = control channel. Odd = initiator, Even = acceptor. |
| `method_id` | 4 bytes | Method identifier (FNV-1a hash) for CALL channels. Control verb for channel 0. 0 for STREAM/TUNNEL. |

#### msg_id Semantics

r[frame.msg-id.scope]
The `msg_id` field MUST be scoped per connection (not per channel). Each peer MUST maintain a monotonically increasing counter starting at 1. Every frame sent by a peer MUST use the next value from its counter.

r[frame.msg-id.call-echo]
**CALL channel rule**: For CALL channels, the response frame MUST echo the request's `msg_id`. This enables request/response correlation for logging, tracing, and debugging.

r[frame.msg-id.stream-tunnel]
**STREAM/TUNNEL channels**: Frames on these channels MUST use monotonically increasing `msg_id` values. The `msg_id` serves for ordering verification and debugging.

r[frame.msg-id.control]
**Control channel**: Control frames (channel 0) MUST use monotonic `msg_id` values like any other frame.

**Why per-connection scope**: Per-connection monotonic IDs are simpler to implement and more useful for debugging (you can sort all frames on a connection by `msg_id` to reconstruct the timeline).

#### Payload Location Fields

| Field | Size | Description |
|-------|------|-------------|
| `payload_slot` | 4 bytes | Slot index for SHM, or `0xFFFFFFFF` for inline payload. |
| `payload_generation` | 4 bytes | Generation counter for ABA protection (SHM only). |
| `payload_offset` | 4 bytes | Byte offset within the slot (typically 0). |
| `payload_len` | 4 bytes | Payload length in bytes. |

#### Flow Control Fields

| Field | Size | Description |
|-------|------|-------------|
| `flags` | 4 bytes | Bitfield of `FrameFlags`. |
| `credit_grant` | 4 bytes | Bytes of credit granted (valid if `CREDITS` flag set). |
| `deadline_ns` | 8 bytes | Absolute deadline in nanoseconds since epoch. `0xFFFFFFFFFFFFFFFF` = no deadline. |

#### Inline Payload

| Field | Size | Description |
|-------|------|-------------|
| `inline_payload` | 16 bytes | Payload data when `payload_slot == 0xFFFFFFFF` and `payload_len <= 16`. |

### Reserved Sentinel Values

r[frame.sentinel.values]
The following sentinel values MUST be used:

| Value | Meaning |
|-------|---------|
| `payload_slot = 0xFFFFFFFF` | Payload is inline (in `inline_payload` field) |
| `payload_slot = 0xFFFFFFFE` | Reserved for future use; implementations MUST NOT use this value |
| `deadline_ns = 0xFFFFFFFFFFFFFFFF` | No deadline |

## FrameFlags

```rust
bitflags! {
    pub struct FrameFlags: u32 {
        const DATA          = 0b0000_0001;  // Frame carries payload data
        const CONTROL       = 0b0000_0010;  // Control message (channel 0 only)
        const EOS           = 0b0000_0100;  // End of stream (half-close)
        const _RESERVED_08  = 0b0000_1000;  // Reserved (do not use)
        const ERROR         = 0b0001_0000;  // Response indicates error
        const HIGH_PRIORITY = 0b0010_0000;  // Priority hint (see Prioritization)
        const CREDITS       = 0b0100_0000;  // credit_grant field is valid
        const _RESERVED_80  = 0b1000_0000;  // Reserved (do not use)
        const NO_REPLY      = 0b0001_0000_0000;  // Fire-and-forget (no response expected)
        const RESPONSE      = 0b0010_0000_0000;  // This is a response frame
    }
}
```

**Note**: [Core Protocol: FrameFlags](@/spec/core.md#frameflags) is the canonical definition. See [Prioritization](@/spec/prioritization.md) for `HIGH_PRIORITY` semantics.

## PayloadBuffer Abstraction

The payload is not necessarily contiguous with the descriptor. Different transports store payloads differently:

### Payload Storage Modes

| Mode | Condition | Storage | Zero-Copy |
|------|-----------|---------|-----------|
| **Inline** | `payload_len <= 16` | In `inline_payload` field | N/A |
| **Heap** | Stream/WebSocket transports | `Vec<u8>` or `bytes::Bytes` | No |
| **SHM Slot** | SHM transport | Shared memory slot | Yes |

### PayloadBuffer Interface

Logically, a `PayloadBuffer` provides:

```rust
trait PayloadBuffer {
    /// Borrow the payload bytes.
    fn as_ref(&self) -> &[u8];
    
    /// For SHM: the slot is freed when the guard is dropped.
    /// For heap: the buffer is deallocated.
}
```

### SHM Zero-Copy Semantics

For shared memory transports, the payload is stored in a slot within the shared memory segment:

1. **Sender** allocates a slot, writes payload, enqueues descriptor
2. **Receiver** dequeues descriptor, borrows payload via `SlotGuard`
3. **Receiver** processes payload while holding the guard
4. **Receiver** drops the guard → slot is freed back to the sender's pool

r[frame.shm.slot-guard]
The `SlotGuard` MUST ensure:
- Payload bytes remain valid for the lifetime of the guard
- The slot cannot be reused until the guard is dropped
- The generation counter prevents ABA problems

r[frame.shm.borrow-required]
Receivers MUST be able to borrow payload data without copying. Copying is permitted only if the application explicitly requests ownership.

## Payload Placement Rules

### When to Use Inline

r[frame.payload.inline]
For inline payloads (payload length ≤ 16 bytes): implementations MUST set `payload_slot = 0xFFFFFFFF`, copy payload to `inline_payload[0..payload_len]`, and `payload_offset` and `payload_generation` are ignored.

### When to Use Out-of-Line

r[frame.payload.out-of-line]
For out-of-line payloads (payload length > 16 bytes): for SHM, implementations MUST allocate a slot and set `payload_slot` to the slot index. For stream/WebSocket transports, the payload MUST follow the descriptor in the byte stream. Implementations MUST set `payload_len` to the actual length.

### Empty Payloads

r[frame.payload.empty]
Empty payloads (`payload_len = 0`) MUST be valid. Implementations MUST set `payload_slot = 0xFFFFFFFF` (inline mode); `inline_payload` contents are ignored. Empty payloads are used for EOS frames, metadata-only frames, etc.

## Descriptor Encoding on Wire

r[frame.desc.encoding]
The 64-byte `MsgDescHot` MUST be encoded as raw bytes (NOT Postcard-encoded):
- All fields MUST be little-endian
- There MUST be no padding between fields
- The total size MUST be exactly 64 bytes

This allows direct memory mapping on SHM, single memcpy for stream transports, and predictable offset calculations.

## Next Steps

- [Payload Encoding](@/spec/payload-encoding.md) – How payload contents are encoded
- [Transport Bindings](@/spec/transport-bindings.md) – How frames are sent over different transports
- [Core Protocol](@/spec/core.md) – Channel lifecycle and control messages
