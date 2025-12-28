+++
title = "Transport Requirements"
description = "Requirements for implementing new transports"
weight = 95
+++

This document defines requirements for implementing new Rapace transports. It specifies the transport abstraction layer, required capabilities, and guidance for new implementations.

For concrete bindings (TCP, WebSocket, SHM), see [Transport Bindings](@/spec/transport-bindings.md).

## Transport Abstraction

### Core Trait

All transports implement a common interface:

```rust
pub trait Transport: Send + Sync + 'static {
    /// Send a frame to the peer.
    fn send_frame(&self, frame: Frame) -> impl Future<Output = Result<(), TransportError>> + Send;

    /// Receive a frame from the peer.
    fn recv_frame(&self) -> impl Future<Output = Result<Frame, TransportError>> + Send;

    /// Get the buffer pool for payload allocation.
    fn buffer_pool(&self) -> &BufferPool;

    /// Close the transport gracefully.
    fn close(&self) -> impl Future<Output = Result<(), TransportError>> + Send;

    /// Check if the transport is still connected.
    fn is_connected(&self) -> bool;
}
```

### Transport Error

```rust
pub enum TransportError {
    /// Connection closed gracefully.
    Closed,
    /// I/O error (network, file descriptor, etc.).
    Io(std::io::Error),
    /// Frame too large for transport.
    FrameTooLarge { size: usize, max: usize },
    /// Protocol violation (malformed frame, etc.).
    Protocol(String),
    /// Resource exhausted (buffers, slots, etc.).
    ResourceExhausted,
    /// Operation timed out.
    Timeout,
}
```

## Required Capabilities

Every transport MUST provide these capabilities:

### 1. Reliable Delivery

r[transport.reliable.delivery]
Frames MUST be delivered exactly once, in order, or not at all (with error).

| Requirement | Description |
|-------------|-------------|
| No duplication | Each frame is delivered at most once |
| No corruption | Frame content is intact or error is reported |
| Ordering | Frames are delivered in send order |
| Error detection | Delivery failures are reported to sender |

### 2. Framing

r[transport.framing.boundaries]
The transport MUST preserve frame boundaries:

```
┌─────────────────────────────────────────┐
│ Frame 1 (complete)                      │
├─────────────────────────────────────────┤
│ Frame 2 (complete)                      │
├─────────────────────────────────────────┤
│ Frame 3 (complete)                      │
└─────────────────────────────────────────┘
```

r[transport.framing.no-coalesce]
Rapace does NOT support message coalescing or splitting at the transport layer. Each `send_frame` call MUST result in exactly one `recv_frame` call on the peer.

### 3. Bidirectional Communication

Both peers can send and receive simultaneously:

```
Peer A                              Peer B
   │                                   │
   │───── Frame 1 ────────────────────▶│
   │◀──────────────── Frame 2 ─────────│
   │───── Frame 3 ────────────────────▶│
   │───── Frame 4 ────────────────────▶│
   │◀──────────────── Frame 5 ─────────│
```

### 4. Graceful Shutdown

r[transport.shutdown.orderly]
Transport MUST support orderly shutdown:

1. `close()` signals intent to close
2. Pending sends MUST complete or fail
3. Peer MUST be notified of closure
4. Resources MUST be released

### 5. Buffer Pool Integration

r[transport.buffer-pool]
Transports MUST provide a `BufferPool` for payload allocation:

```rust
fn buffer_pool(&self) -> &BufferPool;
```

This enables:
- Pooled allocation for reduced memory churn
- Zero-copy paths for SHM transports
- Consistent buffer management across transports

## Optional Capabilities

Transports MAY provide these enhanced capabilities:

### Zero-Copy Payloads

For SHM transports, payloads can be passed by reference:

```rust
pub trait ZeroCopyTransport: Transport {
    /// Send a frame with a slot reference (no copy).
    fn send_frame_zero_copy(&self, frame: Frame, slot: SlotGuard) 
        -> impl Future<Output = Result<(), TransportError>> + Send;
}
```

### Native Multiplexing

Some transports (QUIC, HTTP/2) have native stream multiplexing:

```rust
pub trait MultiplexedTransport: Transport {
    /// Open a new native stream (maps to Rapace channel).
    fn open_stream(&self) -> impl Future<Output = Result<StreamId, TransportError>> + Send;
    
    /// Accept an incoming native stream.
    fn accept_stream(&self) -> impl Future<Output = Result<StreamId, TransportError>> + Send;
    
    /// Send on a specific stream.
    fn send_on_stream(&self, stream: StreamId, frame: Frame)
        -> impl Future<Output = Result<(), TransportError>> + Send;
}
```

### Vectored I/O

For scatter-gather optimization:

```rust
pub trait VectoredTransport: Transport {
    /// Send multiple frames in one syscall.
    fn send_frames(&self, frames: &[Frame])
        -> impl Future<Output = Result<(), TransportError>> + Send;
}
```

### Urgent Data

For out-of-band control messages:

```rust
pub trait UrgentTransport: Transport {
    /// Send high-priority frame (may bypass normal queue).
    fn send_urgent(&self, frame: Frame)
        -> impl Future<Output = Result<(), TransportError>> + Send;
}
```

## Ordering Guarantees

### Single Connection

r[transport.ordering.single]
All frames on a single connection MUST be ordered:

```
send(F1) happens-before send(F2) → recv(F1) happens-before recv(F2)
```

### Multiple Connections

No ordering guarantees across connections. Application must handle:
- Request IDs for correlation
- Logical clocks for causality
- Explicit synchronization

