+++
title = "Transport Bindings"
description = "How frames are sent over TCP, WebSocket, WebTransport, and shared memory"
weight = 30
+++

This document defines how Rapace frames are transmitted over different transports.

## Overview

Rapace supports multiple transports with different characteristics:

| Transport | Use Case | Max Payload | Zero-Copy | Browser |
|-----------|----------|-------------|-----------|---------|
| Stream (TCP, Unix) | General networking | Negotiated (1-16 MB) | No | No |
| WebSocket | Browser, firewalls | Negotiated (16 MB) | No | Yes |
| WebTransport | Modern browser, QUIC | Negotiated (16 MB) | No | Yes |
| SHM Pair | Same-host 1:1 IPC | `slot_size` (4 KB) | Yes | No |
| SHM Hub | Same-host 1:N IPC | 16 MB | Yes | No |

Each transport defines:
- How frames are delimited
- Where payloads are stored
- Maximum payload size
- Signaling mechanism

## Stream Transports (TCP, Unix Sockets)

### Frame Encoding

```
┌────────────────────────────────────────────┐
│ Length (varint, 1-10 bytes)                │
├────────────────────────────────────────────┤
│ MsgDescHot (64 bytes, raw little-endian)   │
├────────────────────────────────────────────┤
│ Payload (N bytes, postcard-encoded)        │
└────────────────────────────────────────────┘
```

The length prefix is a varint encoding of `64 + payload_len`.

### Why Length Prefix?

Stream transports use a length prefix (even though `payload_len` is in the descriptor) for:

1. **Early rejection**: Validate `max_frame_size` before reading/allocating the body
2. **Attack resistance**: Reject oversized frames without buffering any payload
3. **Simple framing**: One "read exactly N bytes" loop instead of two-phase parsing
4. **Opaque forwarding**: Relays/recorders can copy frames without parsing the descriptor

WebSocket and SHM transports don't need this because they have built-in message boundaries.

### Reading Frames

1. Read varint length prefix (max 10 bytes; if continuation bit still set after 10, reject as malformed)
2. **Validate**: If length < 64, reject as malformed (frame too small for descriptor)
3. **Validate**: If length > `max_payload_size + 64`, reject immediately (do not allocate)

> **Note**: `max_frame_size = max_payload_size + 64` (descriptor size). The handshake negotiates `max_payload_size`; frame size is derived.
4. Allocate buffer of `length` bytes
5. Read exactly `length` bytes into buffer
6. Parse first 64 bytes as `MsgDescHot`
7. **Validate**: If `payload_len` in descriptor != `length - 64`, reject as protocol error (prevents desync)
8. Remaining bytes are the payload

### Writing Frames

1. Encode payload:
   - For CALL and STREAM channels: Postcard-encode the payload
   - For TUNNEL channels: Use raw bytes (no encoding)
2. Write varint of `64 + payload.len()`
3. Write 64-byte `MsgDescHot` (little-endian)
4. Write payload bytes

### Validation Rules (Normative)

Receivers MUST enforce:

| Condition | Action |
|-----------|--------|
| Varint > 10 bytes (continuation bit still set) | Reject as malformed, close connection |
| Non-canonical varint (see below) | Reject as malformed, close connection |
| Length < 64 | Reject as malformed, close connection |
| Length > `max_payload_size + 64` | Reject before allocation, close connection |
| `payload_len` != `length - 64` | Reject as protocol error, close connection |

