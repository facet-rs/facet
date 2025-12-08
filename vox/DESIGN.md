# rapace Design Document

A production-grade RPC system with transport-agnostic service traits, facet-driven encoding,
and zero-copy shared memory as the reference transport.

---

# Part I — Transport Layer

This section describes how bytes move between peers. The SHM transport is the reference
implementation; other transports implement the same abstract interface.

## 1. Overview & Goals

### What rapace provides

1. **Single API, no transport leakage** — Plugin authors write normal Rust traits:
   ```rust
   trait Hasher {
       fn calculate_sha256sum(&self, buf: &[u8]) -> [u8; 32];
   }
   ```
   The same trait works whether the peer is in-proc, SHM, TCP, or WebSocket.

2. **Transport-aware, opportunistic zero-copy** — The RPC layer chooses the best strategy:
   - **In-proc**: Direct trait calls, real borrows, no serialization
   - **SHM**: If a `&[u8]` points into SHM → send borrowed (slot+offset+len); else memcpy
   - **Network**: Always serialize; no borrows across machines

3. **Facet at the center** — All types implement facet. The reflection system enables:
   - Transport-specific encoding (postcard for wire, zero-copy views for SHM)
   - Service registry schemas
   - Introspection APIs
   - Future: diff/patch for `&mut T` across transports

4. **Async-native** — eventfd doorbells, tokio integration, no polling

5. **Full observability** — trace_id, span_id, timestamps, telemetry rings

6. **Crash-safe** — Generation counters, dead peer detection, graceful recovery

### Comparison

| vs. | rapace advantage |
|-----|-------------------|
| Unix domain sockets | Zero-copy for large payloads, no kernel transitions on hot path |
| gRPC over loopback | ~10-100× lower latency, no HTTP/2 overhead, SHM for bulk data |
| boost::interprocess | Async-native, built-in RPC semantics, observability, flow control |
| Custom SHM queues | Production-ready: cancellation, deadlines, crash recovery, introspection |

## 2. Transport Trait

All transports implement this core interface:

```rust
/// A transport moves frames between two peers.
///
/// Transports are responsible for:
/// - Frame serialization/deserialization
/// - Flow control at the transport level
/// - Delivering frames reliably (within a session)
///
/// Transports are NOT responsible for:
/// - RPC semantics (channels, methods, deadlines)
/// - Service dispatch
/// - Schema management
trait Transport: Send + Sync {
    /// Send a frame to the peer.
    ///
    /// The frame is borrowed for the duration of the call. The transport
    /// may copy it (stream), reference it (in-proc), or encode it into
    /// SHM slots depending on implementation.
    fn send_frame(&self, frame: &Frame) -> impl Future<Output = Result<(), TransportError>>;

    /// Receive the next frame from the peer.
    ///
    /// Returns a FrameView with lifetime tied to internal buffers.
    /// Caller must process or copy before calling recv_frame again.
    fn recv_frame(&self) -> impl Future<Output = Result<FrameView<'_>, TransportError>>;

    /// Create an encoder context for building outbound frames.
    ///
    /// The encoder is transport-specific: SHM encoders can reference
    /// existing SHM data; stream encoders always copy.
    fn encoder(&self) -> Box<dyn EncodeCtx + '_>;

    /// Graceful shutdown.
    fn close(&self) -> impl Future<Output = Result<(), TransportError>>;
}

/// Object-safe version for dynamic dispatch
trait DynTransport: Send + Sync {
    fn send_frame_boxed(&self, frame: &Frame) -> BoxFuture<'_, Result<(), TransportError>>;
    fn recv_frame_boxed(&self) -> BoxFuture<'_, Result<OwnedFrame, TransportError>>;
    fn encoder_boxed(&self) -> Box<dyn EncodeCtx + '_>;
    fn close_boxed(&self) -> BoxFuture<'_, Result<(), TransportError>>;
}
```

## 3. Shared Memory Transport (Reference Implementation)

### 3.1 Scope & Model

**Each SHM segment represents a session between exactly two peers (A and B).**

The design is intentionally SPSC (single-producer, single-consumer) per ring direction.
This keeps the ring implementation simple and correct.

For N-way topologies:
- Host maintains N separate sessions (one per plugin)
- Or use a broker pattern with `rapace-bus`

Non-goals for v1:
- MPMC rings (complexity not worth it)
- Cross-machine transport (use stream transport for that)
- Encryption (peers are on same machine, use OS isolation)