### QUIC Streams

For QUIC with multiple streams:

| Mode | Ordering |
|------|----------|
| Single stream | Total order (like TCP) |
| Per-channel streams | Ordered within channel |
| Stream per message | No ordering guarantees |

r[transport.ordering.channel]
Rapace's channel abstraction MUST provide ordering within each channel regardless of underlying transport streams.

## Keepalive and Liveness

### Transport-Level Keepalive

r[transport.keepalive.transport]
Transports SHOULD implement keepalive:

| Transport | Mechanism |
|-----------|-----------|
| TCP | `SO_KEEPALIVE` socket option |
| WebSocket | WebSocket ping/pong frames |
| QUIC | QUIC PING frames |
| SHM | Not needed (shared memory) |

### Rapace-Level Keepalive

Independent of transport keepalive:

- Uses Ping/Pong control messages (verbs 5/6)
- Detects application-level liveliness
- Works through proxies that don't forward transport pings

### Timeout Recommendations

| Transport | Connect Timeout | Read Timeout | Keepalive Interval |
|-----------|-----------------|--------------|-------------------|
| TCP (local) | 1s | 30s | 30s |
| TCP (remote) | 10s | 60s | 60s |
| WebSocket | 10s | 60s | 30s |
| QUIC | 5s | 30s | 15s |
| SHM | 100ms | 5s | N/A |

## Flow Control Integration

### Credit-Based Flow Control

Transports participate in Rapace's credit-based flow control:

```rust
impl Transport for MyTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        // Check if we have credits (or credits are in the frame)
        // Transport doesn't enforce credits; Rapace layer does
        self.inner_send(frame).await
    }
}
```

### Backpressure Signaling

r[transport.backpressure]
Transports SHOULD propagate backpressure:

```rust
impl Transport for MyTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        // Block if send buffer is full (backpressure)
        self.wait_for_capacity().await?;
        self.inner_send(frame).await
    }
}
```

### SHM Slot-Based Flow Control

For SHM transports, slot availability IS the flow control:

```rust
impl Transport for ShmTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        // alloc_slot blocks until slot available
        let slot = self.alloc_slot().await?;
        // Write frame to slot
        // Receiver frees slot after processing
    }
}
```

## Implementation Checklist

### Required

- [ ] Implement `Transport` trait
- [ ] Reliable, ordered delivery
- [ ] Frame boundary preservation
- [ ] Bidirectional communication
- [ ] Graceful shutdown
- [ ] Buffer pool integration
- [ ] Error handling and reporting

### Recommended

- [ ] Keepalive mechanism
- [ ] Backpressure propagation
- [ ] Connection pooling support
- [ ] Metrics and logging hooks
- [ ] TLS/security support

### Optional

- [ ] Zero-copy payloads
- [ ] Native multiplexing
- [ ] Vectored I/O
- [ ] Urgent data path

## QUIC Transport Considerations

QUIC is a natural fit for Rapace due to native multiplexing.

### Stream Mapping Options

| Option | Description | Trade-offs |
|--------|-------------|------------|
| Single stream | All Rapace frames on one QUIC stream | Simple, but head-of-line blocking |
| Stream per channel | Each Rapace channel = QUIC stream | Good parallelism, natural mapping |
| Stream per message | New stream for each message | Maximum parallelism, overhead |

**Recommended**: Stream per channel with control channel on stream 0.

### QUIC-Specific Features

| Feature | Rapace Usage |
|---------|--------------|
| 0-RTT | Fast reconnection (if session tickets available) |
| Stream priorities | Map to `rapace.priority` |
| Flow control | Use QUIC's native flow control |
| Connection migration | Transparent to Rapace layer |

### Example Mapping

```
QUIC Connection
├── Stream 0: Rapace control channel
├── Stream 1: Rapace channel 1 (initiator call)
├── Stream 2: Rapace channel 2 (acceptor call)
├── Stream 3: Rapace channel 3 (initiator stream)
└── ...
```

## HTTP/3 Transport Considerations

HTTP/3 (QUIC-based) can carry Rapace:

### WebTransport

Use [WebTransport](https://www.w3.org/TR/webtransport/) for browser support:

```javascript
const transport = new WebTransport('https://example.com/rapace');
await transport.ready;

// Bidirectional streams map to Rapace channels
const stream = await transport.createBidirectionalStream();
```

### HTTP CONNECT

For proxying through HTTP/3:

```
CONNECT rapace://service.example.com HTTP/3
```

## Testing Transports

### Required Tests

1. **Round-trip**: Send frame, receive response
2. **Ordering**: Send N frames, verify order preserved
3. **Large frames**: Send max-size frame
4. **Concurrent**: Multiple channels simultaneously
5. **Shutdown**: Graceful close, pending frame handling
6. **Error handling**: Connection drop, timeout

### Stress Tests

1. **Throughput**: Maximum frames/second
2. **Latency**: P50/P95/P99 under load
3. **Memory**: No leaks under sustained load
4. **Connections**: Many concurrent connections

### Interop Tests

1. **Cross-language**: Rust ↔ TypeScript ↔ Swift
2. **Version compatibility**: Old client, new server
3. **Proxy traversal**: Through load balancers, proxies

## Next Steps

- [Transport Bindings](@/spec/transport-bindings.md) – Concrete TCP, WebSocket, SHM bindings
- [Frame Format](@/spec/frame-format.md) – Frame structure
- [Handshake & Capabilities](@/spec/handshake.md) – Transport-specific handshake variations
