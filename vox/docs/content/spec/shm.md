+++
title = "shared memory transport"
description = "Shared memory Link: hub topology, BipBuffers, VarSlotPool, signaling, and peer lifecycle"
weight = 15
+++

# Shared Memory Transport

> r[shm]
>
> The shared memory (SHM) transport implements the Link interface
> (see `r[link]`) for high-performance IPC on a single machine.
> Like all links, it delivers byte-buffer payloads — it does not
> know or care what those bytes contain. SHM provides the delivery
> mechanism (BipBuffers, VarSlotPool, signaling).

## Rust v7 API status

For Rust users, the current v7 implementation in this repository is based on
low-level composition:

1. Create/attach the shared segment with `roam_shm::segment::Segment`.
2. Manage guest slots with `reserve_peer` / `claim_peer` / `attach_peer`.
3. Build one `ShmLink` per host-guest pair with `ShmLink::for_host` or `ShmLink::for_guest`.
4. Run the normal roam runtime (`BareConduit` + session builder + `Driver`) on top of that link.

The older monolithic host orchestration API (`ShmHost`, `bootstrap`,
`MultiPeerHostDriver`) is not the primary v7 shape. Instead, v7 provides a thin
Unix helper module (`roam_shm::host`) that wraps peer reservation/spawn tickets
while keeping SHM as a transport primitive.

Host-side sketch (Unix, one peer):

```rust
use std::sync::Arc;
use roam_core::{BareConduit, Driver};
use roam_core::session::acceptor;
use roam_shm::{ShmLink, mmap_registry::{MmapChannelRx, MmapChannelTx}, segment::{Segment, SegmentConfig}, varslot::SizeClassConfig};
use roam_types::Parity;
use shm_primitives::{Doorbell, FileCleanup, MmapControlReceiver, create_mmap_control_pair};

let classes = [SizeClassConfig { slot_size: 4096, slot_count: 16 }];
let segment = Segment::create(path, SegmentConfig {
    max_guests: 1,
    bipbuf_capacity: 64 * 1024,
    max_payload_size: 4096,
    inline_threshold: 256,
    heartbeat_interval: 0,
    size_classes: &classes,
}, FileCleanup::Manual)?;

let peer_id = segment.reserve_peer().expect("no free peer slot");
let (host_doorbell, guest_doorbell_handle) = Doorbell::create_pair()?;
let (mmap_sender, mmap_receiver_handle) = create_mmap_control_pair()?;

// pass peer_id + guest_doorbell_handle + mmap_receiver_handle + shm path to guest process...

let mmap_rx = MmapControlReceiver::from_handle(mmap_receiver_handle)?;
let link = ShmLink::for_host(
    Arc::new(segment),
    peer_id,
    host_doorbell,
    MmapChannelTx::Real(mmap_sender),
    MmapChannelRx::Real(mmap_rx),
);

let conduit = BareConduit::new(link);
let (mut session, root_handle, _session_handle) = acceptor(conduit).establish().await?;
let mut driver = Driver::new(root_handle, my_dispatcher, Parity::Even);
// run session.run() and driver.run()
```

> r[shm.architecture]
>
> This transport assumes:
>
>   * All processes sharing the segment run on the **same architecture**
>     (same endianness, same word size, same atomic semantics)
>   * Cross-process atomics are valid (typically true on modern OSes)
>   * The shared memory region is cache-coherent
>
> Cross-architecture SHM is not supported.

# Topology

> r[shm.topology]
>
> The SHM transport uses a hub topology: exactly one **host** and zero
> or more **guests**. The host creates and owns the shared memory segment.
> Guests attach to communicate with the host.

```aasvg
         +---------+
         |  Host   |
         +----+----+
              |
    +---------+---------+
    |         |         |
+---+---+ +---+---+ +---+---+
|Guest 1| |Guest 2| |Guest 3|
+-------+ +-------+ +-------+
```

> r[shm.topology.communication]
>
> Guests communicate only with the host, not with each other. Each
> guest has its own pair of BipBuffers within the shared segment.

> r[shm.topology.bidirectional]
>
> Each host-guest pair forms a bidirectional Link. Either side can
> send and receive payloads.

