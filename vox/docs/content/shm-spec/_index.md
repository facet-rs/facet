+++
title = "Rapace SHM Transport Specification"
description = "Shared memory hub transport binding for Rapace"
+++

# Introduction

This document specifies the shared memory (SHM) hub transport binding for
Rapace. The hub topology supports one **host** and multiple **guests** (1:N),
designed for plugin systems where a host application loads guest plugins
that communicate via shared memory.

> r[shm.scope]
>
> This binding encodes Core Semantics over shared memory. It does NOT
> redefine the meaning of calls, streams, errors, or flow control —
> only their representation in this transport.

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
> guest has its own rings and slot pool within the shared segment.

> r[shm.topology.hub.calls]
>
> Either the host or a guest can initiate calls. The host can call
> methods on any guest; a guest can call methods on the host.

## Peer Identification

> r[shm.topology.peer-id]
>
> Each guest is assigned a `peer_id` (u32) when it attaches. The host
> has implicit `peer_id = 0`. Peer IDs are used for routing and stream
> ID allocation.

> r[shm.topology.stream-ids]
>
> Stream IDs encode the allocating peer:
> - Bits 31-24: `peer_id` of the allocator
> - Bits 23-0: sequence number (monotonically increasing per peer)
>
> This ensures globally unique stream IDs without coordination.

## ID Scope

> r[shm.topology.id-scope]
>
> Request IDs (`request_id`) and stream IDs (`stream_id`) are scoped
> to the guest-host pair, not globally. Two different guests may use
> the same `request_id` value without collision because their rings
> are separate.

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
8       4      version              Segment format version (1)
12      4      header_size          Size of this header
16      8      total_size           Total segment size in bytes
24      4      max_payload_size     Maximum payload per message
28      4      initial_credit       Initial stream credit (bytes)
32      4      max_guests           Maximum number of guests
36      4      ring_size            Descriptor ring capacity (power of 2)
40      8      peer_table_offset    Offset to peer table
48      8      slot_region_offset   Offset to payload slot region
56      4      slot_size            Size of each payload slot
60      4      slots_per_guest      Number of slots per guest
64      4      host_goodbye         Host goodbye flag (0 = active)
68      60     reserved             Reserved for future use (zero)
```

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
    state: AtomicU32,           // 0=Empty, 1=Attached, 2=Goodbye
    epoch: AtomicU32,           // Incremented on attach
    guest_to_host_head: AtomicU32,  // Ring head (guest writes)
    guest_to_host_tail: AtomicU32,  // Ring tail (host reads)
    host_to_guest_head: AtomicU32,  // Ring head (host writes)
    host_to_guest_tail: AtomicU32,  // Ring tail (guest reads)
    ring_offset: u64,           // Offset to this guest's descriptor rings
    slot_pool_offset: u64,      // Offset to this guest's slot pool
    reserved: [u8; 24],         // Reserved (zero)
}
// Total: 64 bytes per entry
```

> r[shm.segment.peer-state]
>
> Peer states:
> - **Empty (0)**: Slot available for a new guest
> - **Attached (1)**: Guest is active
> - **Goodbye (2)**: Guest is shutting down or has crashed

## Per-Guest Rings

> r[shm.segment.guest-rings]
>
> Each guest has two descriptor rings:
> - **Guest→Host ring**: Guest produces, host consumes
> - **Host→Guest ring**: Host produces, guest consumes

Each ring is an array of `ring_size` descriptors. Head/tail indices are
stored in the peer table entry.

## Slot Pools

> r[shm.segment.slot-pools]
>
> Each guest has a dedicated pool of `slots_per_guest` payload slots.
> Slots are used for payloads that exceed inline capacity.

> r[shm.segment.slot-ownership]
>
> Slots from a guest's pool are used for messages **sent by that guest**.
> After the host processes a message, the slot is returned to the guest's
> pool. Similarly, the host has its own slot pool for messages it sends.

# Message Encoding

All abstract messages from Core are encoded as 64-byte descriptors.

## MsgDesc (64 bytes)

> r[shm.desc.size]
>
> Message descriptors MUST be exactly 64 bytes (one cache line).