### 3.2 Memory Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Shared Memory Layout                         │
├─────────────────────────────────────────────────────────────────────┤
│  Control Segment                                                     │
│  ┌─────────────┬─────────────┬─────────────┬─────────────────────┐  │
│  │ Header      │ Service     │ A→B Desc    │ B→A Desc            │  │
│  │ (version,   │ Registry    │ Ring        │ Ring                │  │
│  │  epoch,     │ (methods,   │ (MsgDesc    │ (MsgDesc            │  │
│  │  doorbells) │  schemas)   │  entries)   │  entries)           │  │
│  └─────────────┴─────────────┴─────────────┴─────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│  Data Segment                                                        │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ Slab allocator for payloads                                   │   │
│  │ [slot 0][slot 1][slot 2]...[slot N]                          │   │
│  │ Each slot: generation + data                                  │   │
│  └──────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.3 Segment Header (64 bytes, cache-line aligned)

```rust
#[repr(C, align(64))]
struct SegmentHeader {
    magic: u64,                    // "RAPACE2\0"
    version: u32,                  // Protocol version (major.minor)
    flags: u32,                    // Feature flags

    // Peer liveness
    peer_a_epoch: AtomicU64,       // Incremented by peer A periodically
    peer_b_epoch: AtomicU64,       // Incremented by peer B periodically
    peer_a_last_seen: AtomicU64,   // Timestamp (nanos since epoch)
    peer_b_last_seen: AtomicU64,   // Timestamp

    // Doorbell eventfds (exchanged out-of-band via Unix socket)
}
```

### 3.4 Message Descriptor — Hot Path (64 bytes, one cache line)

```rust
#[repr(C, align(64))]
struct MsgDescHot {
    // Identity (16 bytes)
    msg_id: u64,                   // Unique per session, monotonic
    channel_id: u32,               // Logical stream (0 = control channel)
    method_id: u32,                // For RPC dispatch, or control verb

    // Payload location (16 bytes)
    payload_slot: u32,             // Slot index (u32::MAX = inline)
    payload_generation: u32,       // Generation counter for ABA safety
    payload_offset: u32,           // Offset within slot
    payload_len: u32,              // Actual payload length

    // Flow control & flags (8 bytes)
    flags: u32,                    // EOS, CANCEL, ERROR, etc.
    credit_grant: u32,             // Credits being granted to peer

    // Inline payload for small messages (24 bytes)
    inline_payload: [u8; 24],
}

const INLINE_PAYLOAD_SIZE: usize = 24;
```

**Inline payload rules:**
- The `inline_payload` field has no alignment guarantees beyond `u8`.
- When `payload_slot == u32::MAX`, payload lives in `inline_payload[0..payload_len]`.
- When inline, `payload_generation` and `payload_offset` MUST be zero.

### 3.5 Message Descriptor — Cold Path (Observability)

Stored in a parallel array or separate telemetry ring. Can be disabled for performance.

```rust
#[repr(C, align(64))]
struct MsgDescCold {
    msg_id: u64,                   // Correlates with hot descriptor
    trace_id: u64,                 // Distributed tracing
    span_id: u64,                  // Span within trace
    parent_span_id: u64,           // Parent span
    timestamp_ns: u64,             // When enqueued
    debug_level: u32,              // 0=off, 1=metadata, 2=full payload
    _reserved: u32,
}
```

### 3.6 Descriptor Ring (SPSC)

```rust
#[repr(C)]
struct DescRing {
    // Producer publication index (cache-line aligned)
    visible_head: AtomicU64,
    _pad1: [u8; 56],

    // Consumer side (separate cache line)
    tail: AtomicU64,
    _pad2: [u8; 56],

    // Ring configuration
    capacity: u32,                 // Power of 2
    _pad3: [u8; 60],

    // Descriptors follow (capacity × sizeof(MsgDescHot))
}
```

**Ring algorithm:**