> r[shm.topology.max-guests]
>
> The maximum number of guests is 255 (peer IDs 1–255). The
> `max_guests` field in the segment header MUST be ≤ 255.

> r[shm.topology.peer-id]
>
> A guest's `peer_id` (u8) is `1 + index` in the peer table. The host
> does not have a peer_id.

# Segment Layout

> r[shm.segment]
>
> The host creates a shared memory segment containing all communication
> state. The segment is a memory-mapped file accessible to all
> participating processes.

## Segment Header

> r[shm.segment.header]
>
> The segment MUST begin with a 128-byte header:
>
> ```text
> Offset  Size  Field               Description
> ──────  ────  ─────               ───────────
> 0       8     magic               "ROAMHUB\x07"
> 8       4     version             Segment format version
> 12      4     header_size         128
> 16      8     total_size          Segment size in bytes
> 24      4     max_payload_size    Maximum payload size in bytes
> 28      4     inline_threshold    Max inline frame size (0 = default 256)
> 32      4     max_guests          Maximum number of guests (≤ 255)
> 36      4     bipbuf_capacity     BipBuffer data region size per direction
> 40      8     peer_table_offset   Offset to peer table
> 48      8     var_pool_offset     Offset to shared VarSlotPool
> 56      8     heartbeat_interval  Heartbeat interval in nanoseconds (0 = disabled)
> 64      4     host_goodbye        Host goodbye flag (0 = active)
> 68      4     _pad                Reserved (zero)
> 72      8     current_size        Current segment size in bytes (AtomicU64)
> 80      48    reserved            Reserved (zero)
> ```

> r[shm.segment.magic.v7]
>
> The magic field MUST be exactly `ROAMHUB\x07` (8 bytes). A guest
> MUST validate the magic before proceeding.

> r[shm.segment.config]
>
> The segment header fields are transport-level configuration set by the
> host when creating the segment. Guests accept these values by attaching.
> A guest that cannot operate within these limits MUST NOT attach.
> Session-level negotiation (Hello/HelloYourself) happens above SHM,
> on the Link, as with any other transport.

## Peer Table

> r[shm.peer-table]
>
> The peer table contains one 64-byte entry per potential guest:
>
> ```text
> Offset  Size  Field                Description
> ──────  ────  ─────                ───────────
> 0       4     state                0=Empty, 1=Attached, 2=Goodbye, 3=Reserved
> 4       4     epoch                Incremented on each attach
> 8       8     last_heartbeat       Monotonic clock reading (nanoseconds)
> 16      8     ring_offset          Offset to this guest's BipBuffer pair
> 24      40    reserved             Reserved (zero)
> ```

> r[shm.peer-table.states]
>
> Peer states:
>
>   * **Empty (0)** — slot available for a new guest
>   * **Attached (1)** — guest is active
>   * **Goodbye (2)** — guest is shutting down or has crashed
>   * **Reserved (3)** — host has allocated the slot, guest not yet attached

# BipBuffers

Each guest has two BipBuffers (bipartite circular buffers):

- **Guest→Host**: guest produces, host consumes
- **Host→Guest**: host produces, guest consumes

> r[shm.bipbuf]
>
> A BipBuffer is a lock-free single-producer single-consumer byte ring.
> It guarantees contiguous grants (no wraparound mid-write), which
> allows frames to be written and read without copying.

> r[shm.bipbuf.layout]
>
> Each guest's area (at the peer entry's `ring_offset`) contains two
> BipBuffers laid out sequentially:
>
>   1. Guest→Host BipBuffer: 128-byte header + `bipbuf_capacity` bytes
>   2. Host→Guest BipBuffer: 128-byte header + `bipbuf_capacity` bytes

> r[shm.bipbuf.header]
>
> Each BipBuffer has a 128-byte header (two cache lines):
>
> ```text
> Cache line 0 (producer-owned):
>   write: AtomicU32       — next write position (byte offset)
>   watermark: AtomicU32   — wrap boundary (0 = no wrap active)
>   capacity: u32          — data region size (immutable)
>   padding: 52 bytes
>
> Cache line 1 (consumer-owned):
>   read: AtomicU32        — consumed frontier (byte offset)
>   padding: 60 bytes
> ```
>
> Splitting producer and consumer fields onto separate cache lines
> avoids false sharing.

