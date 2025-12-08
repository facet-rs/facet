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
```

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
    // Producer side (cache-line aligned)
    head: AtomicU64,               // Next slot to write
    _pad1: [u8; 56],

    // Consumer side (separate cache line)
    tail: AtomicU64,               // Next slot to read
    _pad2: [u8; 56],

    // Published position (separate cache line)
    // Producer writes here after descriptor is ready
    visible_head: AtomicU64,
    _pad3: [u8; 56],

    // Ring configuration
    capacity: u32,                 // Power of 2
    _pad4: [u8; 60],

    // Descriptors follow (capacity * sizeof(MsgDescHot))
}
```

### Ring Algorithm (SPSC with explicit memory orderings)

The ring uses the high bits of head/tail as implicit generation/lap counters.
Slot index is computed as `idx = head & (capacity - 1)`.

```rust
impl DescRing {
    /// Producer: enqueue a descriptor
    /// Returns Err if ring is full
    fn enqueue(&self, desc: &MsgDescHot) -> Result<(), RingFull> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);  // Sync with consumer

        // Check if full: (head - tail) >= capacity
        if head.wrapping_sub(tail) >= self.capacity as u64 {
            return Err(RingFull);
        }

        let idx = (head & (self.capacity as u64 - 1)) as usize;

        // Write descriptor (non-atomic, we own this slot)
        unsafe {
            let slot = self.desc_slot(idx);
            std::ptr::write_volatile(slot, *desc);
        }

        // Publish: make descriptor visible to consumer
        // Release ordering ensures desc write is visible before head update
        self.visible_head.store(head + 1, Ordering::Release);
        self.head.store(head + 1, Ordering::Relaxed);

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
        let desc = unsafe {
            let slot = self.desc_slot(idx);
            std::ptr::read_volatile(slot)
        };

        // Advance tail (release not needed, producer only reads tail)
        self.tail.store(tail + 1, Ordering::Release);

        Some(desc)
    }

    /// Batch dequeue: drain up to N descriptors
    fn drain(&self, max: usize) -> impl Iterator<Item = MsgDescHot> {
        std::iter::from_fn(move || self.dequeue()).take(max)
    }
}
```

**Invariants:**
- `head` is only written by producer
- `tail` is only written by consumer
- `visible_head` is written by producer, read by consumer
- Ring is empty when `tail == visible_head`
- Ring is full when `head - tail >= capacity`
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

#[repr(C)]
struct SlotMeta {
    generation: AtomicU32,         // Incremented on each alloc
    state: AtomicU32,              // 0 = free, 1 = allocated, 2 = in-flight
}
```

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

```rust
#[repr(C)]
struct MsgHeader {
    version: u16,                  // Protocol version for this message
    header_len: u16,               // Bytes including this header
    encoding: u16,                 // 1=postcard, 2=json, 3=raw
    flags: u16,                    // Compression, etc.

    correlation_id: u64,           // Reply-to: msg_id of request
    deadline_ns: u64,              // Absolute deadline (0 = none)

    // Variable-length fields follow:
    // - metadata (key-value pairs for headers)
    // - payload
}
```

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

```rust
enum ControlPayload {
    OpenChannel {
        service_name: String,
        method_name: String,
        metadata: Vec<(String, Vec<u8>)>,
    },
    CloseChannel {
        reason: CloseReason,
    },
    GrantCredits {
        bytes: u32,
    },
    Ping {
        payload: [u8; 8],
    },
    Pong {
        payload: [u8; 8],
    },
}
```

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
conn.send_control(ControlPayload::Cancel {
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

### Every Message Has

- `trace_id`: 64-bit, propagated from caller or generated
- `span_id`: 64-bit, unique per message
- `timestamp_ns`: When enqueued

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

## Crash Safety

### Generation Counters

Every reusable resource has a generation:

```rust
// Descriptor slot reuse
struct DescSlot {
    generation: AtomicU32,
    desc: MsgDesc,
}

// On dequeue, verify generation matches
if slot.generation.load(Ordering::Acquire) != expected_gen {
    // Stale reference, ignore
    continue;
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

```rust
impl Session {
    fn cleanup_dead_peer(&self) {
        // 1. Mark all descriptors from dead peer as stale
        for slot in self.ring.slots() {
            if slot.owner == dead_peer_id {
                slot.generation.fetch_add(1, Ordering::Release);
                slot.owner.store(0, Ordering::Release);  // Free
            }
        }

        // 2. Return all payload slots to free list
        for slot in self.data_segment.slots() {
            if slot.owner == dead_peer_id {
                self.data_segment.free(slot);
            }
        }

        // 3. Cancel all channels owned by dead peer
        for channel in self.channels.values() {
            if channel.owner == dead_peer_id {
                channel.cancel(CancelReason::PeerDied);
            }
        }
    }
}
```

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

### Error Handling

On validation failure:
1. **Increment error counter** (for observability)
2. **Drop the descriptor** (do not process)
3. **Optionally** send ERROR frame on control channel
4. **Never panic** - continue processing other descriptors

## Control Channel Semantics

**Channel 0 is reserved** for control and introspection messages.
The `method_id` field indicates the control verb:

| method_id | Name | Direction | Description |
|-----------|------|-----------|-------------|
| 0 | Reserved | - | Invalid |
| 1 | OPEN_CHANNEL | Either | Open a new channel |
| 2 | CLOSE_CHANNEL | Either | Close a channel gracefully |
| 3 | GRANT_CREDITS | Either | Grant flow control credits |
| 4 | PING | Either | Liveness check |
| 5 | PONG | Either | Response to PING |
| 6 | LIST_SERVICES | Request | Introspection: list services |
| 7 | GET_SERVICE | Request | Introspection: get service info |
| 8 | GET_METHOD | Request | Introspection: get method info |
| 9 | GET_SCHEMA | Request | Introspection: get schema |
| 10 | SET_DEBUG_LEVEL | Either | Change debug level for channel |
| 11 | INJECT_FAULT | Either | Fault injection control |

Data channels use `channel_id > 0` with `method_id` indicating the RPC method.

## Service Registry Mutability

The service registry in SHM is **mostly immutable** after session setup:

1. **During setup**: Session creator populates the registry
2. **After handshake**: Registry is read-only
3. **Version number**: Readers can detect changes

```rust
impl ServiceRegistry {
    /// Write-once during setup, panics if called twice
    fn initialize(&mut self, services: &[ServiceDef]) {
        assert!(self.version.load(Ordering::Acquire) == 0, "Already initialized");
        // ... write services ...
        self.version.store(1, Ordering::Release);
    }

    /// Read services, verify version hasn't changed
    fn read<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[ServiceEntry]) -> R,
    {
        let v1 = self.version.load(Ordering::Acquire);
        let result = f(self.services());
        let v2 = self.version.load(Ordering::Acquire);
        assert_eq!(v1, v2, "Registry changed during read");
        result
    }
}
```

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