```rust
impl DescRing {
    /// Producer: enqueue a descriptor
    /// `local_head` is producer-private (stack-local, not in SHM)
    fn enqueue(&self, local_head: &mut u64, desc: &MsgDescHot) -> Result<(), RingFull> {
        let tail = self.tail.load(Ordering::Acquire);

        if local_head.wrapping_sub(tail) >= self.capacity as u64 {
            return Err(RingFull);
        }

        let idx = (*local_head & (self.capacity as u64 - 1)) as usize;
        unsafe { std::ptr::write(self.desc_slot(idx), *desc); }

        *local_head += 1;
        self.visible_head.store(*local_head, Ordering::Release);
        Ok(())
    }

    /// Consumer: dequeue a descriptor
    fn dequeue(&self) -> Option<MsgDescHot> {
        let tail = self.tail.load(Ordering::Relaxed);
        let visible = self.visible_head.load(Ordering::Acquire);

        if tail >= visible {
            return None;
        }

        let idx = (tail & (self.capacity as u64 - 1)) as usize;
        let desc = unsafe { std::ptr::read(self.desc_slot(idx)) };
        self.tail.store(tail + 1, Ordering::Release);
        Some(desc)
    }
}
```

**Invariants:**
- Producer's `local_head` is stack-local, never shared
- `visible_head` is the publication barrier
- Ring is empty when `tail == visible_head`
- Ring is full when `local_head - tail >= capacity`

### 3.7 Data Segment (Slab Allocator)

```rust
#[repr(C)]
struct DataSegment {
    slot_size: u32,                // e.g., 4KB per slot
    slot_count: u32,
    max_frame_size: u32,           // Must be <= slot_size
    _pad: u32,

    free_head: AtomicU64,          // Index (low 32) + generation (high 32)
    // slot_meta: [SlotMeta; slot_count]
    // Then actual slot data
}

#[repr(C)]
struct SlotMeta {
    generation: AtomicU32,         // Incremented on each alloc
    state: AtomicU32,              // FREE / ALLOCATED / IN_FLIGHT
}

enum SlotState {
    Free = 0,
    Allocated = 1,  // Sender owns, writing payload
    InFlight = 2,   // Descriptor enqueued, awaiting receiver
}
```

**Slot state machine:**

```
              alloc()          enqueue()
    FREE ─────────────> ALLOCATED ──────────> IN_FLIGHT
      ▲                                           │
      └───────────────── free() ──────────────────┘
```

**Rule: Sender allocates, receiver frees.**

### 3.8 Crash Safety & Dead Peer Detection

Generation counters on payload slots detect stale references:

```rust
let meta = segment.slot_meta(desc.payload_slot);
if meta.generation.load(Ordering::Acquire) != desc.payload_generation {
    return Err(ValidationError::StaleGeneration);
}
```

Heartbeats for liveness:

```rust
async fn heartbeat_task(header: &SegmentHeader, is_peer_a: bool) {
    let epoch = if is_peer_a { &header.peer_a_epoch } else { &header.peer_b_epoch };
    let timestamp = if is_peer_a { &header.peer_a_last_seen } else { &header.peer_b_last_seen };

    loop {
        epoch.fetch_add(1, Ordering::Release);
        timestamp.store(now_nanos(), Ordering::Release);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
```

### 3.9 Flow Control (Credits)

Credit-based, per-channel. Sender waits for credits before sending.
Receiver grants credits after consuming data.

```rust
struct ChannelSender {
    channel_id: u32,
    available_credits: AtomicU32,
}

impl ChannelSender {
    async fn send(&self, data: &[u8]) -> Result<(), SendError> {
        let needed = data.len() as u32;
        // Wait for credits...
        self.available_credits.fetch_sub(needed, Ordering::AcqRel);
        self.enqueue(data).await
    }
}
```

## 4. Other Transports

### 4.1 rapace-transport-mem (In-Process)

For when host and plugin are in the same process (e.g., dynamically loaded `.so`).

```rust
struct InProcTransport {
    // Direct reference to the service implementation
    service: Arc<dyn Any + Send + Sync>,
}
```

**Characteristics:**
- No serialization — direct trait calls
- Real Rust lifetimes — `&[u8]` is a real borrow
- Still participates in RPC semantics (channels, deadlines, cancellation)
- Many parts are degenerate (no rings, no slots, no encoding)

### 4.2 rapace-transport-stream (TCP/Unix Socket)

For cross-machine or cross-container communication.

```rust
struct StreamTransport {
    read_half: OwnedReadHalf,
    write_half: OwnedWriteHalf,
    read_buf: BytesMut,
}
```

**Characteristics:**
- Frames are length-prefixed messages
- Everything is owned buffers — no zero-copy
- Same RPC semantics (channels, deadlines, flow control)
- Wire format: `[u32 length][frame bytes]`

### 4.3 rapace-transport-websocket (Framed)

For browser clients or WebSocket-based infrastructure.

```rust
struct WebSocketTransport {
    ws: WebSocketStream<TcpStream>,
}
```