> r[shm.bipbuf.init]
>
> On segment creation, all BipBuffer memory MUST be zeroed (write=0,
> watermark=0, read=0).

## Grant / Commit / Release Protocol

> r[shm.bipbuf.grant]
>
> To reserve `n` contiguous bytes for writing (**grant**):
>
>   1. If `write >= read`:
>      - If `capacity - write >= n`: grant `[write..write+n)`.
>      - Else if `read > 0`: set `watermark = write`, `write = 0`.
>        If `n < read`, grant `[0..n)`. Else undo and return full.
>      - Else: no room to wrap, return full.
>   2. If `write < read`:
>      - If `write + n < read`: grant `[write..write+n)`.
>      - Else: return full.

> r[shm.bipbuf.commit]
>
> After writing into a granted region, the producer commits by advancing
> `write += n` with **Release** ordering. This makes the written bytes
> visible to the consumer.

> r[shm.bipbuf.read]
>
> To read available bytes:
>
>   1. Load `watermark` with **Acquire**.
>   2. If `watermark != 0` (wrap active):
>      - If `read < watermark`: readable is `[read..watermark)`.
>      - If `read >= watermark`: set `read = 0`, `watermark = 0`, retry.
>   3. If `watermark == 0`, load `write` with **Acquire**:
>      - If `read < write`: readable is `[read..write)`.
>      - Otherwise: empty.

> r[shm.bipbuf.release]
>
> After processing `n` bytes, the consumer advances `read += n` with
> **Release** ordering. If `read` reaches or exceeds `watermark`, set
> `read = 0` and `watermark = 0`.

> r[shm.bipbuf.backpressure]
>
> If the BipBuffer has no room for the requested grant, the producer
> MUST wait. This is the backpressure point, equivalent to
> `Link::reserve()` blocking. A full BipBuffer indicates a slow
> consumer, not a protocol error.

# SHM Framing

> r[shm.framing]
>
> Each entry written to a BipBuffer has a small header that describes
> how to find the payload. The payload is an opaque byte buffer — the
> same thing any Link delivers (see `r[link.message]`).

> r[shm.framing.header]
>
> Each BipBuffer entry begins with an 8-byte header:
>
> ```text
> Offset  Size  Field        Description
> ──────  ────  ─────        ───────────
> 0       4     total_len    Entry size in bytes (padded to 4-byte boundary)
> 4       1     flags        Bit 0: SLOT_REF, Bit 1: MMAP_REF
> 5       3     reserved     Reserved (zero)
> ```

> r[shm.framing.inline]
>
> When both `SLOT_REF` and `MMAP_REF` are clear (flags bits 0 and 1 = 0),
> the payload bytes
> immediately follow the 8-byte header inline in the BipBuffer.
> The entry occupies `align4(8 + payload_len)` bytes.

> r[shm.framing.slot-ref]
>
> When `SLOT_REF` is set and `MMAP_REF` is clear
> (flags bit 0 = 1, bit 1 = 0), a 12-byte slot reference
> follows the header instead of inline payload:
>
> ```text
> Offset  Size  Field            Description
> ──────  ────  ─────            ───────────
> 0       1     class_idx        Size class index in VarSlotPool
> 1       1     extent_idx       Extent index within the class
> 2       2     reserved         Reserved (zero)
> 4       4     slot_idx         Slot index within the extent
> 8       4     generation       ABA generation counter
> ```
>
> The actual payload is stored in the VarSlotPool slot identified by
> this reference. The entry occupies 20 bytes (`align4(8 + 12)`).

> r[shm.framing.mmap-ref]
>
> When `MMAP_REF` is set and `SLOT_REF` is clear
> (flags bit 1 = 1, bit 0 = 0), a 24-byte mmap reference follows the
> header:
>
> ```text
> Offset  Size  Field            Description
> ──────  ────  ─────            ───────────
> 0       4     map_id           Mapping identifier
> 4       4     map_generation   Mapping ABA generation counter
> 8       8     map_offset       Byte offset within the mapping
> 16      4     payload_len      Payload length in bytes
> 20      4     reserved         Reserved (zero)
> ```
>
> The payload bytes live in the referenced mapping at
> `map_offset..map_offset+payload_len`. The entry occupies 32 bytes
> (`align4(8 + 24)`).
>
> `map_id` indexes a host-managed mapping registry shared by all peers.
> `map_generation` prevents ABA when a registry slot is reused.

