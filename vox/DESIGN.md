# rapace2 Design Document

A production-grade shared-memory RPC system with NIC-style descriptor rings,
bidirectional streaming, full observability, and proper async integration.

## Why rapace2?

| vs. | rapace2 advantage |
|-----|-------------------|
| Unix domain sockets | Zero-copy for large payloads, no kernel transitions on hot path |
| gRPC over loopback | ~10-100x lower latency, no HTTP/2 overhead, shared memory for bulk data |
| boost::interprocess | Async-native, built-in RPC semantics, observability, flow control |
| Custom SHM queues | Production-ready: cancellation, deadlines, crash recovery, introspection |

## Scope & Model

**Each SHM segment represents a session between exactly two peers (A and B).**

The design is intentionally SPSC (single-producer, single-consumer) per ring direction.
This keeps the ring implementation simple and correct. For N-way topologies:
- Host maintains N separate sessions (one per plugin)
- Or use a broker pattern with a future `rapace-bus` design

Non-goals for v1:
- MPMC rings (complexity not worth it)
- Cross-machine transport (use sockets for that)
- Encryption (peers are on same machine, use OS isolation)

## Version Negotiation

Protocol version is a `u32` split into major (high 16 bits) and minor (low 16 bits).

```rust
const PROTOCOL_VERSION: u32 = 0x0001_0000;  // v1.0

fn parse_version(v: u32) -> (u16, u16) {
    ((v >> 16) as u16, v as u16)
}
```

### Negotiation Rules

During handshake, peers exchange their supported version:

| Condition | Action |
|-----------|--------|
| Major versions match | Proceed, use minimum of minor versions |
| Major mismatch | **Abort session** with `IncompatibleVersion` error |
| Minor mismatch | Proceed with feature flags for optional capabilities |

**Feature flags** (in `SegmentHeader.flags`) indicate optional capabilities:
- Bit 0: Telemetry ring support
- Bit 1: Fault injection support
- Bit 2: Extended introspection
- Bits 3-31: Reserved for future use

```rust
bitflags! {
    struct FeatureFlags: u32 {
        const TELEMETRY       = 0b0001;
        const FAULT_INJECTION = 0b0010;
        const EXTENDED_INTRO  = 0b0100;
    }
}
```

Peers MUST only use features that both sides advertise support for.

## Error Taxonomy

Standard error codes for interoperability across implementations.

**Note:** Codes 0-99 are aligned with gRPC status codes for familiarity.
Codes 100+ are rapace-specific extensions.

```rust
#[repr(u32)]
enum ErrorCode {
    // ===== gRPC-aligned codes (0-99) =====

    // Success (not an error)
    Ok = 0,

    // Cancellation & timeouts
    Cancelled = 1,           // Operation was cancelled
    DeadlineExceeded = 2,    // Deadline passed before completion

    // Request errors
    InvalidArgument = 3,     // Malformed request
    NotFound = 4,            // Service/method not found
    AlreadyExists = 5,       // Resource already exists
    PermissionDenied = 6,    // Caller lacks permission

    // Resource errors
    ResourceExhausted = 7,   // Out of credits, slots, channels, etc.
    FailedPrecondition = 8,  // System not in required state

    // Protocol errors
    Aborted = 9,             // Operation aborted (conflict, etc.)
    OutOfRange = 10,         // Value out of valid range
    Unimplemented = 11,      // Method not implemented

    // System errors
    Internal = 12,           // Internal error (bug)
    Unavailable = 13,        // Service temporarily unavailable
    DataLoss = 14,           // Unrecoverable data loss

    // ===== rapace-specific codes (100+) =====

    PeerDied = 100,          // Peer process crashed
    SessionClosed = 101,     // Session shut down
    ValidationFailed = 102,  // Descriptor validation failed
    StaleGeneration = 103,   // Generation counter mismatch
}
```

## Goals

1. **Zero or one copy** on the hot path
2. **Async-friendly** with eventfd doorbells (no polling!)
3. **Bidirectional streaming** with channels as the primitive
4. **Credit-based backpressure** per-channel
5. **Full observability** from day one (trace_id, span_id, timestamps)
6. **Service discovery** with introspection (list services, methods, schemas)
7. **Crash-safe** with generation counters and dead peer detection
8. **Fault injection** for testing

## Architecture Overview

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

## Memory Layout Details

### Segment Header (64 bytes, cache-line aligned)

```rust
#[repr(C, align(64))]
struct SegmentHeader {
    magic: u64,                    // "RAPACE2\0"
    version: u32,                  // Protocol version
    flags: u32,                    // Feature flags

    // Peer liveness
    peer_a_epoch: AtomicU64,       // Incremented by peer A periodically
    peer_b_epoch: AtomicU64,       // Incremented by peer B periodically
    peer_a_last_seen: AtomicU64,   // Timestamp (nanos since epoch)
    peer_b_last_seen: AtomicU64,   // Timestamp

    // Doorbell eventfds (exchanged out-of-band)
    // Not stored in SHM - passed via Unix socket
}
```

### Message Descriptor - Hot Path (64 bytes, one cache line)

The hot descriptor contains everything needed for the data path.
Observability data is stored separately to avoid cache pollution.

```rust
#[repr(C, align(64))]
struct MsgDescHot {
    // Identity (16 bytes)
    msg_id: u64,                   // Unique per session, monotonic
    channel_id: u32,               // Logical stream (0 = control channel)
    method_id: u32,                // For RPC dispatch, or control verb

    // Payload location (16 bytes)
    payload_slot: u32,             // Slot index in data segment (u32::MAX = inline)
    payload_generation: u32,       // Generation counter for ABA safety
    payload_offset: u32,           // Offset within slot
    payload_len: u32,              // Actual payload length

    // Flow control & flags (8 bytes)
    flags: u32,                    // EOS, CANCEL, ERROR, etc.
    credit_grant: u32,             // Credits being granted to peer

    // Inline payload for small messages (24 bytes)
    // Used when payload_slot == u32::MAX and payload_len <= 24
    inline_payload: [u8; 24],
}

/// Inline payload size derived from cache-line math
const INLINE_PAYLOAD_SIZE: usize = 24;  // 64 - 40 bytes of header fields
```

