+++
title = "SHM Specification"
description = "Shared memory hub transport binding for roam"
weight = 30
+++

# Introduction

This document specifies the shared memory (SHM) hub transport binding for
roam. The hub topology supports one **host** and multiple **guests** (1:N),
designed for plugin systems where a host application loads guest plugins
that communicate via shared memory.

> r[shm.scope]
>
> This binding encodes Core Semantics over shared memory. It does NOT
> redefine the meaning of calls, channels, errors, or flow control —
> only their representation in this transport.

> r[shm.architecture]
>
> This binding assumes:
> - All processes sharing the segment run on the **same architecture**
>   (same endianness, same word size, same atomic semantics)
> - Cross-process atomics are valid (typically true on modern OSes)
> - The shared memory region is cache-coherent
>
> Cross-architecture SHM is not supported.

# Topology

## Hub (1:N)

> r[shm.topology.hub]
>
> The hub topology has exactly one **host** and zero or more **guests**.
> The host creates and owns the shared memory segment. Guests attach to
> communicate with the host.

```
         ┌─────────┐
         │  Host   │
         └────┬────┘
              │
    ┌─────────┼─────────┐
    │         │         │
┌───┴───┐ ┌───┴───┐ ┌───┴───┐
│Guest 1│ │Guest 2│ │Guest 3│
└───────┘ └───────┘ └───────┘
```

> r[shm.topology.hub.communication]
>
> Guests communicate only with the host, not with each other. Each
> guest has its own BipBuffers and channel table within the shared
> segment, and all guests share the VarSlotPool.

> r[shm.topology.hub.calls]
>
> Either the host or a guest can initiate calls. The host can call
> methods on any guest; a guest can call methods on the host.

## Peer Identification

> r[shm.topology.peer-id]
>
> A guest's `peer_id` (u8) is 1 + the index of its entry in the peer
> table. Peer table entry 0 corresponds to `peer_id = 1`, entry 1 to
> `peer_id = 2`, etc. The host does not have a peer_id (it is not in
> the peer table).

> r[shm.topology.max-guests]
>
> The maximum number of guests is limited to 255 (peer IDs 1-255).
> The `max_guests` field in the segment header MUST be ≤ 255. The
> peer table has exactly `max_guests` entries.

## ID Widths

Core defines `request_id` and `channel_id` as u64. SHM uses narrower
encodings to fit in the 24-byte frame header:

> r[shm.id.request-id]
>
> SHM encodes `request_id` as u32. The upper 32 bits of Core's u64
> `request_id` are implicitly zero. Implementations MUST NOT use
> request IDs ≥ 2^32.

> r[shm.id.channel-id]
>
> SHM encodes `channel_id` as u32. The upper 32 bits of Core's u64
> `channel_id` are implicitly zero.

## Channel ID Allocation

> r[shm.id.channel-scope]
>
> Channel IDs are scoped to the guest-host pair. Two different guests
> may independently use the same `channel_id` value without collision
> because they have separate channel tables.

> r[shm.id.channel-parity]
>
> Within a guest-host pair, channel IDs use odd/even parity to prevent
> collisions:
> - The **host** allocates even channel IDs (2, 4, 6, ...)
> - The **guest** allocates odd channel IDs (1, 3, 5, ...)
>
> Channel ID 0 is reserved and MUST NOT be used.

## Request ID Scope

> r[shm.id.request-scope]
>
> Request IDs are scoped to the guest-host pair. Two different guests
> may use the same `request_id` value without collision because their
> BipBuffers are separate.

# Handshake

Core Semantics require a Hello exchange to negotiate connection parameters.
SHM replaces this with the segment header:

> r[shm.handshake]
>
> SHM does not use Hello messages. Instead, the segment header fields
> (`max_payload_size`, `initial_credit`, `max_channels`) serve as the
> host's unilateral configuration. Guests accept these values by
> attaching to the segment.

> r[shm.handshake.no-negotiation]
>
> Unlike networked transports, SHM has no negotiation — the host's
> values are authoritative. A guest that cannot operate within these
> limits MUST NOT attach.

# Segment Layout

The host creates a shared memory segment containing all communication
state for all guests.

## Segment Header

> r[shm.segment.header]
>
> The segment MUST begin with a header:

```
Offset  Size   Field                Description
──────  ────   ─────                ───────────
0       8      magic                Magic bytes: "RAPAHUB\x01"
8       4      version              Segment format version (2)
12      4      header_size          Size of this header (128)
16      8      total_size           Initial segment size in bytes
24      4      max_payload_size     Maximum payload per message
28      4      initial_credit       Initial channel credit (bytes)
32      4      max_guests           Maximum number of guests (≤ 255)
36      4      bipbuf_capacity      BipBuffer data region size in bytes per direction
40      8      peer_table_offset    Offset to peer table
48      8      slot_region_offset   (legacy, unused in v2)
56      4      slot_size            Must be 0 in v2 (fixed pools eliminated)
60      4      inline_threshold     Max frame size for inline payloads (0 = default 256)
64      4      max_channels         Max concurrent channels per guest
68      4      host_goodbye         Host goodbye flag (0 = active)
72      8      heartbeat_interval   Heartbeat interval in nanoseconds (0 = disabled)
80      8      var_slot_pool_offset Offset to shared VarSlotPool (must be non-zero in v2)
88      8      current_size         Current segment size (≥ total_size if extents appended)
96      32     reserved             Reserved for future use (zero)
```