> r[shm.framing.flags]
>
> `SLOT_REF` and `MMAP_REF` MUST NOT both be set in the same entry.

> r[shm.framing.threshold]
>
> Payload routing uses three tiers:
>
> - If `8 + payload_len <= inline_threshold`, send inline (SHOULD).
> - Else if `payload_len <= mmap_threshold`, send `SLOT_REF` (MUST).
> - Else, send `MMAP_REF` (MUST).
>
> The default inline threshold is 256 bytes (configurable via the
> segment header; 0 means use the default).
>
> `mmap_threshold` is the largest payload that can fit in any VarSlotPool
> size class (`max(slot_size)` across classes). It is derived from pool
> layout, not stored as a separate header field.

> r[shm.framing.alignment]
>
> `total_len` MUST be padded to a 4-byte boundary. Padding bytes
> SHOULD be zeroed.

# VarSlotPool

> r[shm.varslot]
>
> The VarSlotPool is a shared, lock-free allocator for payloads that
> exceed the inline threshold. It is shared across all guests and the
> host.

## Size Classes

> r[shm.varslot.classes]
>
> The pool consists of multiple **size classes**, each with its own
> slot size and count. The specific configuration is application-
> dependent. Example:
>
> | Class | Slot Size | Count | Total |
> |-------|-----------|-------|-------|
> | 0 | 1 KB | 1024 | 1 MB |
> | 1 | 16 KB | 256 | 4 MB |
> | 2 | 256 KB | 32 | 8 MB |

> r[shm.varslot.selection]
>
> To allocate a slot for a payload of `N` bytes: find the smallest size
> class where `slot_size >= N` and allocate from its free list. If
> exhausted, try the next larger class. If all candidate classes are
> exhausted, the sender MUST wait (backpressure). If no class satisfies
> `slot_size >= N`, the sender MUST use `MMAP_REF` instead of VarSlotPool.

## Slot Metadata

> r[shm.varslot.slot-meta]
>
> Each slot has metadata for ownership tracking and free list management:
>
> ```text
> generation: AtomicU32   — ABA counter, incremented on each allocation
> state: AtomicU32        — 0=Free, 1=Allocated
> owner_peer: AtomicU32   — peer_id of allocator (0 = host)
> next_free: AtomicU32    — free list link (Treiber stack)
> ```

## Free List (Treiber Stack)

> r[shm.varslot.freelist]
>
> Each size class maintains a lock-free free list using a Treiber stack.
> The head is an `AtomicU64` packing `(index, generation)` to prevent
> ABA problems.

> r[shm.varslot.allocate]
>
> To allocate from a size class:
>
>   1. Load `free_head` with Acquire
>   2. If empty (sentinel), class is exhausted
>   3. Load the slot's `next_free`
>   4. CAS `free_head` from current to next
>   5. On success: increment slot's generation, set state to Allocated,
>      set owner_peer
>   6. On failure: retry from step 1

> r[shm.varslot.free]
>
> To free a slot:
>
>   1. Verify generation matches (detect double-free)
>   2. Set state to Free
>   3. Load current `free_head`
>   4. Set slot's `next_free` to current head
>   5. CAS `free_head` to point to this slot
>   6. On failure: retry from step 3
>
> The receiver frees slots after consuming the payload.

## Extent-Based Growth

> r[shm.varslot.extents]
>
> Size classes can grow dynamically via extents. When a class is
> exhausted, additional extents can be appended to the segment file
> (requiring remap by all participants). Each extent contains slot
> metadata followed by slot data storage.

> r[shm.varslot.extents.notification]
>
> When the host appends an extent, it MUST notify all attached guests:
>
>   1. Truncate the segment file to the new size
>   2. Store the new size into `current_size` with **Release** ordering
>   3. Signal every attached guest's doorbell
>
> On each doorbell wakeup, a guest MUST load `current_size` with
> **Acquire** ordering and compare it against its current mapped size.
> If `current_size` exceeds the mapped size, the guest MUST remap the
> segment (e.g. `mremap` on Linux, unmap + remap on other platforms)
> before accessing the new extent's slots.
>
> `total_size` in the header records the initial segment size.
> `current_size` reflects the size after any extent appends; it is
> always ≥ `total_size`. Guests MUST use `current_size` to determine
> how much of the file to map.