**Characteristics:**
- Each WebSocket message is one frame
- Same wire format as stream transport inside the message
- Supports both text (JSON encoding) and binary (postcard encoding)

### 4.4 Transport Extensibility

To add a new transport:

1. Implement `Transport` trait
2. Provide a transport-specific `EncodeCtx` implementation
3. Handle transport-specific connection setup
4. The RPC layer handles everything else

---

# Part II — RPC & Facet Layer

This section defines the transport-agnostic RPC semantics and facet integration.

## 5. RPC Model & Channel Semantics

### 5.1 Sessions and Channels

A **session** is a connection between two peers over some transport.
A **channel** is a logical stream within a session for a single RPC call.

- Channel 0 is reserved for control messages (open/close/cancel/credits/ping/introspection)
- Channels 1+ are for RPC method calls
- Channel IDs are allocated by the initiator and must be unique within the session

### 5.2 RPC Patterns

**Unary RPC:**

```
Client                          Server
  │                                │
  │── OPEN_CHANNEL ───────────────>│
  │   (channel_id=N, method_id=M)  │
  │                                │
  │── DATA (request) + EOS ───────>│  (exactly one request)
  │                                │
  │<─────── DATA (response) + EOS ─│  (exactly one response)
  │       or ERROR + EOS           │
  │                                │
  └── channel closed ──────────────┘
```

**Client-streaming RPC:**

```
Client                          Server
  │                                │
  │── OPEN_CHANNEL ───────────────>│
  │                                │
  │── DATA ────────────────────────>│
  │── DATA ────────────────────────>│  (multiple requests)
  │── DATA + EOS ──────────────────>│
  │                                │
  │<─────── DATA (response) + EOS ─│  (one response)
  │                                │
  └── channel closed ──────────────┘
```

**Server-streaming RPC:**

```
Client                          Server
  │                                │
  │── OPEN_CHANNEL ───────────────>│
  │── DATA (request) + EOS ───────>│
  │                                │
  │<────────────────────── DATA ───│
  │<────────────────────── DATA ───│  (multiple responses)
  │<────────────────── DATA + EOS ─│
  │                                │
  └── channel closed ──────────────┘
```

**Bidirectional streaming:**

```
Client                          Server
  │                                │
  │── OPEN_CHANNEL ───────────────>│
  │                                │
  │── DATA ────────────────────────>│
  │<────────────────────── DATA ───│
  │── DATA ────────────────────────>│  (interleaved)
  │<────────────────────── DATA ───│
  │── EOS ─────────────────────────>│
  │<─────────────────────── EOS ───│
  │                                │
  └── channel closed ──────────────┘
```

### 5.3 Control Channel (Channel 0)

Control frames carry a `ControlPayload` in the body. The `method_id` indicates the verb.

```rust
enum ControlPayload {
    /// method_id = 1: Open a new data channel
    OpenChannel {
        channel_id: u32,
        service_name: String,
        method_name: String,
        metadata: Vec<(String, Vec<u8>)>,
    },

    /// method_id = 2: Close a channel gracefully
    CloseChannel {
        channel_id: u32,
        reason: CloseReason,
    },

    /// method_id = 3: Cancel a channel
    CancelChannel {
        channel_id: u32,
        reason: CancelReason,
    },

    /// method_id = 4: Grant flow control credits
    GrantCredits {
        channel_id: u32,
        bytes: u32,
    },

    /// method_id = 5: Liveness probe
    Ping { payload: [u8; 8] },

    /// method_id = 6: Response to Ping
    Pong { payload: [u8; 8] },
}
```

**Control channel flow control:** Control frames are exempt from per-channel credit flow
control. This ensures CANCEL, PING, and credit grants cannot be blocked by data backpressure.

### 5.4 Error Taxonomy

Codes 0-99 align with gRPC for familiarity. Codes 100+ are rapace-specific.

```rust
#[repr(u32)]
enum ErrorCode {
    // gRPC-aligned (0-99)
    Ok = 0,
    Cancelled = 1,
    DeadlineExceeded = 2,
    InvalidArgument = 3,
    NotFound = 4,
    AlreadyExists = 5,
    PermissionDenied = 6,
    ResourceExhausted = 7,
    FailedPrecondition = 8,
    Aborted = 9,
    OutOfRange = 10,
    Unimplemented = 11,
    Internal = 12,
    Unavailable = 13,
    DataLoss = 14,

    // rapace-specific (100+)
    PeerDied = 100,
    SessionClosed = 101,
    ValidationFailed = 102,
    StaleGeneration = 103,
}
```

