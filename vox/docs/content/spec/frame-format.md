+++
title = "Frame Format"
description = "MsgDescHot descriptor and payload abstraction"
weight = 25
+++

This document defines the Rapace frame structure: the `MsgDescHot` descriptor and the `PayloadBuffer` abstraction.

## Overview

A Rapace frame consists of:

1. **MsgDescHot**: A 64-byte descriptor containing routing, flow control, and payload location info
2. **PayloadBuffer**: The postcard-encoded payload bytes (location varies by transport)

```
Rapace Frame (Logical View)

┌───────────────────────────────────────────────┐
│ MsgDescHot (64 bytes)                          │
│   Identity: msg_id, channel_id, method_id      │
│   Location: payload reference                  │
│   Control: flags, credits, deadline            │
│   Inline: small payloads (≤16 bytes)           │
├───────────────────────────────────────────────┤
│ PayloadBuffer                                  │
│   Location varies by transport                 │
│   - Inline: in descriptor                      │
│   - Stream: heap-allocated                     │
│   - SHM: slot-backed, zero-copy borrowable     │
└───────────────────────────────────────────────┘
```

## MsgDescHot (Hot-Path Descriptor)

The descriptor is **64 bytes** (one cache line) for performance:

```rust
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    pub msg_id: u64,              // Unique message ID (monotonic per session)
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

**Size**: `sizeof(MsgDescHot) == 64` (4 × 16-byte blocks = one cache line)

### Field Details

#### Identity Fields

| Field | Size | Description |
|-------|------|-------------|
| `msg_id` | 8 bytes | Monotonically increasing ID, unique per session. Used for correlation and debugging. |
| `channel_id` | 4 bytes | Logical channel. 0 = control channel. Odd = initiator, Even = acceptor. |
| `method_id` | 4 bytes | Method identifier (FNV-1a hash) for CALL channels. Control verb for channel 0. 0 for STREAM/TUNNEL. |

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

| Value | Meaning |
|-------|---------|
| `payload_slot = 0xFFFFFFFF` | Payload is inline (in `inline_payload` field) |
| `payload_slot = 0xFFFFFFFE` | Reserved for future use |
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

The `SlotGuard` ensures:
- Payload bytes are valid for the lifetime of the guard
- Slot cannot be reused until guard is dropped
- Generation counter prevents ABA problems

**Important**: Receivers MUST be able to borrow payload data without copying. Copying is permitted only if the application explicitly requests ownership.

## Payload Placement Rules

### When to Use Inline

- Payload length ≤ 16 bytes
- Set `payload_slot = 0xFFFFFFFF`
- Copy payload to `inline_payload[0..payload_len]`
- `payload_offset` and `payload_generation` are ignored

### When to Use Out-of-Line

- Payload length > 16 bytes
- For SHM: allocate slot, set `payload_slot` to slot index
- For stream/WebSocket: payload follows descriptor in the byte stream
- Set `payload_len` to actual length

### Empty Payloads

Empty payloads (`payload_len = 0`) are valid:
- Set `payload_slot = 0xFFFFFFFF` (inline mode)
- `inline_payload` contents are ignored
- Used for EOS frames, metadata-only frames, etc.

## Descriptor Encoding on Wire

The 64-byte `MsgDescHot` is always encoded as raw bytes (not postcard-encoded):

- All fields are little-endian
- No padding between fields
- Total size is always exactly 64 bytes

This allows:
- Direct memory mapping on SHM
- Single memcpy for stream transports
- Predictable offset calculations

## Next Steps

- [Payload Encoding](@/spec/payload-encoding.md) – How payload contents are encoded
- [Transport Bindings](@/spec/transport-bindings.md) – How frames are sent over different transports
- [Core Protocol](@/spec/core.md) – Channel lifecycle and control messages