> r[shm.varslot.crash-recovery]
>
> When a guest crashes, the host MUST scan the VarSlotPool for slots
> with `owner_peer == crashed_peer_id` and return them to their
> respective free lists.

# Mmap Payload Registry

> r[shm.mmap]
>
> `MMAP_REF` entries reference payload bytes that live outside the
> BipBuffer and VarSlotPool. The host MUST manage these mappings in a
> registry keyed by `(map_id, map_generation)`.

> r[shm.mmap.registry]
>
> A mapping registry entry contains:
>
> - mapping identity: `map_id`, `map_generation`
> - byte extent: total mapping length in bytes
> - ownership/liveness metadata
> - per-peer delivery state (which peers may still read this mapping)

> r[shm.mmap.publish]
>
> Before the producer commits a BipBuffer entry with `MMAP_REF`, the
> host MUST publish the referenced mapping in the registry and make it
> attachable by the target peer. A receiver MUST treat an unknown
> `(map_id, map_generation)` as a protocol error.

> r[shm.mmap.bounds]
>
> For each `MMAP_REF`, the receiver MUST validate
> `map_offset + payload_len <= mapping_length` from the resolved registry
> entry before exposing borrowed bytes.

> r[shm.mmap.ordering]
>
> Publication and visibility ordering MUST ensure:
>
> 1. Mapping registry entry is visible to the target peer
> 2. Mapping payload bytes are fully initialized
> 3. `MMAP_REF` frame is committed to BipBuffer
>
> A peer MUST NOT observe step 3 without being able to complete steps 1-2.

> r[shm.mmap.release]
>
> When a receiver finishes processing a payload referenced by `MMAP_REF`,
> it MUST release its delivery lease for that `(map_id, map_generation)`.

> r[shm.mmap.reclaim]
>
> The host MUST reclaim a mapping when no peer leases remain and no
> in-flight BipBuffer entries can still reference it.

> r[shm.mmap.aba]
>
> `map_generation` MUST be incremented whenever a `map_id` slot is reused.
> Receivers MUST reject stale generations to prevent ABA aliasing.

> r[shm.mmap.crash-recovery]
>
> On crashed-peer recovery, the host MUST drop all delivery leases held by
> that peer for registry mappings and reevaluate reclaim eligibility.

> r[shm.mmap.attach]
>
> A mapping is "attachable" only after the receiver has a valid
> OS-level handle for that mapping and registry metadata for
> `(map_id, map_generation, mapping_length)`.

> r[shm.mmap.attach.message]
>
> Handle-delivery metadata MUST include:
>
> - `map_id: u32`
> - `map_generation: u32`
> - `mapping_length: u64`
>
> Fields are encoded little-endian.

> r[shm.mmap.attach.unix]
>
> On Unix, the host MUST transfer a mapping fd via `sendmsg` +
> `SCM_RIGHTS` on a per-peer mapping-control socket. Exactly one fd is
> transferred per control message, and the message body MUST be the
> metadata in `r[shm.mmap.attach.message]`. The receiver MUST treat an
> fd/control-body mismatch as a protocol error.

> r[shm.mmap.attach.protocol-error]
>
> A malformed mapping-control message is a terminal protocol error for
> that SHM link. After detecting malformed mapping-control input
> (including invalid body length, truncated control data, or missing/invalid
> transferred handle), the receiver MUST fail the link immediately and
> MUST NOT continue draining mapping-control messages or resolving
> subsequent `MMAP_REF` frames on that link.

> r[shm.mmap.attach.windows]
>
> On Windows, the host MUST duplicate the mapping handle into the guest
> process (`DuplicateHandle`) and deliver the duplicated handle value
> plus metadata from `r[shm.mmap.attach.message]` over the per-peer
> mapping-control channel. The receiver MUST treat invalid or
> non-readable handles as a protocol error.

> r[shm.mmap.attach.once]
>
> A receiver MUST NOT require repeated handle delivery for the same
> `(map_id, map_generation)` tuple. Subsequent `MMAP_REF` frames MAY
> reuse the previously delivered handle until that generation is retired.