### 5.5 Frame Flags

```rust
bitflags! {
    struct FrameFlags: u32 {
        const DATA          = 0b00000001;  // Regular data frame
        const CONTROL       = 0b00000010;  // Control frame
        const EOS           = 0b00000100;  // End of stream (half-close)
        const CANCEL        = 0b00001000;  // Cancel this channel
        const ERROR         = 0b00010000;  // Error response
        const HIGH_PRIORITY = 0b00100000;  // Priority scheduling hint
        const CREDITS       = 0b01000000;  // Contains credit grant
        const METADATA_ONLY = 0b10000000;  // Headers/trailers, no body
    }
}
```

## 6. Facet Integration & Encoding

### 6.1 Everything is Facet

All types in the RPC layer implement facet:

- Method arguments and return types
- Control payloads
- Service schemas in the registry
- Error details

This gives us:
- A dynamic view of any value via reflection
- Schema generation for the service registry
- Future: diff/patch for `&mut T` across transports

### 6.2 Encode/Decode Context

The codec is facet-driven but transport-specific:

```rust
/// Context for encoding values into frames.
///
/// Each transport provides its own implementation that knows how to
/// best represent data for that transport.
trait EncodeCtx {
    /// Encode a facet value into the frame.
    fn encode_value(&mut self, value: &dyn facet::Peek) -> Result<(), EncodeError>;

    /// Encode raw bytes.
    ///
    /// For SHM: checks if bytes are already in SHM and references them zero-copy.
    /// For stream: copies into the output buffer.
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError>;

    /// Finish encoding and return the frame.
    fn finish(self) -> Result<Frame, EncodeError>;
}

/// Context for decoding frames into values.
trait DecodeCtx<'a> {
    /// Decode a facet value from the frame.
    fn decode_value<T: facet::Facet>(&mut self) -> Result<T, DecodeError>;

    /// Borrow raw bytes from the frame.
    ///
    /// Lifetime is tied to the frame — caller must copy if needed longer.
    fn decode_bytes(&mut self) -> Result<&'a [u8], DecodeError>;
}
```

### 6.3 Transport-Specific Encoders

**In-proc encoder:**

```rust
struct InProcEncoder {
    // May bypass encoding entirely for direct calls
    // Or use a trivial pass-through for consistency
}

impl EncodeCtx for InProcEncoder {
    fn encode_value(&mut self, value: &dyn facet::Peek) -> Result<(), EncodeError> {
        // For in-proc, we might just store a reference
        // The "frame" is really just passing the value directly
        Ok(())
    }

    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        // Zero-copy: just reference the bytes
        Ok(())
    }
}
```

**SHM encoder:**

```rust
struct ShmEncoder<'a> {
    session: &'a ShmSession,
    builder: ShmFrameBuilder,
}

impl EncodeCtx for ShmEncoder<'_> {
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        // Check if bytes are already in a known SHM mapping
        if let Some((slot, offset)) = self.session.find_shm_location(bytes) {
            // Zero-copy: encode as slot reference
            self.builder.encode_slot_ref(slot, offset, bytes.len());
        } else {
            // Not in SHM: allocate slot and copy
            let slot = self.session.alloc_slot(bytes.len())?;
            self.session.copy_to_slot(slot, bytes);
            self.builder.encode_slot_ref(slot, 0, bytes.len());
        }
        Ok(())
    }
}
```

**Stream encoder:**

```rust
struct StreamEncoder {
    buf: Vec<u8>,
}

impl EncodeCtx for StreamEncoder {
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        // Always copy into buffer
        postcard::to_io(bytes, &mut self.buf)?;
        Ok(())
    }
}
```

### 6.4 Message Layout

The payload (inline or in slot) contains:

```
payload = [ MsgHeader ][ body bytes ]

┌────────────────────────────────────────────────────────────┐
│ MsgHeader (fixed 24 bytes)                                  │
├────────────────────────────────────────────────────────────┤
│ Metadata (variable, header_len - 24 bytes)                 │  ← optional
├────────────────────────────────────────────────────────────┤
│ Body (payload_len - header_len bytes)                      │
└────────────────────────────────────────────────────────────┘
```