> r[shm.segment.header-size]
>
> The segment header is 128 bytes.

> r[shm.segment.magic]
>
> The magic field MUST be exactly `RAPAHUB\x01` (8 bytes).

## Peer Table

> r[shm.segment.peer-table]
>
> The peer table contains one entry per potential guest:

```rust
#[repr(C)]
struct PeerEntry {
    state: AtomicU32,           // 0=Empty, 1=Attached, 2=Goodbye, 3=Reserved
    epoch: AtomicU32,           // Incremented on attach
    _reserved_head_tail: [AtomicU32; 4], // Reserved (v1 ring indices, zeroed in v2)
    last_heartbeat: AtomicU64,  // Monotonic tick count (see r[shm.crash.heartbeat-clock])
    ring_offset: u64,           // Offset to this guest's area (BipBuffers)
    slot_pool_offset: u64,      // Reserved in v2 (0)
    channel_table_offset: u64,  // Offset to this guest's channel table
    reserved: [u8; 8],          // Reserved (zero)
}
// Total: 64 bytes per entry
```
>
> In v2, the four ring head/tail fields are reserved (zeroed). Ring state
> (write, read, watermark) lives in the BipBuffer headers within the
> guest area instead of in the peer table.

> r[shm.segment.peer-state]
>
> Peer states:
> - **Empty (0)**: Slot available for a new guest
> - **Attached (1)**: Guest is active
> - **Goodbye (2)**: Guest is shutting down or has crashed
> - **Reserved (3)**: Host has allocated slot, guest not yet attached (see `r[shm.spawn.reserved-state]`)

## Per-Guest BipBuffers

Each guest has two BipBuffers (variable-length byte SPSC ring buffers):
- **Guest→Host BipBuffer**: Guest produces frames, host consumes
- **Host→Guest BipBuffer**: Host produces frames, guest consumes

> r[shm.bipbuf.layout]
>
> Each guest's area (at `PeerEntry.ring_offset`) contains:
> 1. Guest→Host BipBuffer: 128-byte header + `bipbuf_capacity` bytes data
> 2. Host→Guest BipBuffer: 128-byte header + `bipbuf_capacity` bytes data
> 3. Channel table: `max_channels × 16` bytes
>
> Total per guest: `align64(2 × (128 + bipbuf_capacity)) + align64(max_channels × 16)`.

> r[shm.bipbuf.header]
>
> Each BipBuffer has a 128-byte header (two cache lines):
>
> ```rust
> #[repr(C, align(64))]
> struct BipBufHeader {
>     // --- Cache line 0: producer-owned ---
>     write: AtomicU32,       // Next write position (byte offset)
>     watermark: AtomicU32,   // Wrap boundary (0 = no wrap active)
>     capacity: u32,          // Data region size in bytes (immutable)
>     _pad0: [u8; 52],
>
>     // --- Cache line 1: consumer-owned ---
>     read: AtomicU32,        // Consumed frontier (byte offset)
>     _pad1: [u8; 60],
> }
> ```
>
> Splitting producer and consumer fields onto separate cache lines avoids
> false sharing.

> r[shm.bipbuf.initialization]
>
> On segment creation, all BipBuffer memory MUST be zeroed (write=0,
> watermark=0, read=0). On guest attach, the guest MUST NOT assume buffer
> contents are valid.

> r[shm.bipbuf.grant]
>
> To reserve `n` contiguous bytes for writing (**grant**):
>
> 1. If `write >= read`:
>    - If `capacity - write >= n`: grant `[write..write+n)`. Done.
>    - Else if `read > 0`: set `watermark = write`, `write = 0`. If
>      `n < read`, grant `[0..n)`. Else undo (`write = old`, `watermark = 0`)
>      and return full.
>    - Else (`read == 0`): no room to wrap, return full.
> 2. If `write < read`:
>    - If `write + n < read`: grant `[write..write+n)`. Done.
>    - Else return full.

> r[shm.bipbuf.commit]
>
> After writing data into a granted region, the producer commits by
> advancing `write += n` with **Release** ordering. This makes the
> written bytes visible to the consumer.