**Inline payload alignment:** The `inline_payload` field has no alignment guarantees beyond
`u8` (byte-aligned). Serializers reading from inline payloads **MUST NOT** assume any
particular alignment. Use `ptr::read_unaligned` or copy to an aligned buffer before
casting to structured types.

### Message Descriptor - Cold Path (Observability)

Stored in a parallel array or separate telemetry ring.
Can be disabled per-channel or globally for maximum performance.

```rust
#[repr(C, align(64))]
struct MsgDescCold {
    msg_id: u64,                   // Correlates with hot descriptor
    trace_id: u64,                 // Distributed tracing
    span_id: u64,                  // Span within trace
    parent_span_id: u64,           // Parent span for hierarchy
    timestamp_ns: u64,             // When enqueued
    debug_level: u32,              // 0=off, 1=metadata, 2=full payload mirror
    _reserved: u32,
}
```

### Debug Levels (per-channel configurable)

- **Level 0**: Metrics only (counters), no cold descriptors
- **Level 1**: Metadata only (cold desc without payload)
- **Level 2**: Full payload mirroring to telemetry ring
- **Level 3**: Plus fault injection rules active

### Descriptor Ring

```rust
#[repr(C)]
struct DescRing {
    // Producer publication index (cache-line aligned)
    // This is the ONLY producer index visible to consumer
    visible_head: AtomicU64,
    _pad1: [u8; 56],

    // Consumer side (separate cache line)
    tail: AtomicU64,               // Next slot to read
    _pad2: [u8; 56],

    // Ring configuration
    capacity: u32,                 // Power of 2
    _pad3: [u8; 60],

    // Descriptors follow (capacity * sizeof(MsgDescHot))
}
```

### Ring Algorithm (SPSC with explicit memory orderings)

The ring uses the high bits of head/tail as implicit generation/lap counters.
Slot index is computed as `idx = position & (capacity - 1)`.

**Key design decision:** We use a single `visible_head` instead of separate `head` + `visible_head`.
The producer tracks its private head position in a local variable, reducing cache traffic
and eliminating confusion about which index is authoritative.

```rust
impl DescRing {
    /// Producer: enqueue a descriptor
    /// `local_head` is producer-private state (not in shared memory)
    /// Returns Err if ring is full
    fn enqueue(&self, local_head: &mut u64, desc: &MsgDescHot) -> Result<(), RingFull> {
        let tail = self.tail.load(Ordering::Acquire);  // Sync with consumer

        // Check if full: (head - tail) >= capacity
        if local_head.wrapping_sub(tail) >= self.capacity as u64 {
            return Err(RingFull);
        }

        let idx = (*local_head & (self.capacity as u64 - 1)) as usize;

        // Write descriptor (non-atomic, we own this slot)
        // The Release store on visible_head provides the happens-before guarantee,
        // so we use normal ptr::write here (not volatile)
        unsafe {
            let slot = self.desc_slot(idx);
            std::ptr::write(slot, *desc);
        }

        *local_head += 1;

        // Publish: make descriptor visible to consumer
        // Release ordering ensures desc write completes before consumer sees new head
        self.visible_head.store(*local_head, Ordering::Release);

        Ok(())
    }

    /// Consumer: dequeue a descriptor
    /// Returns None if ring is empty
    fn dequeue(&self) -> Option<MsgDescHot> {
        let tail = self.tail.load(Ordering::Relaxed);
        let visible = self.visible_head.load(Ordering::Acquire);  // Sync with producer

        // Check if empty
        if tail >= visible {
            return None;
        }

        let idx = (tail & (self.capacity as u64 - 1)) as usize;

        // Read descriptor
        // The Acquire load on visible_head provides the happens-before guarantee,
        // so we use normal ptr::read here (not volatile)
        let desc = unsafe {
            let slot = self.desc_slot(idx);
            std::ptr::read(slot)
        };

        // Advance tail
        // Release ordering ensures desc read completes before producer sees freed slot
        self.tail.store(tail + 1, Ordering::Release);

        Some(desc)
    }

    /// Batch dequeue: drain up to N descriptors
    fn drain(&self, max: usize) -> impl Iterator<Item = MsgDescHot> {
        std::iter::from_fn(move || self.dequeue()).take(max)
    }
}
```

**Memory ordering rationale:**
- `visible_head.store(Release)` → `visible_head.load(Acquire)` forms a synchronizes-with relationship
- This ensures the descriptor write happens-before the consumer's read
- `tail.store(Release)` → `tail.load(Acquire)` ensures the consumer's read happens-before producer reuses slot
- No volatile needed: atomics with proper ordering provide all necessary guarantees

**Invariants:**
- Producer's `local_head` is stack-local, never shared
- `visible_head` is the publication barrier (producer writes, consumer reads)
- `tail` is only written by consumer, read by producer for fullness check
- Ring is empty when `tail == visible_head`
- Ring is full when `local_head - tail >= capacity`
- No per-slot generation needed for SPSC (monotonic indices are sufficient)

### Data Segment (Slab Allocator)

```rust
#[repr(C)]
struct DataSegment {
    // Allocator metadata
    slot_size: u32,                // e.g., 4KB per slot
    slot_count: u32,
    max_frame_size: u32,           // Must be <= slot_size
    _pad: u32,

    // Free list (lock-free stack)
    free_head: AtomicU64,          // Index (low 32) + generation (high 32)

    // Slot metadata array
    // slot_meta: [SlotMeta; slot_count]
    // Then actual slot data
}
```