```rust
#[repr(C)]
struct MsgHeader {
    version: u16,          // Message format version
    header_len: u16,       // Total header size
    encoding: u16,         // Body encoding (Postcard, Json, Raw)
    flags: u16,            // Compression, etc.
    correlation_id: u64,   // Reply-to: msg_id of request
    deadline_ns: u64,      // Absolute deadline (0 = none)
}

#[repr(u16)]
enum Encoding {
    Postcard = 1,  // Default for all messages
    Json = 2,      // For debugging, external tooling
    Raw = 3,       // App-defined, no schema
}
```

## 7. Service Traits, Codegen & Stubs

### 7.1 The `#[rapace::service]` Macro

Users write pure Rust traits with no transport awareness:

```rust
#[rapace::service]
trait Calculator {
    fn add(&self, a: i32, b: i32) -> i32;
    fn divide(&self, a: i32, b: i32) -> Result<i32, DivideByZero>;

    // Streaming
    fn sum_stream(&self, numbers: impl Stream<Item = i32>) -> i32;
    fn fibonacci(&self, count: u32) -> impl Stream<Item = u64>;
}
```

The macro generates:

1. **Facet derivations** for all argument/return types (or asserts they implement facet)

2. **Client stub** with identical signatures:
   ```rust
   struct CalculatorClient<T: Transport> {
       transport: T,
       // ...
   }

   impl<T: Transport> CalculatorClient<T> {
       pub fn add(&self, a: i32, b: i32) -> impl Future<Output = Result<i32, RpcError>> {
           // Encode (a, b), send frame, await response, decode result
       }
   }
   ```

3. **Server skeleton** that receives frames and calls the impl:
   ```rust
   struct CalculatorServer<S: Calculator> {
       service: S,
   }

   impl<S: Calculator> CalculatorServer<S> {
       fn dispatch(&self, method_id: u32, args: &[u8]) -> Result<Vec<u8>, RpcError> {
           match method_id {
               1 => {
                   let (a, b) = decode(args)?;
                   let result = self.service.add(a, b);
                   encode(result)
               }
               // ...
           }
       }
   }
   ```

### 7.2 Transport Selection

The same client works over any transport:

```rust
// Over SHM
let client = CalculatorClient::new(shm_transport);

// Over TCP
let client = CalculatorClient::new(stream_transport);

// In-process (direct calls, no serialization)
let client = CalculatorClient::new_inproc(Box::new(my_calculator));
```

### 7.3 Call Flow

When `client.add(1, 2)` is called:

1. **Pack arguments** into a facet value: `(1i32, 2i32)`
2. **Get encoder** from transport: `transport.encoder()`
3. **Encode** via facet reflection: `encoder.encode_value(&args)`
4. **Send frame**: `transport.send_frame(frame)`
5. **Await response**: `transport.recv_frame()`
6. **Decode** result: `decoder.decode_value::<i32>()`
7. **Return** to caller

For in-proc transport, steps 2-6 collapse to a direct function call.

---

# Part III — Cross-Cutting Concerns

## 8. Lifetimes, Ownership & Zero-Copy Policy

This section codifies how lifetimes work across transports.

### 8.1 Public API Lifetimes

Traits and stubs always expose normal Rust signatures:

```rust
trait Hasher {
    fn hash(&self, data: &[u8]) -> [u8; 32];      // borrows input
}

trait Differ {
    fn diff(&self, old: &str, new: &str) -> Diff; // borrows both
}

trait Processor {
    fn process(&mut self, item: &mut Item);       // mutable borrow
}
```

**No transport-specific types leak into these signatures.**

### 8.2 Lifetime Regimes by Transport

#### In-proc

```rust
impl InProcTransport {
    fn call<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&dyn Service) -> R
    {
        // Direct call — real Rust lifetimes
        f(&*self.service)
    }
}
```

- Borrows are real — `&[u8]` is a real reference
- `&mut T` works with normal lifetime semantics
- May bypass RPC encoding entirely

#### SHM

**Outbound side:**

```rust
fn send_request(&self, data: &[u8]) {
    // Borrow lives only for duration of this call
    let encoder = self.transport.encoder();

    if self.is_in_shm(data) {
        // Zero-copy: encode as slot reference
        encoder.encode_slot_ref(data);
    } else {
        // Not in SHM: alloc slot, copy data
        encoder.encode_with_copy(data);
    }
}
```

**Inbound side:**

```rust
fn handle_frame<'frame>(&self, frame: ShmFrameView<'frame>) {
    // View lifetime is tied to frame processing
    let data: &'frame [u8] = frame.payload();

    // Process within this scope
    self.service.process(data);

    // After this function returns, frame is released
}
```