> r[shm.bipbuf.read]
>
> To read available bytes:
>
> 1. Load `watermark` with **Acquire**.
> 2. If `watermark != 0` (wrap active):
>    - If `read < watermark`: readable region is `[read..watermark)`.
>    - If `read >= watermark`: set `read = 0`, `watermark = 0`, retry.
> 3. If `watermark == 0`, load `write` with **Acquire**:
>    - If `read < write`: readable region is `[read..write)`.
>    - Otherwise the buffer is empty.

> r[shm.bipbuf.release]
>
> After processing `n` bytes, the consumer releases by advancing
> `read += n` with **Release** ordering. If `read` reaches or exceeds
> `watermark`, set `read = 0` and `watermark = 0` (wrap to beginning).

> r[shm.bipbuf.full]
>
> If the BipBuffer has no room for the requested grant, the producer
> MUST wait. Implementations SHOULD use a doorbell signal or polling
> with backoff. A full BipBuffer indicates backpressure from a slow
> consumer, not a protocol error.

> r[shm.backpressure.host-to-guest]
>
> When the host cannot write to a guest's H→G BipBuffer (buffer full
> or no slots available), it MUST queue the message and retry when
> capacity becomes available. The guest signals the doorbell after
> consuming messages, which the host uses as a cue to drain its
> pending send queue.

# Message Encoding

All abstract messages from Core are encoded as variable-length frames
written into BipBuffers.

## ShmFrameHeader (24 bytes)

> r[shm.frame.header]
>
> Each frame begins with a 24-byte header (little-endian):
>
> ```text
> Offset  Size  Field        Description
> ──────  ────  ─────        ───────────
> 0       4     total_len    Frame size in bytes (including this field), padded to 4
> 4       1     msg_type     Message type (see r[shm.desc.msg-type])
> 5       1     flags        Bit 0: FLAG_SLOT_REF (payload in VarSlotPool)
> 6       2     _reserved    Reserved (zero)
> 8       4     id           request_id or channel_id (u32)
> 12      8     method_id    Method hash (u64, 0 for non-Request messages)
> 20      4     payload_len  Actual payload byte count
> ```

> r[shm.frame.alignment]
>
> `total_len` MUST be padded up to a 4-byte boundary. The padding
> bytes (between the end of the payload or slot reference and the next
> 4-byte boundary) SHOULD be zeroed.

> r[shm.frame.inline]
>
> When `FLAG_SLOT_REF` is clear (flags bit 0 = 0), the payload bytes
> immediately follow the 24-byte header inline in the BipBuffer.
> The frame occupies `align4(24 + payload_len)` bytes total.

> r[shm.frame.slot-ref]
>
> When `FLAG_SLOT_REF` is set (flags bit 0 = 1), a 12-byte `SlotRef`
> follows the header instead of inline payload:
>
> ```text
> Offset  Size  Field            Description
> ──────  ────  ─────            ───────────
> 0       1     class_idx        Size class index in VarSlotPool
> 1       1     extent_idx       Extent index within the class
> 2       2     _pad             Reserved (zero)
> 4       4     slot_idx         Slot index within the extent
> 8       4     slot_generation  ABA generation counter
> ```
>
> The actual payload is stored in the VarSlotPool slot identified by
> this reference. The frame occupies 36 bytes (`align4(24 + 12)`).

> r[shm.frame.threshold]
>
> A frame with `24 + payload_len <= inline_threshold` SHOULD be sent
> inline. Larger payloads MUST use a slot reference. The default
> inline threshold is 256 bytes (configurable via the segment header's
> `inline_threshold` field; 0 means use the default).

## Metadata Encoding

The abstract Message type (see [CORE-SPEC]) has separate `metadata` and
`payload` fields. SHM frames carry both in the payload region, combined
into a single encoded value:

> r[shm.metadata.in-payload]
>
> For Request and Response messages, the frame's payload contains both
> metadata and arguments/result, encoded as a single [POSTCARD] value:
>
> ```rust
> struct RequestPayload {
>     metadata: Vec<(String, MetadataValue)>,
>     arguments: T,  // method arguments tuple
> }
>
> struct ResponsePayload {
>     metadata: Vec<(String, MetadataValue)>,
>     result: Result<T, RoamError<E>>,
> }
> ```
>
> This differs from other transports where metadata and payload are
> separate fields in the Message enum.

> r[shm.metadata.limits]
>
> The limits from `r[call.metadata.limits]` apply: at most 128 keys,
> each value at most 16 KB. Violations are connection errors.

## Message Types

> r[shm.desc.msg-type]
>
> The `msg_type` field in `ShmFrameHeader` identifies the abstract message:
>
> | Value | Message | `id` Field Contains |
> |-------|---------|---------------------|
> | 1 | Request | `request_id` |
> | 2 | Response | `request_id` |
> | 3 | Cancel | `request_id` |
> | 4 | Data | `channel_id` |
> | 5 | Close | `channel_id` |
> | 6 | Reset | `channel_id` |
> | 7 | Goodbye | (unused) |
> | 8 | Connect | `request_id` |
> | 9 | Accept | `request_id` |
> | 10 | Reject | `request_id` |