**Size invariants:**
- `max_frame_size <= slot_size` (enforced at segment creation)
- `DescriptorLimits.max_payload_len <= max_frame_size` (enforced at validation)

If `max_payload_len` exceeds `max_frame_size`, validation MUST reject the descriptor.

```rust

#[repr(C)]
struct SlotMeta {
    generation: AtomicU32,         // Incremented on each alloc
    state: AtomicU32,              // See SlotState enum
}

/// Slot states for the data segment allocator
enum SlotState {
    Free = 0,       // Available for allocation
    Allocated = 1,  // Sender owns, writing payload
    InFlight = 2,   // Descriptor enqueued, awaiting receiver
}
```

### Slot State Machine

```
                     ┌─────────────────────────────────────────┐
                     │                                         │
                     ▼                                         │
┌──────────┐    alloc()    ┌───────────┐   enqueue()   ┌───────────┐
│   FREE   │──────────────>│ ALLOCATED │──────────────>│ IN_FLIGHT │
│          │               │           │               │           │
│ (owner:  │               │ (owner:   │               │ (owner:   │
│  none)   │               │  sender)  │               │  receiver)│
└──────────┘               └───────────┘               └───────────┘
     ▲                                                       │
     │                                                       │
     │                      free()                           │
     └───────────────────────────────────────────────────────┘
```

| Transition | Actor | Operation | Notes |
|------------|-------|-----------|-------|
| FREE → ALLOCATED | Sender | `alloc()` | Generation incremented |
| ALLOCATED → IN_FLIGHT | Sender | `enqueue()` | Descriptor published to ring |
| IN_FLIGHT → FREE | Receiver | `free()` | After processing payload |

**Crash recovery:** If sender crashes in ALLOCATED state, slot is leaked until
session cleanup. If sender crashes after IN_FLIGHT, receiver still frees normally.

### Payload Allocation Protocol

**Rule: Sender allocates, receiver frees.**

```rust
// Sender side
fn send_with_payload(&self, payload: &[u8]) -> Result<(), SendError> {
    let slot = if payload.len() <= INLINE_PAYLOAD_SIZE {
        // Small message: inline in descriptor
        u32::MAX
    } else {
        // Large message: allocate from data segment
        self.data_segment.alloc()?
    };

    // ... write payload, enqueue descriptor ...
}

// Receiver side
fn on_frame_received(&self, desc: &MsgDescHot) {
    // Process payload...

    // Free the slot (if not inline)
    if desc.payload_slot != u32::MAX {
        self.data_segment.free(desc.payload_slot, desc.payload_generation);
    }
}
```

**Allocation is per-direction:** Each peer allocates from its own "outbound" pool.
This avoids contention and simplifies ownership tracking.

## Message Header (in payload)

The payload (inline or in a slot) begins with a `MsgHeader`, followed by the body:

```
payload = [ MsgHeader ][ body bytes ]
```

```rust
#[repr(C)]
struct MsgHeader {
    version: u16,                  // Message format version (see below)
    header_len: u16,               // Bytes including this header
    encoding: u16,                 // Body encoding (see Encoding enum)
    flags: u16,                    // Compression, etc.

    correlation_id: u64,           // Reply-to: msg_id of request
    deadline_ns: u64,              // Absolute deadline (0 = none)

    // Variable-length fields follow:
    // - metadata (key-value pairs for headers)
    // - body
}
```

### Version Fields

There are two version fields with different purposes:

| Field | Type | Scope | Purpose |
|-------|------|-------|---------|
| `SegmentHeader.version` | u32 | Session | Protocol version (major.minor), negotiated at handshake |
| `MsgHeader.version` | u16 | Message | Message format version within the protocol |

**For v1:** `MsgHeader.version` MUST equal the negotiated minor version from handshake.
This ties message format to protocol version for simplicity.

**Future extension:** If message format needs to evolve independently of protocol
version, `MsgHeader.version` can be decoupled. For now, they're synchronized.

```rust
// During message creation
fn create_msg_header(session: &Session) -> MsgHeader {
    MsgHeader {
        version: session.negotiated_minor_version(),  // From handshake
        header_len: size_of::<MsgHeader>() as u16,
        encoding: Encoding::Postcard as u16,
        ..Default::default()
    }
}
```

/// Body encoding format
#[repr(u16)]
enum Encoding {
    /// postcard via facet-postcard - default for all control messages
    Postcard = 1,
    /// JSON - for debugging, external tooling
    Json = 2,
    /// Raw bytes - app-defined, no schema
    Raw = 3,
}
```

### Encoding Rules

1. **Control messages (channel 0)**: MUST use `Encoding::Postcard`. The body is
   `postcard::to_vec(&control_payload)` where `control_payload: ControlPayload`.

2. **RPC messages (channel > 0)**: MAY use any encoding. The encoding is typically
   negotiated per-method via the service registry or fixed by convention.

3. **facet-postcard**: For Rust services, we use [facet](https://crates.io/crates/facet)
   with postcard as the wire format. This gives us:
   - Compact binary encoding
   - Schema derivation from Rust types
   - Forward/backward compatibility via postcard's encoding rules

### Payload Layering

```
┌─────────────────────────────────────────────────────────┐
│ MsgDescHot (64 bytes, in ring)                          │
│   - msg_id, channel_id, method_id                       │
│   - payload_slot / inline_payload                       │
│   - flags, credits                                      │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼ (payload_slot or inline)
┌─────────────────────────────────────────────────────────┐
│ MsgHeader (fixed size)                                   │
│   - version, encoding, flags                            │
│   - correlation_id, deadline_ns                         │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼ (after header_len bytes)
┌─────────────────────────────────────────────────────────┐
│ Body (encoding-dependent)                               │
│   - Postcard: postcard(ControlPayload) or postcard(T)  │
│   - Json: JSON string                                   │
│   - Raw: opaque bytes                                   │
└─────────────────────────────────────────────────────────┘
```

### Schema Storage

The service registry's `schema` field contains:
- For Rust/facet services: a facet schema descriptor serialized via postcard
- For other languages: implementation-defined (MAY be JSON Schema, protobuf descriptor, etc.)

Implementations SHOULD document their schema format. Cross-language interop requires
agreeing on schema representation.

## Channel Model

Everything is a **channel**. RPC is sugar on top.

### Channel Lifecycle

```
OPEN_CHANNEL(channel_id, method_id, metadata)
    │
    ▼