```rust
#[repr(C, align(64))]
pub struct MsgDesc {
    // Identity (16 bytes)
    pub msg_type: u8,             // Message type
    pub flags: u8,                // Message flags
    pub _reserved: [u8; 2],       // Reserved (zero)
    pub id: u32,                  // request_id or stream_id
    pub method_id: u64,           // Method ID (for Request only)

    // Payload location (16 bytes)
    pub payload_slot: u32,        // Slot index (0xFFFFFFFF = inline)
    pub payload_generation: u32,  // ABA counter
    pub payload_offset: u32,      // Offset within slot
    pub payload_len: u32,         // Payload length in bytes

    // Inline payload (32 bytes)
    pub inline_payload: [u8; 32], // Used when payload_slot == 0xFFFFFFFF
}
```

## Message Types

> r[shm.desc.msg-type]
>
> The `msg_type` field identifies the abstract message:

| Value | Message | `id` Field Contains |
|-------|---------|---------------------|
| 1 | Request | `request_id` |
| 2 | Response | `request_id` |
| 3 | Cancel | `request_id` |
| 4 | Data | `stream_id` |
| 5 | Close | `stream_id` |
| 6 | Reset | `stream_id` |
| 7 | Goodbye | (unused) |

Note: There is no Credit message type. Credit is conveyed via shared
counters (see [Flow Control](#flow-control)).

## Payload Encoding

> r[shm.payload.encoding]
>
> Payloads MUST be [POSTCARD]-encoded.

> r[shm.payload.inline]
>
> If `payload_len <= 32`, the payload MUST be stored inline and
> `payload_slot` MUST be `0xFFFFFFFF`.

> r[shm.payload.slot]
>
> If `payload_len > 32`, the payload MUST be stored in a slot from
> the sender's pool.

## Slot Lifecycle

> r[shm.slot.lifecycle]
>
> 1. Sender allocates slot from its pool, writes payload
> 2. Sender enqueues descriptor with slot reference
> 3. Receiver processes message, reads payload
> 4. Receiver marks slot as free (returns to sender's pool)

> r[shm.slot.generation]
>
> Each slot has a generation counter incremented on allocation. The
> receiver verifies `payload_generation` matches to detect ABA issues.

# Ordering and Synchronization

## Memory Ordering

> r[shm.ordering.ring-publish]
>
> When enqueueing a descriptor:
> 1. Write descriptor and payload with Release ordering
> 2. Increment ring head with Release ordering

> r[shm.ordering.ring-consume]
>
> When dequeueing a descriptor:
> 1. Load head with Acquire ordering
> 2. If head != tail, load descriptor with Acquire ordering
> 3. Process message
> 4. Increment tail with Release ordering

## Wakeup Mechanism

> r[shm.wakeup.futex]
>
> On Linux, use futex on the ring head for efficient waiting.
> Wake after incrementing head.

> r[shm.wakeup.fallback]
>
> On other platforms, use polling with backoff or platform-specific
> primitives.

# Flow Control

SHM uses shared counters for flow control instead of explicit Credit
messages.

## Credit Counters

> r[shm.flow.counter-per-stream]
>
> Each active stream has a `granted_total: AtomicU32` counter. The
> receiver publishes; the sender reads.

Where these counters live is implementation-defined (e.g., a stream
metadata table indexed by stream_id).

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
> Streams start with `granted_total = initial_credit` from segment
> header. Sender's `sent_total` starts at 0.

## Zero Credit

> r[shm.flow.zero-credit]
>
> Sender waits. Use futex on the counter to avoid busy-wait.
> Receiver wakes after granting credit.

## Credit and Reset

> r[shm.flow.reset]
>
> After Reset, stop accessing the stream's credit counter. Values
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
> It MAY send a Goodbye descriptor with reason first.

> r[shm.goodbye.host]
>
> The host signals shutdown by setting `host_goodbye` in the header.
> Guests observe this and detach.

## Crash Detection

> r[shm.crash.epoch]
>
> Guests increment epoch on attach. If epoch changes unexpectedly,
> the previous instance crashed.

> r[shm.crash.recovery]
>
> On crash detection:
> 1. Treat in-flight operations as failed
> 2. Reset rings to empty
> 3. Return all slots to free
> 4. Allow new guest to attach to that slot

# Byte Accounting

> r[shm.bytes.what-counts]
>
> For flow control, "bytes" = `payload_len` of Data descriptors
> (the [POSTCARD]-encoded element size). Descriptor overhead and
> slot padding do NOT count.

# References

- **[CORE-SPEC]** Rapace Core Specification
  <@/spec/_index.md#core-semantics>

- **[POSTCARD]** Postcard Wire Format Specification
  <https://postcard.jamesmunns.com/wire-format>