# Signaling

> r[shm.signal]
>
> SHM uses out-of-band signaling to wake blocked producers and
> consumers, avoiding busy-wait.

## Doorbells

> r[shm.signal.doorbell]
>
> A doorbell is a bidirectional notification channel between host and
> guest. On Unix, it is implemented as a `socketpair(AF_UNIX, SOCK_STREAM)`.
> The host keeps one end, the guest gets the other via the spawn ticket.

> r[shm.signal.doorbell.signal]
>
> To signal the peer: write a single byte to the socket. The byte
> value is ignored; only the wakeup matters.

> r[shm.signal.doorbell.wait]
>
> To wait for a signal: `poll()` on the doorbell fd. `POLLIN` means
> the peer signaled (drain the socket after waking). `POLLHUP` or
> `POLLERR` means the peer process has died.

> r[shm.signal.doorbell.death]
>
> When a process terminates, the kernel closes its end of the
> socketpair. The surviving process sees `POLLHUP`/`POLLERR`,
> providing immediate death notification without polling.

> r[shm.signal.doorbell.integration]
>
> After committing a frame to the BipBuffer, signal the doorbell. The
> receiver `poll()`s the doorbell and checks BipBuffers on wakeup.
> This integrates with async I/O frameworks (epoll, kqueue, IOCP).

> r[shm.signal.doorbell.optional]
>
> Doorbell support is OPTIONAL. Implementations MAY use polling with
> backoff instead. Doorbells are recommended when death detection
> latency matters or busy-waiting must be avoided.

# Guest Lifecycle

## Attaching

> r[shm.guest.attach]
>
> To attach, a guest:
>
>   1. Opens and memory-maps the segment file
>   2. Validates magic and version
>   3. Finds an Empty peer table entry (or uses its Reserved entry
>      if spawned by the host)
>   4. Atomically sets state from Empty (or Reserved) to Attached (CAS)
>   5. Increments epoch
>   6. Begins processing

> r[shm.guest.attach-failure]
>
> If no Empty or Reserved slots exist, the guest cannot attach.

## Detaching

> r[shm.guest.detach]
>
> To detach gracefully:
>
>   1. Set state to Goodbye
>   2. Drain remaining payloads
>   3. Unmap segment

## Spawning

> r[shm.spawn]
>
> The host typically spawns guest processes. Before spawning, the host:
>
>   1. Allocates a peer table entry (sets state to Reserved)
>   2. Creates a doorbell pair
>   3. Creates a per-peer mapping-control channel
>   4. Spawns the guest process, passing:
>      - Path to the segment file
>      - Assigned peer ID
>      - Guest's doorbell fd (Unix: via fd inheritance)
>      - Guest's mapping-control endpoint

> r[shm.spawn.fd-inheritance]
>
> On Unix, both the doorbell fd and mapping-control endpoint MUST be
> inheritable by the child process. The host MUST NOT set `O_CLOEXEC`
> on either guest endpoint before spawning.

## Bootstrap over Control Connection

> r[shm.bootstrap]
>
> A host MAY bootstrap SHM guests over a pre-existing control
> connection instead of relying on direct fd inheritance at spawn time.
> This bootstrap flow allocates (or confirms) a guest peer and transfers
> the SHM resources needed to attach.

> r[shm.bootstrap.request]
>
> The guest bootstrap request payload MUST be:
>
>   1. 4-byte magic `RSH0`
>   2. `sid_len` as little-endian `u16`
>   3. `sid_len` bytes of session identifier data
>
> The host MUST reject requests with the wrong magic or malformed
> length.

> r[shm.bootstrap.sid]
>
> The bootstrap wire protocol treats session identifier bytes as opaque.
> Any format constraints (for example UUID/hex policy) are
> application-specific validation layered above this protocol.

> r[shm.bootstrap.response]
>
> The host bootstrap response payload MUST be:
>
>   1. 4-byte magic `RSP0`
>   2. `status` as `u8`
>   3. `peer_id` as little-endian `u32`
>   4. `payload_len` as little-endian `u16`
>   5. `payload_len` bytes of payload