┌───────────────────────────────────────┐
│            Channel Open               │
│  - Both sides can send DATA frames    │
│  - Either side can send CREDITS       │
│  - Either side can half-close (EOS)   │
│  - Either side can CANCEL             │
└───────────────────────────────────────┘
    │
    ▼ (both sides EOS or CANCEL)
CHANNEL_CLOSED
```

### Frame Types (via flags)

```rust
bitflags! {
    struct FrameFlags: u32 {
        const DATA          = 0b00000001;  // Regular data frame
        const CONTROL       = 0b00000010;  // Control frame (open/close/etc)
        const EOS           = 0b00000100;  // End of stream (half-close)
        const CANCEL        = 0b00001000;  // Cancel this channel
        const ERROR         = 0b00010000;  // Error response
        const HIGH_PRIORITY = 0b00100000;  // Skip normal queue
        const CREDITS       = 0b01000000;  // Contains credit grant
        const METADATA_ONLY = 0b10000000;  // Headers/trailers, no body
    }
}
```

### Control Frame Payloads

Control frames on channel 0 carry a `ControlPayload` in the body. The `method_id` field
in the descriptor determines which variant to expect.

```rust
/// Control payloads - serialized via postcard in the frame body
enum ControlPayload {
    /// method_id = 1: Open a new data channel
    OpenChannel {
        channel_id: u32,
        service_name: String,
        method_name: String,
        metadata: Vec<(String, Vec<u8>)>,
    },

    /// method_id = 2: Close a channel gracefully (both sides should EOS)
    CloseChannel {
        channel_id: u32,
        reason: CloseReason,
    },

    /// method_id = 3: Cancel a channel (advisory, may race with in-flight frames)
    CancelChannel {
        channel_id: u32,
        reason: CancelReason,
    },

    /// method_id = 4: Grant flow control credits to peer
    GrantCredits {
        channel_id: u32,
        bytes: u32,
    },

    /// method_id = 5: Liveness probe
    Ping {
        payload: [u8; 8],
    },

    /// method_id = 6: Response to Ping
    Pong {
        payload: [u8; 8],
    },
}

#[derive(Debug, Clone, Copy)]
enum CloseReason {
    Normal = 0,
    GoingAway = 1,
    ProtocolError = 2,
}

#[derive(Debug, Clone, Copy)]
enum CancelReason {
    UserRequested = 0,
    Timeout = 1,
    DeadlineExceeded = 2,
    ResourceExhausted = 3,
    PeerDied = 4,
}
```

**CANCEL semantics:** The `FrameFlags::CANCEL` flag on data frames is a shorthand for
"this frame cancels the channel". For control channel, use `ControlPayload::CancelChannel`.
Both are equivalent; the flag is an optimization to avoid parsing the payload when
only the cancellation signal matters.

## Service Registry

Built into the control segment, queryable in-band.

```rust
#[repr(C)]
struct ServiceRegistry {
    service_count: u32,
    _pad: u32,
    // Followed by ServiceEntry array
}

#[repr(C)]
struct ServiceEntry {
    name_offset: u32,              // Offset to null-terminated name
    name_len: u32,
    method_count: u32,
    methods_offset: u32,           // Offset to MethodEntry array
    schema_offset: u32,            // Offset to schema blob (optional)
    schema_len: u32,
    version_major: u16,
    version_minor: u16,
}

#[repr(C)]
struct MethodEntry {
    name_offset: u32,
    name_len: u32,
    method_id: u32,                // Used in MsgDesc.method_id
    flags: u32,                    // Unary, client-stream, server-stream, bidi
    request_schema_offset: u32,
    response_schema_offset: u32,
}
```

### Reserved Channel 0: Introspection

```rust
// Built-in methods on channel 0, method_id 0
enum IntrospectionRequest {
    ListServices,
    GetService { name: String },
    GetMethod { service: String, method: String },
    GetSchema { service: String, method: String },
}

enum IntrospectionResponse {
    ServiceList { services: Vec<ServiceInfo> },
    Service { info: ServiceInfo },
    Method { info: MethodInfo },
    Schema { schema: Vec<u8> },  // e.g., JSON schema or custom format
}
```

## Flow Control

Credit-based, per-channel.

### Sender Side

```rust
struct ChannelSender {
    channel_id: u32,
    available_credits: AtomicU32,  // Bytes we're allowed to send
    pending_frames: VecDeque<Frame>,
}

impl ChannelSender {
    async fn send(&self, data: &[u8]) -> Result<(), SendError> {
        let needed = data.len() as u32;

        // Wait for credits
        loop {
            let credits = self.available_credits.load(Ordering::Acquire);
            if credits >= needed {
                if self.available_credits
                    .compare_exchange(credits, credits - needed, ...)
                    .is_ok()
                {
                    break;
                }
            } else {
                // Park until we receive a CREDITS frame
                self.credits_available.notified().await;
            }
        }

        // Now enqueue the frame
        self.enqueue(data).await
    }
}
```

### Receiver Side

```rust
struct ChannelReceiver {
    channel_id: u32,
    consumed_bytes: AtomicU32,     // Bytes consumed since last credit grant
    credit_threshold: u32,         // Grant credits when consumed > threshold
}