**Canonical varint requirement**: The length prefix MUST be encoded in canonical form (shortest possible encoding). A non-canonical encoding (e.g., `[0x80, 0x00]` for the value 0) is a protocol error. This prevents ambiguity and ensures consistent behavior across implementations. See [Payload Encoding: Varint Canonicalization](@/spec/payload-encoding.md#varint-canonicalization) for the general rule.

These rules prevent:
- Varint overflow attacks
- Non-canonical encoding ambiguities
- Frames too small to contain a valid descriptor
- Memory exhaustion from oversized frames
- Desync from mismatched length prefix vs descriptor

### Size Limits

- `max_payload_size` is negotiated during handshake (frame size = payload + 64)
- Frames exceeding `max_payload_size + 64` MUST be rejected before allocation
- If payload is too large, use a STREAM channel and chunk the data

## WebSocket Transport

### Frame Encoding

Each WebSocket **binary message** contains exactly one Rapace frame:

```
┌────────────────────────────────────────────┐
│ MsgDescHot (64 bytes, raw little-endian)   │
├────────────────────────────────────────────┤
│ Payload (N bytes, postcard-encoded)        │
└────────────────────────────────────────────┘
```

No additional length prefix (WebSocket provides message framing).

### Reading Frames

1. Receive binary WebSocket message
2. **Validate**: If message length < 64, reject as malformed
3. **Validate**: If message length > `max_payload_size + 64`, reject
4. Parse first 64 bytes as `MsgDescHot`
5. Remaining bytes are the payload

### Writing Frames

1. Encode payload (postcard)
2. Allocate buffer of `64 + payload.len()` bytes
3. Write 64-byte `MsgDescHot` (little-endian)
4. Write payload bytes
5. Send as single binary WebSocket message

### Ping/Pong

WebSocket has its own ping/pong mechanism. Rapace-level Ping/Pong (control verbs 5/6) is separate and used for application-level liveness checking.

Negotiate `RAPACE_PING` capability to determine which to use.

## WebTransport

[WebTransport](https://www.w3.org/TR/webtransport/) provides QUIC-based communication from browsers with support for multiple streams, datagrams, and connection migration.

### Why WebTransport?

| Feature | WebSocket | WebTransport |
|---------|-----------|--------------|
| Protocol | TCP | QUIC (UDP) |
| Head-of-line blocking | Yes (single stream) | No (multiple streams) |
| Connection migration | No | Yes |
| 0-RTT reconnection | No | Yes |
| Datagrams | No | Yes |
| Native multiplexing | No | Yes |

WebTransport is the recommended browser transport for new deployments.

### Connection Establishment

```javascript
// Client (browser)
const transport = new WebTransport('https://example.com/rapace');
await transport.ready;

// Use bidirectional stream for Rapace
const stream = await transport.createBidirectionalStream();
const writer = stream.writable.getWriter();
const reader = stream.readable.getReader();
```

Server endpoints MUST:
1. Serve over HTTPS (WebTransport requires TLS)
2. Handle the WebTransport handshake at the specified path
3. Support bidirectional streams

### Stream Mapping

Two modes are supported:

#### Single Stream Mode (Simple)

All Rapace frames on one bidirectional stream:

```
WebTransport Connection
└── Bidirectional Stream 0: All Rapace frames
```

Frame encoding is identical to WebSocket (no length prefix):

```
┌────────────────────────────────────────────┐
│ MsgDescHot (64 bytes, raw little-endian)   │
├────────────────────────────────────────────┤
│ Payload (N bytes, postcard-encoded)        │
└────────────────────────────────────────────┘
```

#### Multi-Stream Mode (Advanced)

Map Rapace channels to WebTransport streams:

```
WebTransport Connection
├── Bidirectional Stream 0: Rapace control channel (ch 0)
├── Bidirectional Stream 1: Rapace channel 1
├── Bidirectional Stream 2: Rapace channel 2
└── ...
```

Benefits:
- No head-of-line blocking between channels
- Native priority support (QUIC stream priorities)
- Independent flow control per channel

Negotiate multi-stream mode via handshake capability bit 4 (`WEBTRANSPORT_MULTI_STREAM`). See [Handshake: Defined Feature Bits](@/spec/handshake.md#defined-feature-bits).

### Datagram Support

WebTransport datagrams can be used for unreliable messages:

```javascript
// Send unreliable datagram
const writer = transport.datagrams.writable.getWriter();
await writer.write(frameBytes);

// Receive datagrams
const reader = transport.datagrams.readable.getReader();
const { value } = await reader.read();
```

Use cases:
- Real-time telemetry (acceptable to lose some samples)
- Ping/pong with tolerance for loss
- Streaming media (video/audio frames)

Datagrams use the same frame format but:
- MUST NOT be used for CALL channels (require reliability)
- MUST NOT be used for TUNNEL channels (byte stream semantics require ordering)
- MAY be used for STREAM channels marked as unreliable via `rapace.unreliable` metadata
- Size limited by QUIC datagram MTU (~1200 bytes)
- Requires handshake capability bit 5 (`WEBTRANSPORT_DATAGRAMS`) to be negotiated

To mark a STREAM channel as unreliable:
```rust
OpenChannel {
    kind: ChannelKind::Stream,
    metadata: vec![
        ("rapace.unreliable".into(), vec![1]),  // Enable unreliable delivery
    ],
    // ...
}
```

When unreliable delivery is enabled, the transport MAY use WebTransport datagrams instead of streams for that channel. Items may be lost or arrive out of order.

### Server Implementation

```rust
// Rust server using h3-webtransport
async fn handle_webtransport(session: WebTransportSession) {
    // Accept the incoming Rapace connection
    let stream = session.accept_bi().await?;
    
    // Wrap in Rapace transport
    let transport = WebTransportRapace::new(stream);
    
    // Handle Rapace protocol
    handle_rapace_connection(transport).await
}
```

### Browser Client Implementation

```typescript
class WebTransportRapaceClient {
    private transport: WebTransport;
    private writer: WritableStreamDefaultWriter<Uint8Array>;
    private reader: ReadableStreamDefaultReader<Uint8Array>;
    
    static async connect(url: string): Promise<WebTransportRapaceClient> {
        const transport = new WebTransport(url);
        await transport.ready;
        
        const stream = await transport.createBidirectionalStream();
        return new WebTransportRapaceClient(
            transport,
            stream.writable.getWriter(),
            stream.readable.getReader()
        );
    }
    
    async sendFrame(frame: Uint8Array): Promise<void> {
        await this.writer.write(frame);
    }
    
    async recvFrame(): Promise<Uint8Array> {
        const { value, done } = await this.reader.read();
        if (done) throw new Error('Connection closed');
        return value;
    }
    
    close(): void {
        this.transport.close();
    }
}
```

### Fallback to WebSocket

For browsers that don't support WebTransport:

```typescript
async function connect(url: string): Promise<RapaceTransport> {
    if ('WebTransport' in window) {
        try {
            return await WebTransportRapaceClient.connect(url.replace('wss:', 'https:'));
        } catch (e) {
            console.warn('WebTransport failed, falling back to WebSocket', e);
        }
    }
    return await WebSocketRapaceClient.connect(url);
}
```

### Size Limits

- Stream messages: Negotiated `max_payload_size` (typically 16 MB)
- Datagrams: ~1200 bytes (QUIC MTU minus overhead)

## Shared Memory Transport (Pair)

The "pair" SHM transport is for 1:1 communication between two processes.

### Memory Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│  Segment Header (128 bytes)                                         │
│    magic, version, config, peer epochs, futex words                 │
├─────────────────────────────────────────────────────────────────────┤
│  A→B Descriptor Ring (SPSC queue of MsgDescHot)                     │
├─────────────────────────────────────────────────────────────────────┤
│  B→A Descriptor Ring (SPSC queue of MsgDescHot)                     │
├─────────────────────────────────────────────────────────────────────┤
│  Data Segment Header (64 bytes)                                     │
├─────────────────────────────────────────────────────────────────────┤
│  Slot Metadata Array (generation + state per slot)                  │
├─────────────────────────────────────────────────────────────────────┤
│  Slot Data (slot_count × slot_size bytes)                           │
└─────────────────────────────────────────────────────────────────────┘
```

### Sending a Frame

1. **Payload ≤ 16 bytes (inline)**:
   - Set `payload_slot = 0xFFFFFFFF`
   - Copy to `inline_payload`
   - Enqueue descriptor to ring

2. **Payload > 16 bytes**:
   - Allocate slot from Treiber stack (lock-free)
   - Copy payload to slot
   - Mark slot as `InFlight`
   - Set `payload_slot`, `payload_generation`, `payload_len`
   - Enqueue descriptor to ring
   - Signal peer via futex

### Receiving a Frame

1. Dequeue descriptor from ring
2. Signal peer that ring space is available (futex)
3. If `payload_slot == 0xFFFFFFFF`:
   - Payload is in `inline_payload[0..payload_len]`
4. Else:
   - Create `SlotGuard` referencing the slot
   - Borrow payload via guard (zero-copy)
   - Process payload
   - Drop guard → slot freed back to sender's pool

### Slot Lifecycle

```
Free → Allocated → InFlight → Free
       (sender)   (sender)   (receiver frees)
```

Generation counter increments on each allocation, preventing ABA problems.

### Size Limits

- Default `slot_size`: 4096 bytes (configurable)
- Payloads > `slot_size` fail with `PayloadTooLarge`
- For large data, use STREAM channels and chunk

## Shared Memory Transport (Hub)

The "hub" SHM transport is for 1:N communication (one host, multiple peers).

### Size Classes

The hub uses tiered size classes to support payloads up to 16 MB:

| Class | Slot Size | Initial Slots | Total Pool |
|-------|-----------|---------------|------------|
| 0 | 1 KB | 1024 | 1 MB |
| 1 | 16 KB | 256 | 4 MB |
| 2 | 256 KB | 32 | 8 MB |
| 3 | 4 MB | 8 | 32 MB |
| 4 | 16 MB | 4 | 64 MB |

### Allocation

1. Find smallest size class that fits the payload
2. Try to allocate from that class
3. If exhausted, try larger classes
4. If all exhausted, wait for slot availability (futex)

### Slot Reference Encoding

Hub slot references encode both class and index in `payload_slot`:

```
Bits 31-29: Size class (0-7)
Bits 28-0:  Global index within class
```

### Memory Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│  Hub Header (256 bytes)                                             │
├─────────────────────────────────────────────────────────────────────┤
│  Peer Table (max_peers × 64 bytes)                                  │
├─────────────────────────────────────────────────────────────────────┤
│  Ring Region (per-peer send/recv rings)                             │
├─────────────────────────────────────────────────────────────────────┤
│  Size Class Headers (5 × 128 bytes)                                 │
├─────────────────────────────────────────────────────────────────────┤
│  Extent Region (slot data organized by size class)                  │
└─────────────────────────────────────────────────────────────────────┘
```

## Signaling Mechanisms

### Futex (SHM, Linux)

SHM transports use futex for cross-process signaling:

| Futex | Purpose |
|-------|---------|
| `a_to_b_data_futex` | A signals B that data is available |
| `a_to_b_space_futex` | B signals A that ring space is available |
| `b_to_a_data_futex` | B signals A that data is available |
| `b_to_a_space_futex` | A signals B that ring space is available |
| `slot_available_futex` | Signal that a slot was freed |

Signal pattern:
```rust
futex.fetch_add(1, Ordering::Release);
futex_wake(&futex, u32::MAX);
```

Wait pattern:
```rust
let current = futex.load(Ordering::Acquire);
futex_wait(&futex, current, timeout);
```

### Doorbell (SHM, macOS/cross-platform)

On platforms without futex, Unix socketpairs provide signaling:

- Send 1-byte datagram to signal
- `poll`/`epoll`/`kqueue` for async waiting

## Handshake Considerations

### Limit Exchange

During handshake, peers exchange:
- `max_payload_size`: Maximum payload bytes per frame
- `max_channels`: Maximum concurrent channels

Effective limits are the minimum of both peers.

### Transport Detection

The transport type is typically known from context:
- TCP connection → stream transport
- WebSocket upgrade → WebSocket transport
- SHM file descriptor → SHM transport

The handshake protocol is the same; only framing differs.

## Summary

| Aspect | Stream | WebSocket | WebTransport | SHM Pair | SHM Hub |
|--------|--------|-----------|--------------|----------|---------|
| Length prefix | Varint | None | None | None | None |
| Payload location | After desc | After desc | After desc | Slot/inline | Slot/inline |
| Max payload | Negotiated | Negotiated | Negotiated | `slot_size` | 16 MB |
| Zero-copy receive | No | No | No | Yes | Yes |
| Signaling | TCP | WS events | QUIC | Futex | Futex |
| Browser support | No | Yes | Yes (modern) | No | No |
| Multiplexing | Rapace | Rapace | Native+Rapace | Rapace | Rapace |

## Next Steps

- [Frame Format](@/spec/frame-format.md) – MsgDescHot structure
- [Core Protocol](@/spec/core.md) – Channel lifecycle
- [Handshake & Capabilities](@/spec/handshake.md) – Limit negotiation