**Key constraint:** You never hand out references tied to the whole session lifetime.
Frame-local views cannot escape the handler.

#### Stream/WebSocket

- Everything is owned — no borrows survive encoding
- Lifetimes are confined to decode buffers within `recv_frame`
- No zero-copy (data crosses process/machine boundary)

### 8.3 `&T` / `&mut T` Semantics Across Transports

| Type | In-proc | SHM | Network |
|------|---------|-----|---------|
| `&[u8]` | Real borrow | Zero-copy if in SHM, else snapshot+copy | Snapshot+serialize |
| `&str` | Real borrow | Zero-copy if in SHM, else snapshot+copy | Snapshot+serialize |
| `&T` | Real borrow | Snapshot into SHM slot | Snapshot+serialize |
| `&mut T` | Real mutable borrow | Snapshot+send; (future: diff on return) | Snapshot+serialize |

**Future: `&mut T` diff/patch**

For `&mut T` across non-in-proc transports:
1. Snapshot `T` and send
2. Server receives owned copy, applies mutations
3. Server sends back either full value or facet-driven diff
4. Client applies patch to original

This is marked as future work — for now, `&mut T` is snapshot-only.

### 8.4 Soundness Constraints

1. **Never manufacture `'static` from SHM mappings** — SHM can be unmapped

2. **Frame-local views cannot escape the handler** — enforce via lifetime bounds:
   ```rust
   fn handle<'a>(&'a self, frame: FrameView<'a>) {
       // frame.payload() returns &'a [u8]
       // Cannot store this reference beyond this call
   }
   ```

3. **Generation counters must match** — stale slot references are rejected

4. **SHM zero-copy only when frame is active** — check state before dereferencing

## 9. Observability & Telemetry

### 9.1 Logical vs Physical Observability

Every message *logically* has:
- `trace_id`: 64-bit, propagated from caller
- `span_id`: 64-bit, unique per message
- `timestamp_ns`: When enqueued

*Physically*, these may be:
- Stored in `MsgDescCold` (when `debug_level >= 1`)
- Defaulted to 0 (when disabled)
- Stored only in process-local memory

### 9.2 Telemetry Ring

Optional separate ring for telemetry events, readable by observer processes:

```rust
#[repr(C)]
struct TelemetryEvent {
    timestamp_ns: u64,
    trace_id: u64,
    span_id: u64,
    channel_id: u32,
    event_type: u32,      // SEND, RECV, CANCEL, TIMEOUT, ERROR
    payload_len: u32,
    latency_ns: u32,
}
```

**Backpressure:** Telemetry overflow drops events, never blocks data path.

### 9.3 Cross-Transport Tracing

`trace_id` and `span_id` are preserved across transport boundaries:

```
[Browser] ──WebSocket──> [Gateway] ──SHM──> [Plugin]
    │                        │                  │
 trace_id=X              trace_id=X         trace_id=X
 span_id=1               span_id=2          span_id=3
```

## 10. Safety & Validation

### 10.1 Descriptor Validation

All descriptors MUST be validated before use:

```rust
fn validate_descriptor(desc: &MsgDescHot, limits: &DescriptorLimits) -> Result<(), ValidationError> {
    // Bounds check payload slot
    if desc.payload_slot != u32::MAX {
        if desc.payload_slot >= limits.slot_count {
            return Err(ValidationError::SlotOutOfBounds);
        }

        let end = desc.payload_offset.saturating_add(desc.payload_len);
        if end > limits.slot_size {
            return Err(ValidationError::PayloadOutOfBounds);
        }

        // Generation check
        // ...
    } else {
        if desc.payload_len > INLINE_PAYLOAD_SIZE as u32 {
            return Err(ValidationError::InlinePayloadTooLarge);
        }
    }

    if desc.payload_len > limits.max_payload_len {
        return Err(ValidationError::PayloadTooLarge);
    }

    Ok(())
}
```

### 10.2 Recommended Limits

```rust
struct DescriptorLimits {
    max_payload_len: u32,                    // Default: 1MB
    max_channels: u32,                       // Default: 1024
    max_frames_in_flight_per_channel: u32,   // Default: 64
    max_frames_in_flight_total: u32,         // Default: 4096
}
```

### 10.3 Error Handling

On validation failure:
1. Increment error counter
2. Drop the descriptor
3. Optionally send ERROR frame
4. Never panic — continue processing