> r[shm.flow.no-credit-message]
>
> There is no Credit message type. Credit is conveyed via shared
> atomic counters in the channel table (see [Flow Control](#flow-control)).

## Payload Encoding

> r[shm.payload.encoding]
>
> Payloads MUST be [POSTCARD]-encoded.

> r[shm.payload.inline]
>
> If `24 + payload_len <= inline_threshold`, the payload SHOULD be
> stored inline in the BipBuffer frame (see `r[shm.frame.inline]`).
> The default inline threshold is 256 bytes.

> r[shm.payload.slot]
>
> If `24 + payload_len > inline_threshold`, the payload MUST be stored
> in the shared VarSlotPool and referenced via a `SlotRef` in the frame
> (see `r[shm.frame.slot-ref]`).

## Slot Lifecycle

> r[shm.slot.allocate]
>
> To allocate a slot from the VarSlotPool:
> 1. Find the smallest size class where `slot_size >= payload_len`
> 2. Pop from that class's Treiber stack free list (see `r[shm.varslot.allocation]`)
> 3. If exhausted, try the next larger class
> 4. Increment the slot's generation counter
> 5. Write payload to the slot's data region

> r[shm.slot.free]
>
> To free a slot:
> 1. Push it back onto the size class's Treiber stack free list
>    (see `r[shm.varslot.freeing]`)
>
> The receiver frees slots after processing the message.

> r[shm.slot.exhaustion]
>
> If no free slots are available in any suitable size class, the sender
> MUST wait. Use polling with backoff. Slot exhaustion is not a protocol
> error — it indicates backpressure.

# Ordering and Synchronization

## Memory Ordering

> r[shm.ordering.ring-publish]
>
> When writing a frame to a BipBuffer:
> 1. Grant contiguous space (check write vs read positions)
> 2. Write frame header and payload into the granted region
> 3. Commit: advance `write += total_len` with **Release** ordering

> r[shm.ordering.ring-consume]
>
> When reading frames from a BipBuffer:
> 1. Load `write` with **Acquire** ordering
> 2. If readable bytes available, read frame(s)
> 3. Process message(s)
> 4. Release: advance `read += bytes_consumed` with **Release** ordering

## Wakeup Mechanism

Use doorbells for efficient cross-process notification, complemented by
polling with backoff:

> r[shm.wakeup.consumer-wait]
>
> **Consumer waiting for messages** (BipBuffer empty):
> - Wait: poll on doorbell fd (or busy-wait with backoff)
> - Wake: Producer signals doorbell after committing bytes

> r[shm.wakeup.producer-wait]
>
> **Producer waiting for space** (BipBuffer full):
> - Wait: poll on doorbell fd (or busy-wait with backoff)
> - Wake: Consumer signals doorbell after releasing bytes

> r[shm.wakeup.credit-wait]
>
> **Sender waiting for credit** (zero remaining):
> - Wait: futex_wait on `ChannelEntry.granted_total`
> - Wake: Receiver calls futex_wake on `granted_total` after updating

> r[shm.wakeup.slot-wait]
>
> **Sender waiting for slots** (VarSlotPool exhausted):
> - Wait: poll with backoff (implementation-defined strategy)
> - Wake: Receiver signals after freeing a slot back to the pool

> r[shm.wakeup.fallback]
>
> On non-Linux platforms, use polling with exponential backoff or
> platform-specific primitives (e.g., `WaitOnAddress` on Windows).

# Flow Control

SHM uses shared counters for flow control instead of explicit Credit
messages.

## Channel Metadata Table

> r[shm.flow.channel-table]
>
> Each guest-host pair has a **channel metadata table** for tracking
> active channels. The table is located at a fixed offset within the
> guest's region:

```rust
#[repr(C)]
struct ChannelEntry {
    state: AtomicU32,        // 0=Free, 1=Active, 2=Closed
    granted_total: AtomicU32, // Cumulative bytes authorized
    _reserved: [u8; 8],      // Reserved (zero)
}
// 16 bytes per entry
```

> r[shm.flow.channel-table-location]
>
> Each guest's channel table offset is stored in `PeerEntry.channel_table_offset`.
> The table size is `max_channels * 16` bytes.

> r[shm.flow.channel-table-indexing]
>
> The `channel_id` directly indexes the channel table: channel N uses
> entry N. This means:
> - Channel IDs MUST be < `max_channels`
> - Channel ID 0 is reserved; entry 0 is unused
> - Usable channel IDs are 1 to `max_channels - 1`

> r[shm.flow.channel-activate]
>
> When opening a new channel, the allocator MUST initialize the entry:
> 1. Set `granted_total = initial_credit` (from segment header)
> 2. Set `state = Active` (with Release ordering)
>
> The sender maintains its own `sent_total` counter locally (not in
> shared memory).

> r[shm.flow.channel-id-reuse]
>
> A channel ID MAY be reused after the channel is closed (Close or Reset
> received by both peers). To reuse:
> 1. Sender sends Close or Reset
> 2. Receiver sets `ChannelEntry.state = Free` (with Release ordering)
> 3. Allocator polls for `state == Free` before reusing
>
> On reuse, the allocator reinitializes per `r[shm.flow.channel-activate]`.
>
> Implementations SHOULD delay reuse to avoid races (e.g., wait for
> the entry to be Free before reallocating).

## Credit Counters

> r[shm.flow.counter-per-channel]
>
> Each active channel has a `granted_total: AtomicU32` counter in its
> channel table entry. The receiver publishes; the sender reads.

## Counter Semantics

> r[shm.flow.granted-total]
>
> `granted_total` is cumulative bytes authorized by the receiver.
> Monotonically increasing (modulo wrap).

> r[shm.flow.remaining-credit]
>
> remaining = `granted_total - sent_total` (wrapping subtraction).
> Sender MUST NOT send if remaining < payload size.

> r[shm.flow.wrap-rule]
>
> Interpret `granted_total - sent_total` as signed i32. Negative or
> > 2^31 indicates corruption.

## Memory Ordering for Credit

> r[shm.flow.ordering.receiver]
>
> Update `granted_total` with Release after consuming data.

> r[shm.flow.ordering.sender]
>
> Load `granted_total` with Acquire before deciding to send.

## Initial Credit

> r[shm.flow.initial]
>
> Channels start with `granted_total = initial_credit` from segment
> header. Sender's `sent_total` starts at 0.

## Zero Credit

> r[shm.flow.zero-credit]
>
> Sender waits. Use futex on the counter to avoid busy-wait.
> Receiver wakes after granting credit.

## Credit and Reset

> r[shm.flow.reset]
>
> After Reset, stop accessing the channel's credit counter. Values
> after Reset are undefined.

# Guest Lifecycle

## Attaching

> r[shm.guest.attach]
>
> To attach, a guest:
> 1. Opens the shared memory segment
> 2. Validates magic and version
> 3. Finds an Empty peer table entry
> 4. Atomically sets state from Empty to Attached (CAS)
> 5. Increments epoch
> 6. Begins processing

> r[shm.guest.attach-failure]
>
> If no Empty slots exist, the guest cannot attach (hub is full).

## Detaching

> r[shm.guest.detach]
>
> To detach gracefully:
> 1. Set state to Goodbye
> 2. Drain remaining messages
> 3. Complete or cancel in-flight work
> 4. Unmap segment

## Host Observing Guests

> r[shm.host.poll-peers]
>
> The host periodically checks peer states. On observing Goodbye or
> epoch change (crash), the host cleans up that guest's resources.

# Failure and Goodbye

## Goodbye

> r[shm.goodbye.guest]
>
> A guest signals shutdown by setting its peer state to Goodbye.
> It MAY send a Goodbye frame with reason first.

> r[shm.goodbye.host]
>
> The host signals shutdown by setting `host_goodbye` in the header
> to a non-zero value. Guests MUST poll this field and detach when
> it becomes non-zero.

> r[shm.goodbye.payload]
>
> A Goodbye frame's payload is a [POSTCARD]-encoded `String`
> containing the reason. Per `r[core.error.goodbye-reason]`, the
> reason MUST contain the rule ID that was violated.

> r[shm.goodbye.host-atomic]
>
> The `host_goodbye` field MUST be accessed atomically (load/store
> with at least Relaxed ordering). It is written by the host and
> read by all guests.

## Crash Detection

The host is responsible for detecting crashed guests. Epoch-based
detection only works when a new guest attaches; the host needs
additional mechanisms to detect a guest that crashed while attached.

> r[shm.crash.host-owned]
>
> The host MUST use an out-of-band mechanism to detect crashed guests.
> Common approaches:
> - Hold a process handle (e.g., `pidfd` on Linux, process handle on
>   Windows) and detect termination
> - Require guests to update a heartbeat field periodically
> - Use OS-specific death notifications

> r[shm.crash.heartbeat]
>
> If using heartbeats: each `PeerEntry` contains a `last_heartbeat:
> AtomicU64` field. Guests MUST update this at least every
> `heartbeat_interval` nanoseconds (from segment header). The host
> declares a guest crashed if heartbeat is stale by more than
> `2 * heartbeat_interval`.

> r[shm.crash.heartbeat-clock]
>
> Heartbeat values are **monotonic clock readings**, not wall-clock time.
> All processes read from the same system monotonic clock, so values are
> directly comparable without synchronization.
>
> Each process writes its current monotonic clock reading (in nanoseconds)
> to `last_heartbeat`. The host compares the guest's value against its
> own clock reading: if `host_now - guest_heartbeat > 2 * heartbeat_interval`,
> the guest is considered crashed.
>
> Platform clock sources:
> - Linux: `CLOCK_MONOTONIC` (via `clock_gettime` or `Instant`)
> - Windows: `QueryPerformanceCounter`
> - macOS: `mach_absolute_time`

> r[shm.crash.epoch]
>
> Guests increment epoch on attach. If epoch changes unexpectedly,
> the previous instance crashed and was replaced.

> r[shm.crash.recovery]
>
> On detecting a crashed guest, the host MUST:
> 1. Set the peer state to Goodbye
> 2. Treat all in-flight operations as failed
> 3. Reset BipBuffer headers (write=0, read=0, watermark=0) for both
>    the G→H and H→G buffers
> 4. Scan VarSlotPool for slots owned by the crashed peer
>    (`owner_peer == peer_id`) and return them to their free lists
> 5. Reset channel table entries to Free
> 6. Set state to Empty (allowing new guest to attach)

# Byte Accounting

> r[shm.bytes.what-counts]
>
> For flow control, "bytes" = `payload_len` of Data frames (the
> [POSTCARD]-encoded element size). Frame header overhead and slot
> padding do NOT count.

# File-Backed Segments

For cross-process communication, the SHM segment must be backed by a
file that can be memory-mapped by multiple processes.

## Segment File

> r[shm.file.path]
>
> The host creates the segment as a regular file at a path known to
> both host and guests. Common locations:
> - `/dev/shm/<name>` (Linux tmpfs, recommended)
> - `/tmp/<name>` (portable but may be disk-backed)
> - Application-specific directory
>
> The path MUST be communicated to guests out-of-band (e.g., via
> command-line argument or environment variable).

> r[shm.file.create]
>
> To create a segment file:
> 1. Open or create the file with read/write permissions
> 2. Truncate to the required `total_size`
> 3. Memory-map the entire file with `MAP_SHARED`
> 4. Initialize all data structures (header, peer table, BipBuffers,
>    VarSlotPool, channel tables)
> 5. Write header magic last (signals segment is ready)

> r[shm.file.attach]
>
> To attach to an existing segment:
> 1. Open the file read/write
> 2. Memory-map with `MAP_SHARED`
> 3. Validate magic and version
> 4. Read configuration from header
> 5. Proceed with guest attachment per `r[shm.guest.attach]`

> r[shm.file.permissions]
>
> The segment file SHOULD have permissions that allow all intended
> guests to read and write. On POSIX systems, mode 0600 or 0660 is
> typical, with the host and guests running as the same user or group.

> r[shm.file.cleanup]
>
> The host SHOULD delete the segment file on graceful shutdown. On
> crash, stale segment files may remain; implementations SHOULD handle
> this (e.g., by deleting and recreating on startup).

## Platform Mapping

> r[shm.file.mmap-posix]
>
> On POSIX systems, use `mmap()` with:
> - `PROT_READ | PROT_WRITE`
> - `MAP_SHARED` (required for cross-process visibility)
> - File descriptor from `open()` or `shm_open()`

> r[shm.file.mmap-windows]
>
> On Windows, use:
> - `CreateFileMapping()` to create a file mapping object
> - `MapViewOfFile()` to map it into the process address space
> - Named mappings can use `Global\<name>` for cross-session access

# Peer Spawning

The host typically spawns guest processes and provides them with the
information needed to attach to the segment.

## Spawn Ticket

> r[shm.spawn.ticket]
>
> Before spawning a guest, the host:
> 1. Allocates a peer table entry (finds Empty slot, sets to Reserved)
> 2. Creates a doorbell pair (see Doorbell section)
> 3. Prepares a "spawn ticket" containing:
>    - `hub_path`: Path to the segment file
>    - `peer_id`: Assigned peer ID (1-255)
>    - `doorbell_fd`: Guest's end of the doorbell (Unix only)

> r[shm.spawn.reserved-state]
>
> The peer entry state during spawning:
> - Host sets state to **Reserved** before spawn
> - Guest sets state to **Attached** after successful attach
> - If spawn fails, host resets state to **Empty**
>
> The Reserved state prevents other guests from claiming the slot.

```rust
#[repr(u32)]
pub enum PeerState {
    Empty = 0,
    Attached = 1,
    Goodbye = 2,
    Reserved = 3,  // Host has allocated, guest not yet attached
}
```

## Command-Line Arguments

> r[shm.spawn.args]
>
> The canonical way to pass spawn ticket information to a guest process
> is via command-line arguments:
>
> ```
> --hub-path=<path>    # Path to segment file
> --peer-id=<id>       # Assigned peer ID (1-255)
> --doorbell-fd=<fd>   # Doorbell file descriptor (Unix only)
> ```

> r[shm.spawn.fd-inheritance]
>
> On Unix, the doorbell file descriptor MUST be inheritable by the child
> process. The host MUST NOT set `O_CLOEXEC` / `FD_CLOEXEC` on the
> guest's doorbell fd before spawning. After spawn, the host closes its
> copy of the guest's doorbell fd (keeping only its own end).

## Guest Initialization

> r[shm.spawn.guest-init]
>
> A spawned guest process:
> 1. Parses command-line arguments to extract ticket info
> 2. Opens and maps the segment file
> 3. Validates segment header
> 4. Locates its peer entry using `peer_id`
> 5. Verifies state is Reserved (set by host)
> 6. Atomically sets state from Reserved to Attached
> 7. Initializes doorbell from the inherited fd
> 8. Begins message processing

# Doorbell Mechanism

Doorbells provide instant cross-process wakeup and death detection,
complementing BipBuffer-based communication.

## Purpose

> r[shm.doorbell.purpose]
>
> A doorbell is a bidirectional notification channel between host and
> guest that provides:
> - **Wakeup**: Signal the other side to check for work
> - **Death detection**: Detect when the other process terminates
>
> Unlike futex (which requires polling shared memory), a doorbell
> allows blocking on I/O that unblocks immediately when the peer dies.

## Implementation

> r[shm.doorbell.socketpair]
>
> On Unix, doorbells are implemented using `socketpair()`:
>
> ```c
> int fds[2];
> socketpair(AF_UNIX, SOCK_STREAM, 0, fds);
> // fds[0] = host end, fds[1] = guest end
> ```
>
> The host keeps `fds[0]` and passes `fds[1]` to the guest via the
> spawn ticket.

> r[shm.doorbell.signal]
>
> To signal the peer, write a single byte to the socket:
>
> ```c
> char byte = 1;
> write(doorbell_fd, &byte, 1);
> ```
>
> The byte value is ignored; only the wakeup matters.

> r[shm.doorbell.wait]
>
> To wait for a signal (with optional timeout):
>
> ```c
> struct pollfd pfd = { .fd = doorbell_fd, .events = POLLIN };
> poll(&pfd, 1, timeout_ms);
> if (pfd.revents & POLLIN) {
>     // Peer signaled - check for work
>     char buf[16];
>     read(doorbell_fd, buf, sizeof(buf));  // drain
> }
> if (pfd.revents & (POLLHUP | POLLERR)) {
>     // Peer died
> }
> ```

> r[shm.doorbell.death]
>
> When a process terminates, its end of the socketpair is closed by the
> kernel. The surviving process sees `POLLHUP` or `POLLERR` on its end,
> providing immediate death notification without polling.

## Integration with BipBuffers

> r[shm.doorbell.ring-integration]
>
> Doorbells complement BipBuffer-based messaging:
> - After committing a frame to the BipBuffer, signal the doorbell
> - The receiver can `poll()` both the doorbell fd and other I/O
> - On doorbell signal, check BipBuffers for new frames
>
> This avoids busy-waiting and integrates with async I/O frameworks.

> r[shm.doorbell.optional]
>
> Doorbell support is OPTIONAL. Implementations MAY use only futex-based
> wakeup (per `r[shm.wakeup.*]`). Doorbells are recommended when:
> - Death detection latency is critical
> - Integration with async I/O (epoll/kqueue/IOCP) is desired
> - Busy-waiting must be avoided entirely

# Death Notification

The host needs to detect when guest processes crash or hang so it can
clean up resources and optionally restart them.

## Notification Callback

> r[shm.death.callback]
>
> When adding a peer, the host MAY register a death callback:
>
> ```rust
> type DeathCallback = Arc<dyn Fn(PeerId) + Send + Sync>;
>
> struct AddPeerOptions {
>     peer_name: Option<String>,
>     on_death: Option<DeathCallback>,
> }
> ```
>
> The callback is invoked when the guest's doorbell indicates death
> (POLLHUP/POLLERR) or when heartbeat timeout is exceeded.

> r[shm.death.callback-context]
>
> The death callback:
> - Is called from the host's I/O or monitor thread
> - Receives the `peer_id` of the dead guest
> - SHOULD NOT block for long (schedule cleanup asynchronously)
> - MAY trigger guest restart logic

## Detection Methods

> r[shm.death.detection-methods]
>
> Implementations SHOULD use multiple detection methods:
>
> | Method | Latency | Reliability | Platform |
> |--------|---------|-------------|----------|
> | Doorbell POLLHUP | Immediate | High | Unix |
> | Heartbeat timeout | 2× interval | Medium | All |
> | Process handle | Immediate | High | All |
> | Epoch change | On reattach | Low | All |
>
> Doorbell provides the best latency on Unix. Process handles (pidfd
> on Linux, process handle on Windows) provide immediate notification
> on all platforms.

> r[shm.death.process-handle]
>
> On Linux 5.3+, use `pidfd_open()` to get a pollable fd for the child
> process. On Windows, the process handle from `CreateProcess()` is
> waitable. This provides kernel-level death notification without
> relying on doorbells.

## Recovery Actions

> r[shm.death.recovery]
>
> On guest death detection, per `r[shm.crash.recovery]`:
> 1. Invoke the death callback (if registered)
> 2. Set peer state to Goodbye, then Empty
> 3. Reset BipBuffers and reclaim VarSlotPool slots
> 4. Close host's doorbell end
> 5. Optionally respawn the guest

# Variable-Size Slot Pools

In v2, the shared VarSlotPool is the **only** slot mechanism. All
payloads that exceed the inline threshold are stored in the VarSlotPool.
Fixed-size per-guest bitmap pools (v1) have been eliminated.

## Size Classes

> r[shm.varslot.classes]
>
> A variable-size pool consists of multiple **size classes**, each with
> its own slot size and count. Example configuration:
>
> | Class | Slot Size | Count | Total | Use Case |
> |-------|-----------|-------|-------|----------|
> | 0 | 1 KB | 1024 | 1 MB | Small RPC args |
> | 1 | 16 KB | 256 | 4 MB | Typical payloads |
> | 2 | 256 KB | 32 | 8 MB | Images, CSS |
> | 3 | 4 MB | 8 | 32 MB | Compressed fonts |
> | 4 | 16 MB | 4 | 64 MB | Decompressed fonts |
>
> The specific configuration is application-dependent.

> r[shm.varslot.selection]
>
> To allocate a slot for a payload of size `N`:
> 1. Find the smallest size class where `slot_size >= N`
> 2. Allocate from that class's free list
> 3. If exhausted, try the next larger class (optional)
> 4. If all classes exhausted, block or return error

## Shared Pool Architecture

> r[shm.varslot.shared]
>
> The VarSlotPool is **shared** across all guests:
> - One pool region for the entire hub (at `var_slot_pool_offset`)
> - All guests and the host allocate from the same size classes
> - Slot ownership is tracked per-allocation
>
> This allows efficient use of memory when different guests have
> different payload size distributions.

> r[shm.varslot.ownership]
>
> Each slot tracks its current owner:
>
> ```rust
> struct SlotMeta {
>     generation: AtomicU32,  // ABA counter
>     state: AtomicU32,       // Free=0, Allocated=1, InFlight=2
>     owner_peer: AtomicU32,  // Peer ID that allocated (0 = host)
>     next_free: AtomicU32,   // Free list link
> }
> ```
>
> When a guest crashes, slots with `owner_peer == crashed_peer_id`
> are returned to their respective free lists.

## Extent-Based Growth

> r[shm.varslot.extents]
>
> Size classes can grow dynamically via **extents**:
> - Each size class starts with one extent of `slot_count` slots
> - When exhausted, additional extents can be allocated
> - Extents are appended to the segment file (requires remap)
>
> ```rust
> struct SizeClassHeader {
>     slot_size: u32,
>     slots_per_extent: u32,
>     extent_count: AtomicU32,
>     extent_offsets: [AtomicU64; MAX_EXTENTS],
> }
> ```

> r[shm.varslot.extent-layout]
>
> Each extent contains:
> 1. Extent header (class, index, slot count, offsets)
> 2. Slot metadata array (one `SlotMeta` per slot)
> 3. Slot data array (actual payload storage)
>
> ```
> ┌─────────────────────────────────────────────────┐
> │ ExtentHeader (64 bytes)                         │
> ├─────────────────────────────────────────────────┤
> │ SlotMeta[0] │ SlotMeta[1] │ ... │ SlotMeta[N-1] │
> ├─────────────────────────────────────────────────┤
> │ Slot[0] data │ Slot[1] data │ ... │ Slot[N-1]   │
> └─────────────────────────────────────────────────┘
> ```

## Free List Management

> r[shm.varslot.freelist]
>
> Each size class maintains a lock-free free list using a Treiber stack:
>
> ```rust
> struct SizeClassHeader {
>     // ...
>     free_head: AtomicU64,  // Packed (index, generation)
> }
> ```
>
> Allocation pops from the head; freeing pushes to the head. The
> generation counter prevents ABA problems.

> r[shm.varslot.allocation]
>
> To allocate from a size class:
> 1. Load `free_head` with Acquire
> 2. If empty (sentinel value), class is exhausted
> 3. Load the slot's `next_free` pointer
> 4. CAS `free_head` from current to next
> 5. On success, increment slot's generation, set state to Allocated
> 6. On failure, retry from step 1

> r[shm.varslot.freeing]
>
> To free a slot:
> 1. Verify generation matches (detect double-free)
> 2. Set slot state to Free
> 3. Load current `free_head`
> 4. Set slot's `next_free` to current head
> 5. CAS `free_head` to point to this slot
> 6. On failure, retry from step 3

# References

- **[CORE-SPEC]** roam Core Specification
  <@/spec/_index.md#core-semantics>

- **[POSTCARD]** Postcard Wire Format Specification
  <https://postcard.jamesmunns.com/wire-format>