> r[shm.bootstrap.status]
>
> Bootstrap status codes are:
>
>   * `0` = success
>   * `1` = error
>
> Unknown status values MUST be treated as protocol error.

> r[shm.bootstrap.success]
>
> On success (`status = 0`), `peer_id` MUST be the assigned guest peer,
> and the host MUST transfer bootstrap file descriptors as ancillary data
> on Unix (`SCM_RIGHTS`) in this exact order:
>
>   1. guest doorbell fd
>   2. SHM segment file fd
>   3. mapping-control fd
>
> Success responses MUST include exactly 3 file descriptors.

> r[shm.bootstrap.error]
>
> On error (`status = 1`), `peer_id` MUST be zero. The payload SHOULD
> contain a UTF-8 diagnostic string. The host MUST NOT transfer SHM
> bootstrap fds for error responses.

> r[shm.bootstrap.unix]
>
> On Unix, descriptor transfer for bootstrap success MUST use
> `sendmsg`/`recvmsg` ancillary data (`SCM_RIGHTS`). The response frame
> and its fd transfer MUST correspond to the same accepted bootstrap
> request.

## Crash Detection

> r[shm.crash.detection]
>
> The host MUST detect crashed guests using at least one of:
>
>   * **Doorbell**: `POLLHUP`/`POLLERR` on the socketpair (immediate)
>   * **Process handle**: `pidfd_open()` on Linux, process handle on
>     Windows (immediate)
>   * **Heartbeat**: guest updates `last_heartbeat` periodically; host
>     declares crash if `host_now - guest_heartbeat > 2 × heartbeat_interval`
>   * **Epoch**: detects re-attach after crash (not real-time)

> r[shm.crash.heartbeat-clock]
>
> Heartbeat values are monotonic clock readings in nanoseconds. All
> processes read from the same system monotonic clock, so values are
> directly comparable.

> r[shm.crash.recovery]
>
> On detecting a crashed guest, the host MUST:
>
>   1. Set the peer state to Goodbye
>   2. Scan the H2G BipBuffer for any `SLOT_REF` and `MMAP_REF` entries
>      and reclaim those backing resources (they are host-owned but were
>      in flight to the crashed guest and will never be released by the
>      receiver)
>   3. Reset both BipBuffer headers (write=0, read=0, watermark=0)
>   4. Scan VarSlotPool for slots with `owner_peer == crashed_peer_id`
>      and return them to their free lists
>   5. Drop all mapping-registry leases held by `crashed_peer_id` and
>      reclaim mappings that become unreferenced
>   6. Set state to Empty (allowing a new guest to attach)
>
> Step 2 MUST precede step 3: once the H2G BipBuffer is reset its
> content is gone and in-flight `SLOT_REF`/`MMAP_REF` entries cannot be
> recovered.

## Host Shutdown

> r[shm.host.goodbye]
>
> The host signals shutdown by setting `host_goodbye` in the segment
> header to a non-zero value. Guests MUST poll this field and detach
> when it becomes non-zero. `host_goodbye` MUST be accessed atomically.

# File-Backed Segments

> r[shm.file]
>
> For cross-process communication, the segment is backed by a
> memory-mapped file.

> r[shm.file.create]
>
> To create a segment:
>
>   1. Create the file with read/write permissions
>   2. Truncate to `total_size`
>   3. Memory-map with `MAP_SHARED` (POSIX) or `CreateFileMapping` +
>      `MapViewOfFile` (Windows)
>   4. Initialize all data structures
>   5. Write magic last (signals segment is ready)

> r[shm.file.attach]
>
> To attach to an existing segment:
>
>   1. Open the file read/write
>   2. Memory-map with `MAP_SHARED`
>   3. Validate magic and version
>   4. Proceed with guest attachment

> r[shm.file.cleanup]
>
> The host SHOULD delete the segment file on graceful shutdown. Stale
> files from crashes SHOULD be detected and recreated on startup.

> r[shm.file.permissions]
>
> The segment file SHOULD have permissions that allow all intended
> guests to read and write. On POSIX systems, mode 0666 is typical,
> with the host and guests running as the same user.

> r[shm.file.mmap-windows]
>
> On Windows, use `CreateFileMapping()` to create a file mapping object
> and `MapViewOfFile()` to map it into the process address space.