impl ChannelReceiver {
    fn on_frame_consumed(&self, len: u32, conn: &Connection) {
        let consumed = self.consumed_bytes.fetch_add(len, Ordering::AcqRel) + len;
        if consumed >= self.credit_threshold {
            self.consumed_bytes.store(0, Ordering::Release);
            conn.send_credits(self.channel_id, consumed);
        }
    }
}
```

## Async Integration (eventfd)

No more polling! Each direction has an eventfd for wakeups.

```rust
// During session setup (over Unix socket):
// 1. Create eventfd for each direction
// 2. Exchange fds via SCM_RIGHTS

struct Doorbell {
    eventfd: OwnedFd,
    async_fd: AsyncFd<OwnedFd>,  // tokio wrapper
}

impl Doorbell {
    fn ring(&self) {
        // Write 1 to eventfd
        let val: u64 = 1;
        unsafe {
            libc::write(self.eventfd.as_raw_fd(), &val as *const _ as *const _, 8);
        }
    }

    async fn wait(&self) {
        self.async_fd.readable().await.unwrap();
        // Read to reset
        let mut val: u64 = 0;
        unsafe {
            libc::read(self.eventfd.as_raw_fd(), &mut val as *mut _ as *mut _, 8);
        }
    }
}

// Writer task
loop {
    let frame = outgoing.recv().await;
    ring.enqueue(frame);
    doorbell.ring();  // Wake peer
}

// Reader task
loop {
    doorbell.wait().await;  // Async wait, no spinning!
    while let Some(desc) = ring.dequeue() {
        process(desc);
    }
}
```

## Cancellation

Cooperative, with deadlines.

```rust
struct CancellationToken {
    cancelled: AtomicBool,
    reason: AtomicU32,
}

// Sending cancellation
conn.send_control(ControlPayload::CancelChannel {
    channel_id,
    reason: CancelReason::Timeout,
});

// Receiver checks
if token.is_cancelled() {
    return Err(Error::Cancelled(token.reason()));
}

// Deadline enforcement
struct DeadlineEnforcer {
    deadlines: BinaryHeap<(Instant, ChannelId)>,
}

impl DeadlineEnforcer {
    async fn run(&mut self, conn: &Connection) {
        loop {
            if let Some((deadline, channel_id)) = self.deadlines.peek() {
                tokio::select! {
                    _ = tokio::time::sleep_until(*deadline) => {
                        conn.cancel_channel(*channel_id, CancelReason::DeadlineExceeded);
                        self.deadlines.pop();
                    }
                    new_deadline = self.new_deadlines.recv() => {
                        self.deadlines.push(new_deadline);
                    }
                }
            } else {
                let new = self.new_deadlines.recv().await;
                self.deadlines.push(new);
            }
        }
    }
}
```

## Observability

### Every Message Has (Logically)

- `trace_id`: 64-bit, propagated from caller or generated
- `span_id`: 64-bit, unique per message
- `timestamp_ns`: When enqueued

**Physical vs logical:** These fields are *logically* present on every message, but
*physically* they may be:
- Stored in `MsgDescCold` (when `debug_level >= 1`)
- Synthesized/defaulted (e.g., `trace_id = 0` when `debug_level = 0`)
- Stored only in process-local memory (not in SHM)

When observability is disabled (`debug_level = 0`), implementations MAY:
- Skip writing `MsgDescCold` entirely
- Default `trace_id`/`span_id` to 0 in metrics
- Still track `timestamp_ns` for latency calculations (implementation choice)

### Telemetry Ring (Optional)

A separate ring for telemetry events, readable by observer processes:

```rust
#[repr(C)]
struct TelemetryEvent {
    timestamp_ns: u64,
    trace_id: u64,
    span_id: u64,
    channel_id: u32,
    event_type: u32,      // SEND, RECV, CANCEL, TIMEOUT, ERROR
    payload_len: u32,
    latency_ns: u32,      // For RECV: time since SEND
}
```

### Metrics (in control segment)

```rust
#[repr(C)]
struct ChannelMetrics {
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    flow_control_stalls: AtomicU64,
    errors: AtomicU64,
}

#[repr(C)]
struct GlobalMetrics {
    ring_high_watermark: AtomicU64,
    total_allocations: AtomicU64,
    allocation_failures: AtomicU64,
}
```

## Fault Injection

Built-in hooks for testing:

```rust
struct FaultInjector {
    drop_rate: AtomicU32,          // 0-10000 (0.00% - 100.00%)
    delay_ms: AtomicU32,           // Artificial delay
    error_rate: AtomicU32,         // Force errors

    // Per-channel overrides
    channel_faults: DashMap<u32, ChannelFaults>,
}

impl FaultInjector {
    fn should_drop(&self, channel_id: u32) -> bool {
        let rate = self.channel_faults
            .get(&channel_id)
            .map(|f| f.drop_rate.load(Ordering::Relaxed))
            .unwrap_or_else(|| self.drop_rate.load(Ordering::Relaxed));

        rand::random::<u32>() % 10000 < rate
    }
}

// Controllable via introspection channel
enum FaultCommand {
    SetDropRate { channel_id: Option<u32>, rate: u32 },
    SetDelay { channel_id: Option<u32>, delay_ms: u32 },
    SetErrorRate { channel_id: Option<u32>, rate: u32 },
}
```

**Fault injection timing:** Fault injection MUST happen **after** descriptor validation.
This ensures that:
1. Bugs in validation logic are not masked by fault injection
2. Invalid descriptors are caught regardless of fault injection settings
3. Metrics accurately reflect validation failures vs. injected faults

```rust
fn process_frame(&self, desc: &MsgDescHot) -> Result<(), ProcessError> {
    // Step 1: Always validate first
    validate_descriptor(desc, &self.data_segment)?;

    // Step 2: Then apply fault injection (if enabled)
    if self.fault_injector.should_drop(desc.channel_id) {
        self.metrics.injected_drops.fetch_add(1, Ordering::Relaxed);
        return Ok(());  // Silently drop valid frame
    }

    // Step 3: Normal processing
    self.dispatch(desc)
}
```

## Crash Safety

### Design Philosophy

The SPSC ring is intentionally "dumb" — it has no per-slot generation or ownership tracking.
All crash safety is handled at two layers:
1. **Data segment**: Generation counters on payload slots (SlotMeta)
2. **Channel state**: Tracked in process-local memory, not SHM

This keeps the hot path simple while still enabling safe recovery.

### Generation Counters (Data Segment Only)

Payload slots have generations to detect stale references after crash recovery:

```rust
// In SlotMeta (already defined above)
struct SlotMeta {
    generation: AtomicU32,  // Incremented on each alloc
    state: AtomicU32,       // FREE / ALLOCATED / IN_FLIGHT
}

