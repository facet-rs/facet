+++
title = "Rapace SHM Transport Specification"
description = "Shared memory transport binding for Rapace"
+++

# Introduction

This document specifies the shared memory (SHM) transport binding for Rapace.
It defines how the abstract messages and semantics from the
[Core Specification](@/spec/_index.md#core-semantics) are encoded and
transmitted over shared memory.

> r[shm.scope]
>
> This binding encodes Core Semantics over shared memory. It does NOT
> redefine the meaning of calls, streams, errors, or flow control —
> only their representation in this transport.

# Topology

## Peer-to-Peer (Pair)

> r[shm.topology.pair]
>
> The "pair" topology connects exactly two peers via a shared memory
> segment. Either peer can initiate calls to the other.

In pair topology:
- One peer creates the segment (the **creator**)
- The other peer attaches to it (the **attacher**)
- This distinction affects stream ID allocation but not who can call whom

> r[shm.topology.pair.stream-ids]
>
> In pair topology, the creator MUST allocate odd stream IDs (1, 3, 5, ...).
> The attacher MUST allocate even stream IDs (2, 4, 6, ...).

## Hub (1:N) — Future

Hub topology (one host, multiple peers) is not specified in this version.
When specified, it will address:
- Peer identification and routing
- Whether IDs (request_id, stream_id) are hop-by-hop or end-to-end
- Whether flow control is hop-by-hop or end-to-end

# Segment Layout

A shared memory segment contains all state for one connection.

## Segment Header

> r[shm.segment.header]
>
> The segment MUST begin with a 128-byte header:

```
Offset  Size   Field                Description
──────  ────   ─────                ───────────
0       8      magic                Magic bytes: "RAPACE\x00\x01"
8       4      version              Segment format version (1)
12      4      header_size          Size of this header (128)
16      4      total_size           Total segment size in bytes
20      4      max_payload_size     Maximum payload per message
24      4      initial_credit       Initial stream credit (bytes)
28      4      ring_size            Descriptor ring capacity (power of 2)
32      8      a_to_b_ring_offset   Offset to A→B descriptor ring
40      8      b_to_a_ring_offset   Offset to B→A descriptor ring
48      8      slot_region_offset   Offset to payload slot region
56      4      slot_size            Size of each payload slot
60      4      slot_count           Number of payload slots
64      4      a_epoch              Creator's epoch (incremented on attach)
68      4      b_epoch              Attacher's epoch (incremented on attach)
72      4      a_goodbye            Creator's goodbye flag (0 = active)
76      4      b_goodbye            Attacher's goodbye flag (0 = active)
80      48     reserved             Reserved for future use (zero)
```

> r[shm.segment.magic]
>
> The magic field MUST be exactly `RAPACE\x00\x01` (8 bytes). If magic
> does not match, the segment MUST be rejected.

> r[shm.segment.version]
>
> If the version field is not recognized, the segment MUST be rejected.

## Descriptor Rings

Each direction has a single-producer, single-consumer (SPSC) ring of
message descriptors.

> r[shm.ring.structure]
>
> Each ring consists of:
> - `head: AtomicU32` — next slot to write (producer increments)
> - `tail: AtomicU32` — next slot to read (consumer increments)
> - `descriptors: [MsgDesc; ring_size]` — the descriptor array

> r[shm.ring.capacity]
>
> `ring_size` MUST be a power of 2. Indexing uses `index & (ring_size - 1)`.

> r[shm.ring.full-empty]
>
> The ring is empty when `head == tail`. The ring is full when
> `head - tail == ring_size`. One slot is always unused to distinguish
> full from empty.

## Payload Slot Region

Large payloads are stored in a separate slot region.

> r[shm.slots.layout]
>
> The slot region contains `slot_count` slots of `slot_size` bytes each,
> laid out contiguously.

> r[shm.slots.metadata]
>
> Each slot has associated metadata (stored separately or inline):
> - `generation: AtomicU32` — ABA counter, incremented on each allocation
> - `state: AtomicU8` — Free(0), Allocated(1), InFlight(2)

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
    pub msg_type: u8,             // Message type (see below)
    pub flags: u8,                // Message flags
    pub _reserved: [u8; 2],       // Reserved (zero)
    pub stream_id_or_request_id: u32,  // Depends on msg_type
    pub method_id: u64,           // Method ID (for Request only)

    // Payload location (16 bytes)
    pub payload_slot: u32,        // Slot index (0xFFFFFFFF = inline)
    pub payload_generation: u32,  // ABA counter (for slot payloads)
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

| Value | Message | ID Field Contains |
|-------|---------|-------------------|
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
> Payloads MUST be [POSTCARD]-encoded, consistent with the main spec.

> r[shm.payload.inline]
>
> If `payload_len <= 32`, the payload MUST be stored inline in
> `inline_payload` and `payload_slot` MUST be `0xFFFFFFFF`.

> r[shm.payload.slot]
>
> If `payload_len > 32`, the payload MUST be stored in a slot:
> - `payload_slot` is the slot index
> - `payload_generation` is the slot's generation counter at allocation
> - `payload_offset` is typically 0
> - The payload occupies `payload_len` bytes starting at the slot

> r[shm.payload.max-size]
>
> Payloads MUST NOT exceed `max_payload_size` from the segment header.
> Payloads exceeding `slot_size` MUST be rejected (use streaming for
> large data).

## Slot Lifecycle

> r[shm.slot.lifecycle]
>
> Slots follow this lifecycle:
> 1. **Free**: Available for allocation
> 2. **Allocated**: Sender has claimed it, writing payload
> 3. **InFlight**: Descriptor enqueued, receiver may read
> 4. **Free**: Receiver done, slot returned to sender's pool

> r[shm.slot.ownership]
>
> Each peer owns half the slots (for sending). Slots are returned to
> the original owner after the receiver processes them.

> r[shm.slot.generation]
>
> The generation counter MUST be incremented on each allocation. The
> receiver MUST verify that `payload_generation` matches the slot's
> current generation to detect ABA issues.

# Ordering and Synchronization

## Memory Ordering

> r[shm.ordering.ring-publish]
>
> When enqueueing a descriptor, the producer MUST:
> 1. Write the descriptor with Release ordering (or fence before)
> 2. Increment `head` with Release ordering

> r[shm.ordering.ring-consume]
>
> When dequeueing a descriptor, the consumer MUST:
> 1. Load `head` with Acquire ordering
> 2. If `head != tail`, load the descriptor with Acquire ordering
> 3. Process the message
> 4. Increment `tail` with Release ordering

> r[shm.ordering.payload-visibility]
>
> The producer MUST ensure payload bytes are visible before incrementing
> `head`. The Release ordering on descriptor write and `head` increment
> provides this guarantee.

## Wakeup Mechanism

> r[shm.wakeup.futex]
>
> On Linux, implementations SHOULD use futex for efficient waiting:
> - Wait on `head` when ring is empty
> - Wake after incrementing `head`

> r[shm.wakeup.fallback]
>
> On platforms without futex, implementations MAY use:
> - Polling with backoff
> - Platform-specific primitives (e.g., `WaitOnAddress` on Windows)
> - Unix socket pairs for signaling

# Flow Control

SHM uses shared counters for flow control instead of explicit Credit
messages. This avoids high-frequency descriptor traffic for ACKs.

## Credit Counters

> r[shm.flow.counter-location]
>
> Each stream has a `granted_total: AtomicU32` counter. The receiver
> publishes this counter; the sender reads it.

Counter location is implementation-defined (e.g., in stream metadata
region, or derived from slot availability).

## Counter Semantics

> r[shm.flow.granted-total]
>
> `granted_total` is the cumulative bytes the receiver has authorized.
> It is monotonically increasing (modulo wrap). The sender tracks
> `sent_total` locally.

> r[shm.flow.remaining-credit]
>
> Remaining credit = `granted_total - sent_total` (using wrapping
> subtraction). The sender MUST NOT send if remaining credit is less
> than the payload size.

> r[shm.flow.wrap-rule]
>
> To handle wrap safely: the difference `granted_total - sent_total`
> MUST be interpreted as a signed 32-bit value. If negative or greater
> than 2^31, this indicates corruption or a bug.

## Memory Ordering for Credit

> r[shm.flow.ordering.receiver]
>
> The receiver MUST update `granted_total` with Release ordering after
> consuming data. This ensures the sender sees credit only after the
> receiver has actually freed resources.

> r[shm.flow.ordering.sender]
>
> The sender MUST load `granted_total` with Acquire ordering before
> deciding whether to send.

## Initial Credit

> r[shm.flow.initial]
>
> All streams start with `granted_total = initial_credit` from the
> segment header. The sender's `sent_total` starts at 0.

## Zero Credit

> r[shm.flow.zero-credit]
>
> If remaining credit is zero, the sender MUST wait. Implementations
> SHOULD use a futex/condvar keyed on the counter to avoid busy-waiting.

> r[shm.flow.zero-credit-wakeup]
>
> The receiver SHOULD wake waiters after updating `granted_total`.

## Credit and Reset

> r[shm.flow.reset]
>
> After sending or receiving Reset for a stream, both peers MUST stop
> reading/writing the stream's credit counter. Counter values after
> Reset are undefined and MUST be ignored.

# Failure and Goodbye

## Goodbye Flags

> r[shm.goodbye.flags]
>
> Each peer has a goodbye flag in the segment header (`a_goodbye`,
> `b_goodbye`). Setting this flag to non-zero signals shutdown.

> r[shm.goodbye.set]
>
> To send Goodbye, a peer MUST:
> 1. Optionally enqueue a Goodbye descriptor with reason in payload
> 2. Set its goodbye flag to non-zero with Release ordering
> 3. Stop sending new messages (except Goodbye)

> r[shm.goodbye.observe]
>
> Peers MUST periodically check the other's goodbye flag. Upon observing
> a non-zero goodbye flag:
> 1. Stop initiating new calls
> 2. Drain any remaining messages from the ring
> 3. Complete or cancel in-flight work
> 4. Set own goodbye flag if not already set
> 5. Detach from the segment

## Goodbye Descriptor

> r[shm.goodbye.descriptor]
>
> The optional Goodbye descriptor (`msg_type = 7`) MAY contain a reason
> string in its payload. This provides the same information as the
> networked Goodbye message (rule ID + context).

## Crash Detection

> r[shm.crash.epoch]
>
> Each peer increments its epoch on attach. If a peer observes the other's
> epoch has changed unexpectedly, the previous session crashed.

> r[shm.crash.recovery]
>
> On detecting a crash:
> 1. Treat all in-flight operations as failed
> 2. Reset ring head/tail to empty state
> 3. Return all slots to free state
> 4. Resume normal operation with the new peer

# Byte Accounting

> r[shm.bytes.what-counts]
>
> For flow control, "bytes" means the `payload_len` of Data descriptors —
> the [POSTCARD]-encoded stream element size. Descriptor overhead, slot
> padding, and inline payload space do NOT count.

This aligns with `r[core.flow.byte-accounting]` from the main spec.

# References

- **[CORE-SPEC]** Rapace Core Specification
  <@/spec/_index.md#core-semantics>

- **[POSTCARD]** Postcard Wire Format Specification
  <https://postcard.jamesmunns.com/wire-format>