## 11. Plugin System Integration

### 11.1 The Vision

Plugins implement traits like:

```rust
#[rapace::service]
trait HtmlDiffer {
    fn diff(&self, old: &str, new: &str) -> HtmlDiff;
}

#[rapace::service]
trait ImageProcessor {
    fn resize(&self, image: &[u8], width: u32, height: u32) -> Vec<u8>;
}
```

Host and plugins can be:
- **In the same process** — dynamically loaded `.so`, direct calls
- **In sibling processes** — connected via SHM, zero-copy for large data
- **Remote** — connected via WebSocket, full serialization

### 11.2 Single Trait, Multiple Transports

```rust
// Plugin implementation
struct MyDiffer;

impl HtmlDiffer for MyDiffer {
    fn diff(&self, old: &str, new: &str) -> HtmlDiff {
        // Implementation
    }
}

// Host can use it three ways:

// 1. In-proc (same process)
let differ = HtmlDifferClient::new_inproc(Box::new(MyDiffer));

// 2. SHM (sibling process)
let session = ShmSession::connect("/tmp/differ.sock")?;
let differ = HtmlDifferClient::new(session);

// 3. Remote (different machine)
let conn = TcpStream::connect("192.168.1.100:8080")?;
let differ = HtmlDifferClient::new(StreamTransport::new(conn));

// All three have identical APIs
let diff = differ.diff(old_html, new_html).await?;
```

### 11.3 Service Registry & Introspection

The service registry in SHM (or exchanged during handshake for other transports) contains:
- Service names and versions
- Method signatures
- Facet schemas for all types

This enables:
- Runtime service discovery
- Schema validation
- Tooling (debuggers, inspectors)

---

# Appendix A: Version Negotiation

Protocol version is `u32`: major (high 16 bits), minor (low 16 bits).

| Condition | Action |
|-----------|--------|
| Major versions match | Proceed, use minimum of minor versions |
| Major mismatch | Abort with `IncompatibleVersion` |
| Minor mismatch | Proceed with feature flags |

Feature flags in `SegmentHeader.flags`:
- Bit 0: Telemetry ring support
- Bit 1: Fault injection support
- Bit 2: Extended introspection

# Appendix B: Session Handshake (SHM)

```
1. Peer A creates SHM segment + eventfds
2. Peer A listens on Unix socket
3. Peer B connects
4. Peer A sends via SCM_RIGHTS:
   - SHM fd
   - Doorbell eventfds (A→B and B→A)
   - Session metadata
5. Peer B maps SHM, sets up rings
6. Peer B sends ACK with capabilities
7. Normal operation begins
```

# Appendix C: Fault Injection

Built-in hooks for testing:

```rust
struct FaultInjector {
    drop_rate: AtomicU32,      // 0-10000 (0.00% - 100.00%)
    delay_ms: AtomicU32,
    error_rate: AtomicU32,
    channel_faults: DashMap<u32, ChannelFaults>,
}
```

**Timing:** Fault injection happens AFTER descriptor validation.

# Appendix D: Mental Model — RPC as a Spine

The RPC layer is not a single layer in the stack; it's a spine through multiple layers:

```
┌─────────────────────────────────────────────────────────┐
│ Service Traits (top)                                     │
│   What users write: pure Rust traits                     │
├─────────────────────────────────────────────────────────┤
│ Facet Reflection                                         │
│   - Dynamic view of method arguments/results             │
│   - Schema for service registry                          │
│   - (Future) diff/patch for &mut T                       │
├─────────────────────────────────────────────────────────┤
│ RPC Framing                                              │
│   - method_id, channel_id, MsgHeader                     │
│   - Deadlines, error codes, flow control                 │
├─────────────────────────────────────────────────────────┤
│ Transport-Specific Encoding                              │
│   - In-proc: pass-through                               │
│   - SHM: slot references when possible                   │
│   - Stream: postcard serialization                       │
├─────────────────────────────────────────────────────────┤
│ Transport Implementation (bottom)                        │
│   - SHM: rings, slots, doorbells                        │
│   - Stream: length-prefixed TCP                          │
│   - WebSocket: framed messages                           │
└─────────────────────────────────────────────────────────┘
```

A trait method call flows through ALL of these:

```
Trait → Facet → Encoder → Transport → [wire] → Transport → Decoder → Facet → Trait
```

This vertical integration means:
- The same trait works on any transport
- Each transport optimizes for its medium
- No transport-specific types leak into user code