// When processing a descriptor, verify generation matches
let meta = segment.slot_meta(desc.payload_slot);
if meta.generation.load(Ordering::Acquire) != desc.payload_generation {
    // Stale reference from before crash recovery - ignore
    return Err(ValidationError::StaleGeneration);
}
```

### Heartbeats & Dead Peer Detection

```rust
// Each peer increments its epoch periodically
async fn heartbeat_task(header: &SegmentHeader, is_peer_a: bool) {
    let epoch = if is_peer_a { &header.peer_a_epoch } else { &header.peer_b_epoch };
    let timestamp = if is_peer_a { &header.peer_a_last_seen } else { &header.peer_b_last_seen };

    loop {
        epoch.fetch_add(1, Ordering::Release);
        timestamp.store(now_nanos(), Ordering::Release);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// Peer death detection
fn is_peer_alive(header: &SegmentHeader, is_peer_a: bool) -> bool {
    let timestamp = if is_peer_a {
        header.peer_a_last_seen.load(Ordering::Acquire)
    } else {
        header.peer_b_last_seen.load(Ordering::Acquire)
    };

    let age = now_nanos() - timestamp;
    age < Duration::from_secs(1).as_nanos() as u64
}
```

### Cleanup on Crash

When a peer dies, the surviving peer performs cleanup:

```rust
impl Session {
    fn cleanup_dead_peer(&self) {
        // 1. Reset ring indices
        // All in-flight descriptors are considered lost.
        // The SPSC ring has no per-slot ownership; we just reset positions.
        self.inbound_ring.reset();  // Sets visible_head = tail = 0

        // 2. Reclaim all data segment slots
        // With per-direction allocation (sender allocs, receiver frees),
        // the surviving peer can safely reclaim everything that isn't FREE.
        for (idx, meta) in self.data_segment.slot_metas().enumerate() {
            let state = meta.state.swap(SlotState::Free as u32, Ordering::AcqRel);
            if state != SlotState::Free as u32 {
                // Bump generation to invalidate any stale descriptors
                meta.generation.fetch_add(1, Ordering::Release);
                // Return to free list
                self.data_segment.push_free(idx as u32);
            }
        }

        // 3. Cancel all channels (local state, not in SHM)
        for channel in self.channels.values() {
            channel.cancel(CancelReason::PeerDied);
        }
    }
}
```

**Why no per-slot owner field?** With SPSC rings and per-direction allocation pools,
ownership is implicit: if a slot is IN_FLIGHT in the A→B data segment, peer A allocated
it and peer B will free it. On crash, the surviving peer can safely reclaim all non-FREE
slots because the dead peer can no longer reference them.

## API Surface

```rust
// Connection setup
let session = Session::create("my-service")?;
let shm_fd = session.shm_fd();
// Send shm_fd + doorbell eventfds to peer via Unix socket

// Or connect to existing
let session = Session::connect(shm_fd, doorbell_fds)?;

// Register services
session.register_service::<MyService>()?;

// Introspection
let services = session.list_services().await?;
let methods = session.get_service("com.example.foo").await?.methods;

// Unary RPC
let response: FooResponse = session
    .call("com.example.foo", "DoThing", &request)
    .with_deadline(Duration::from_secs(5))
    .with_trace_id(trace_id)
    .await?;

// Bidirectional streaming
let (tx, rx) = session
    .open_stream("com.example.chat", "Subscribe")
    .await?;

tx.send(&message).await?;
while let Some(event) = rx.recv().await {
    // ...
}
tx.close().await?;

// Cancellation
let token = session.cancellation_token();
tokio::select! {
    result = do_work() => { ... }
    _ = token.cancelled() => { return Err(Cancelled); }
}
```

## Implementation Plan

### Phase 1: Core Infrastructure
- [ ] Memory layout structs (repr(C), aligned)
- [ ] Descriptor ring (SPSC, lock-free)
- [ ] Data segment slab allocator
- [ ] eventfd doorbells
- [ ] Basic session setup via Unix socket

### Phase 2: Channel Layer
- [ ] Channel abstraction
- [ ] Frame types and serialization
- [ ] Channel lifecycle (open/close/cancel)
- [ ] Credit-based flow control

### Phase 3: Service Layer
- [ ] Service registry in SHM
- [ ] Method dispatch
- [ ] service! macro (updated)
- [ ] Introspection (channel 0)

### Phase 4: Observability
- [ ] trace_id/span_id propagation
- [ ] Telemetry ring
- [ ] Metrics in control segment
- [ ] Debug tap support

### Phase 5: Reliability
- [ ] Generation counters
- [ ] Heartbeat tasks
- [ ] Dead peer detection
- [ ] Crash cleanup
- [ ] Deadline enforcement

### Phase 6: Testing & Fault Injection
- [ ] Fault injection hooks
- [ ] Integration tests
- [ ] Stress tests
- [ ] Chaos testing

## File Structure

```
crates/rapace2/
├── Cargo.toml
├── DESIGN.md
├── src/
│   ├── lib.rs              # Public API
│   ├── layout.rs           # Memory layout structs
│   ├── ring.rs             # Descriptor ring
│   ├── alloc.rs            # Slab allocator
│   ├── doorbell.rs         # eventfd integration
│   ├── session.rs          # Session setup/teardown
│   ├── channel.rs          # Channel abstraction
│   ├── flow.rs             # Flow control
│   ├── registry.rs         # Service registry
│   ├── dispatch.rs         # Method dispatch
│   ├── service.rs          # service! macro
│   ├── observe.rs          # Observability
│   ├── cancel.rs           # Cancellation
│   ├── fault.rs            # Fault injection
│   └── cleanup.rs          # Crash recovery
└── examples/
    ├── echo_server.rs
    ├── echo_client.rs
    └── stress_test.rs
```

## Security & Validation

### Threat Model

The protocol assumes **cooperative but potentially buggy** peers. Both peers:
- Are on the same machine (no encryption needed)
- May crash at any time
- May have bugs that produce malformed data

A **fully malicious** peer can cause denial-of-service but should **never** cause
memory unsafety in a correct implementation.

### Required Validation

Implementations **MUST** validate all descriptor fields before use:

```rust
fn validate_descriptor(desc: &MsgDescHot, segment: &DataSegment) -> Result<(), ValidationError> {
    // Bounds check payload slot
    if desc.payload_slot != u32::MAX {
        if desc.payload_slot >= segment.slot_count {
            return Err(ValidationError::SlotOutOfBounds);
        }

        // Bounds check offset + length within slot
        let end = desc.payload_offset.saturating_add(desc.payload_len);
        if end > segment.slot_size {
            return Err(ValidationError::PayloadOutOfBounds);
        }

        // Generation check
        let meta = segment.slot_meta(desc.payload_slot);
        if meta.generation.load(Ordering::Acquire) != desc.payload_generation {
            return Err(ValidationError::StaleGeneration);
        }
    } else {
        // Inline payload: check length fits
        if desc.payload_len > INLINE_PAYLOAD_SIZE as u32 {
            return Err(ValidationError::InlinePayloadTooLarge);
        }
    }

    // Channel ID reasonableness (optional)
    // Method ID reasonableness (optional)

    Ok(())
}
```

### Service Name & Method Length Limits

Control payloads with strings **MUST** enforce length limits:

```rust
const MAX_SERVICE_NAME_LEN: usize = 256;
const MAX_METHOD_NAME_LEN: usize = 128;
const MAX_METADATA_KEY_LEN: usize = 64;
const MAX_METADATA_VALUE_LEN: usize = 4096;
const MAX_METADATA_PAIRS: usize = 32;
```

### Descriptor Sanity Caps

In addition to bounds checking, implementations SHOULD enforce these recommended caps
to limit DoS exposure and make resource usage predictable:

```rust
/// Recommended limits (tunable per-deployment)
struct DescriptorLimits {
    /// Maximum payload size even if slot allows more
    /// Default: 1MB. Prevents single frame from consuming excessive resources.
    max_payload_len: u32,

    /// Maximum concurrent channels per session
    /// Default: 1024. Prevents channel ID exhaustion attacks.
    max_channels: u32,

    /// Maximum in-flight frames per channel (before backpressure kicks in)
    /// Default: 64. Bounds memory usage per channel.
    max_frames_in_flight_per_channel: u32,

    /// Maximum total in-flight frames across all channels
    /// Default: 4096. Global cap for memory budgeting.
    max_frames_in_flight_total: u32,
}

impl Default for DescriptorLimits {
    fn default() -> Self {
        Self {
            max_payload_len: 1024 * 1024,      // 1MB
            max_channels: 1024,
            max_frames_in_flight_per_channel: 64,
            max_frames_in_flight_total: 4096,
        }
    }
}
```

These limits are **advisory** — implementations may adjust based on deployment
requirements. However, having explicit, documented defaults helps operators
reason about resource consumption.

### Error Handling

On validation failure:
1. **Increment error counter** (for observability)
2. **Drop the descriptor** (do not process)
3. **Optionally** send ERROR frame on control channel
4. **Never panic** - continue processing other descriptors

## Control Channel Semantics

**Channel 0 is reserved** for control and introspection messages.
The `method_id` field indicates the control verb.

**Flow control exemption:** Control channel frames (channel_id == 0) are **NOT** subject to
per-channel credit-based flow control. This ensures that critical operations like CANCEL,
PING, and credit grants cannot be blocked behind data backpressure. Implementations MUST
process control frames even when data channels are stalled.

**Global limits still apply:** Control frames ARE subject to global resource limits
(e.g., `max_frames_in_flight_total`), but MUST NOT be blocked by per-channel credit
exhaustion. Implementations SHOULD reserve headroom in global limits for control traffic.

**CONTROL flag semantics:** The CONTROL flag in FrameFlags is a convenience hint for fast-path
dispatch. Authoritative routing is based on `channel_id` and `method_id`. Implementations
SHOULD NOT rely solely on the CONTROL flag for correctness.

| method_id | Name | Direction | Description |
|-----------|------|-----------|-------------|
| 0 | Reserved | - | Invalid |
| 1 | OPEN_CHANNEL | Either | Open a new data channel |
| 2 | CLOSE_CHANNEL | Either | Close a channel gracefully |
| 3 | CANCEL_CHANNEL | Either | Cancel a channel (advisory) |
| 4 | GRANT_CREDITS | Either | Grant flow control credits |
| 5 | PING | Either | Liveness check |
| 6 | PONG | Either | Response to PING |
| 7 | LIST_SERVICES | Request | Introspection: list services |
| 8 | GET_SERVICE | Request | Introspection: get service info |
| 9 | GET_METHOD | Request | Introspection: get method info |
| 10 | GET_SCHEMA | Request | Introspection: get schema |
| 11 | SET_DEBUG_LEVEL | Either | Change debug level for channel |
| 12 | INJECT_FAULT | Either | Fault injection control |

Data channels use `channel_id > 0` with `method_id` indicating the RPC method.

## Service Registry Mutability

The service registry in SHM is **mostly immutable** after session setup:

1. **During setup**: Session creator populates the registry
2. **After handshake**: Registry is read-only
3. **Version number**: Readers can detect changes

```rust
impl ServiceRegistry {
    /// Write-once during setup, returns error if called twice
    fn initialize(&mut self, services: &[ServiceDef]) -> Result<(), RegistryError> {
        if self.version.load(Ordering::Acquire) != 0 {
            return Err(RegistryError::AlreadyInitialized);
        }
        // ... write services ...
        self.version.store(1, Ordering::Release);
        Ok(())
    }

    /// Read services, verify version hasn't changed
    /// Returns Err if registry was modified during read (should be rare)
    fn read<F, R>(&self, f: F) -> Result<R, RegistryError>
    where
        F: FnOnce(&[ServiceEntry]) -> R,
    {
        let v1 = self.version.load(Ordering::Acquire);
        let result = f(self.services());
        let v2 = self.version.load(Ordering::Acquire);
        if v1 != v2 {
            return Err(RegistryError::ModifiedDuringRead);
        }
        Ok(result)
    }
}

#[derive(Debug)]
enum RegistryError {
    AlreadyInitialized,
    ModifiedDuringRead,
}
```

**Error handling policy:** Implementations MAY choose to panic in debug builds for
`ModifiedDuringRead` (indicates a serious bug), but SHOULD return errors in release
builds to avoid crashing production systems due to unexpected registry updates.

For dynamic service registration, use the control channel to notify peers,
then coordinate a registry update with proper synchronization.

## Cancellation Semantics

### Properties

1. **Advisory**: Cancellation is a hint, not a guarantee
2. **Idempotent**: Multiple CANCEL frames for same channel are ignored
3. **Ordered**: CANCEL is a normal frame, respects ordering

### Deadline Handling

```rust
// deadline_ns in MsgHeader is absolute time in CLOCK_MONOTONIC domain

// Enforcement:
// 1. Sender sets deadline_ns = now + timeout
// 2. Receiver checks: if now > deadline_ns, treat as cancelled
// 3. Allow small slack for clock skew (same machine, should be minimal)

const DEADLINE_SLACK_NS: u64 = 1_000_000; // 1ms

fn is_past_deadline(deadline_ns: u64) -> bool {
    if deadline_ns == 0 {
        return false; // No deadline
    }
    let now = clock_monotonic_ns();
    now > deadline_ns.saturating_add(DEADLINE_SLACK_NS)
}
```

### Cancellation Chain

When a channel is cancelled:
1. Local handler receives cancellation token trigger
2. CANCEL frame is sent to peer
3. Peer should stop processing and send CANCEL or EOS back
4. Both sides clean up resources

### Late Frames After CANCEL

**Race condition:** A sender may enqueue DATA frames before receiving a CANCEL from the receiver.

**Rule:** Frames received after local cancellation MAY be dropped silently. Implementations
SHOULD NOT attempt to process stale data after cancellation, unless required for protocol
teardown (e.g., waiting for final EOS to confirm clean shutdown).

**Metrics:** Dropped-after-cancel frames SHOULD still be counted in metrics (e.g.,
`frames_dropped_after_cancel` counter) to aid debugging "why is this channel still
sending data after cancel?" scenarios.

```rust
fn on_frame_received(&self, channel_id: u32, desc: &MsgDescHot) {
    if self.is_cancelled(channel_id) {
        // Late frame after cancellation - drop silently
        // Still free the payload slot to avoid leaks
        if desc.payload_slot != u32::MAX {
            self.data_segment.free(desc.payload_slot, desc.payload_generation);
        }
        return;
    }
    // Normal processing...
}
```

## Unary RPC Definition

For clarity, **unary RPC** is defined as:

```
Client                          Server
  │                                │
  │── OPEN_CHANNEL ───────────────>│
  │   (channel_id=N, method_id=M)  │
  │                                │
  │── DATA (request) + EOS ───────>│  (exactly one request frame)
  │                                │
  │<─────── DATA (response) + EOS ─│  (exactly one response frame)
  │       or ERROR + EOS           │
  │                                │
  └── channel closed ──────────────┘
```

This maps cleanly to the channel model: open, send one + EOS, receive one + EOS, done.

## Session Handshake

Sessions are established via Unix socket, then communication moves to SHM:

```
1. Peer A creates SHM segment + eventfds
2. Peer A listens on Unix socket
3. Peer B connects to Unix socket
4. Peer A sends:
   - SHM fd (via SCM_RIGHTS)
   - Doorbell eventfd for A→B direction
   - Doorbell eventfd for B→A direction
   - Session metadata (version, capabilities)
5. Peer B maps SHM, sets up rings
6. Peer B sends ACK with its capabilities
7. Both sides begin normal operation

The Unix socket can be kept open for:
- Session teardown notification
- Out-of-band control (optional)
- Passing additional FDs later (optional)
```

## Host Managing Multiple Sessions

For a host with N plugin sessions:

```rust
struct PluginHost {
    sessions: Vec<Session>,
    poll: Poller,  // epoll/kqueue wrapper
}

impl PluginHost {
    fn run(&mut self) {
        loop {
            // Wait for any doorbell to fire
            let events = self.poll.wait();

            for event in events {
                let session_idx = event.key;
                let session = &mut self.sessions[session_idx];

                // Drain that session's ring
                while let Some(desc) = session.inbound_ring.dequeue() {
                    self.handle_frame(session_idx, desc);
                }
            }
        }
    }
}
```

This gives us "logical MPSC" (many plugins sending to one host) with
physical SPSC isolation (each plugin has its own ring).

**Scaling:** This model scales linearly with CPU cores by sharding sessions across
multiple pollers/threads. Each poller owns a subset of sessions with no cross-thread
synchronization on the hot path.
