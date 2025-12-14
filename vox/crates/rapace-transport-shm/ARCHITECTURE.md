# Shared Memory Transport Architecture

rapace-transport-shm provides two distinct SHM architectures optimized for different use cases:

1. **Two-Peer Sessions** (SPSC) - Simple point-to-point communication
2. **Hub Architecture** (MPMC) - Multi-peer communication with shared memory pool

---

## Table of Contents

1. [Architecture Comparison](#architecture-comparison)
2. [Two-Peer Session Architecture](#two-peer-session-architecture)
3. [Hub Architecture](#hub-architecture)
4. [Lock-Free Allocation](#lock-free-allocation)
5. [Thread Safety Guarantees](#thread-safety-guarantees)
6. [Resource Exhaustion](#resource-exhaustion)
7. [When to Use Which](#when-to-use-which)

---

## Architecture Comparison

| Feature | Two-Peer Session | Hub Architecture |
|---------|------------------|------------------|
| **Peers** | Exactly 2 | Up to 32 |
| **Concurrency** | SPSC (Single-Producer Single-Consumer) | MPMC (Multi-Producer Multi-Consumer) |
| **Allocation** | Fixed-size slab allocator | Size-class based with lock-free Treiber stacks |
| **Slot Sizes** | Single size (configurable, default 64KB) | 5 size classes: 1KB, 16KB, 256KB, 4MB, 16MB |
| **Memory Layout** | Two descriptor rings + slab region | Peer table + per-peer rings + shared slot pool |
| **Use Case** | Host ↔ Single Plugin | Host ↔ Many Plugins |
| **Growth** | Fixed at creation | Extents can be appended (future) |

---

## Two-Peer Session Architecture

### Memory Layout

```
┌─────────────────────────────────────────────────────────────┐
│  Segment Header (64 bytes)                                   │
│    magic: "RAPASHM\0"                                        │
│    version, generation counters                              │
├─────────────────────────────────────────────────────────────┤
│  A→B Descriptor Ring                                         │
│    DescRingHeader (192 bytes)                                │
│    Ring Buffer (capacity × MsgDescHot)                       │
│    - head, tail (atomics for SPSC synchronization)           │
├─────────────────────────────────────────────────────────────┤
│  B→A Descriptor Ring                                         │
│    DescRingHeader (192 bytes)                                │
│    Ring Buffer (capacity × MsgDescHot)                       │
├─────────────────────────────────────────────────────────────┤
│  Data Segment (slab allocator)                               │
│    DataSegmentHeader (128 bytes)                             │
│    Slot Metadata Array (slot_count × SlotMeta)              │
│    Slot Data Array (slot_count × slot_size)                 │
│    - Each slot: state (Free/Allocated/InFlight), generation  │
└─────────────────────────────────────────────────────────────┘
```

### Characteristics

- **SPSC Rings**: Each direction has a dedicated ring with one producer, one consumer
  - No CAS loops needed for ring operations
  - Simple atomic head/tail updates
  - Cache-line aligned for performance

- **Fixed-Size Slots**: All slots are the same size
  - Predictable memory layout
  - Simple allocation (just grab next free slot)
  - Waste for small payloads, but simple and fast

- **Generation Counters**: Detect stale references after peer crashes
  - Each slot has a generation counter
  - Incremented on each alloc/free cycle
  - Stale references fail gracefully

### When to Use

- Host communicating with a **single** plugin
- Fixed message size distribution
- Maximum simplicity and performance
- No need for dynamic peer management

---

## Hub Architecture

### Memory Layout

```
+-------------------------------------------------------------------+
| HUB HEADER (256 bytes)                                            |
|   magic: "RAPAHUB\0", version, max_peers                          |
|   peer_id_counter, active_peers                                   |
|   current_size (atomic), extent_count (atomic)                    |
|   Offsets: peer_table, ring_region, size_classes, extents        |
+-------------------------------------------------------------------+
| PEER TABLE (max_peers × 64 bytes)                                 |
|   Per peer:                                                        |
|     - peer_id, flags (ACTIVE/DYING/DEAD/RESERVED)                 |
|     - epoch, last_seen                                             |
|     - futex words for notification                                 |
|     - send_ring_offset, recv_ring_offset                          |
+-------------------------------------------------------------------+
| RING REGION (max_peers × 2 rings × ~17KB each)                    |
|   Each peer gets TWO rings:                                        |
|     - Send ring: host → peer                                       |
|     - Recv ring: peer → host                                       |
|   Each ring: DescRingHeader (192B) + capacity × MsgDescHot (64B)  |
+-------------------------------------------------------------------+
| SIZE CLASS HEADERS (5 × 128 bytes)                                 |
|   Per class:                                                       |
|     - slot_size, extent_slot_shift                                 |
|     - free_head (atomic, tagged pointer to Treiber stack)          |
|     - slot_available (futex for waiting - future)                  |
|     - extent_offsets[16] (atomic array of extent locations)        |
+-------------------------------------------------------------------+
| EXTENT REGION (growable)                                           |
|   Extent 0 (Size Class 0: 1KB slots):                              |
|     - ExtentHeader: size_class, slot_size, slot_count              |
|     - Slot Metadata Array: slot_count × HubSlotMeta                |
|       (state, owner_peer, generation, next_free, refcount)         |
|     - Slot Data Array: slot_count × 1KB                            |
|                                                                    |
|   Extent 1 (Size Class 1: 16KB slots):                             |
|     - ExtentHeader                                                 |
|     - Slot Metadata Array: slot_count × HubSlotMeta                |
|     - Slot Data Array: slot_count × 16KB                           |
|                                                                    |
|   ... (up to 5 size classes × 16 extents each = 80 extents max)   |
+-------------------------------------------------------------------+
```

### Size Classes

The hub uses **5 size classes** optimized for different payload types:

| Class | Slot Size | Initial Count | Total Memory | Typical Use Case |
|-------|-----------|---------------|--------------|------------------|
| 0 | 1 KB | 1024 | 1 MB | Small RPC args/responses |
| 1 | 16 KB | 256 | 4 MB | Typical payloads, JSON |
| 2 | 256 KB | 32 | 8 MB | Images, CSS bundles |
| 3 | 4 MB | 8 | 32 MB | Compressed fonts, video |
| 4 | 16 MB | 4 | 64 MB | Decompressed assets |

**Total initial allocation**: ~109 MB (extents created on demand)

### Multi-Peer Communication

- **Host** creates the hub and manages peer lifecycle
- **Plugins** connect as peers (up to 32 total)
- Each peer gets:
  - Unique `peer_id` (allocated atomically)
  - Two dedicated descriptor rings (send/recv)
  - Doorbell eventfd pair for async notification
  - Shared access to the slot pool

### Peer Lifecycle

```rust
// Host creates hub
let hub_host = HubHost::create("/tmp/my-hub.shm", HubConfig::default())?;

// Add a peer (reserves peer_id, creates rings)
let peer_info = hub_host.add_peer()?;
// peer_info.peer_id = 1
// peer_info.doorbell = host's end (for waking plugin)
// peer_info.peer_doorbell_fd = plugin's end (pass via CLI)

// Plugin connects
let hub_peer = HubPeer::connect("/tmp/my-hub.shm", peer_info.peer_id)?;

// Both can now allocate from shared slot pool
```

Peer flags track state:
- `RESERVED`: Host allocated peer_id, waiting for plugin to connect
- `ACTIVE`: Peer connected and running
- `DYING`: Graceful shutdown in progress
- `DEAD`: Crash detected (via generation mismatch or timeout)

---

## Lock-Free Allocation

The hub uses **Treiber stacks** (lock-free LIFO stacks) for each size class. This enables **MPMC allocation** without locks.

### Algorithm

Each size class maintains:
```rust
pub struct SizeClassHeader {
    slot_size: u32,

    // Tagged pointer: (tag << 32) | global_index
    // Tag prevents ABA problem
    free_head: AtomicU64,

    extent_offsets: [AtomicU64; 16],
    // ...
}

pub struct HubSlotMeta {
    state: AtomicU32,        // Free → Allocated → InFlight → Free
    owner_peer: AtomicU32,   // Which peer owns this slot
    generation: AtomicU32,   // Detects stale references
    next_free: AtomicU32,    // Pointer for Treiber stack
    refcount: AtomicU32,     // (unused currently, for future shared refs)
}
```

### Allocation Steps

1. **Find size class**:
   ```rust
   let class = find_smallest_class(payload_size);
   // e.g., 10KB payload → class 1 (16KB slots)
   ```

2. **Pop from Treiber stack** (lock-free CAS loop):
   ```rust
   loop {
       let old_head = free_head.load(Acquire);
       let (global_index, tag) = unpack(old_head);

       if global_index == FREE_LIST_END {
           return NoFreeSlots; // Exhausted!
       }

       let meta = slot_meta(class, global_index);
       let next = meta.next_free.load(Acquire);

       let new_head = pack(next, tag + 1); // Increment tag for ABA safety

       if free_head.compare_exchange_weak(old_head, new_head, AcqRel, Acquire).is_ok() {
           // Successfully popped!
           meta.state.store(Allocated, Release);
           meta.owner_peer.store(peer_id, Release);
           meta.generation.fetch_add(1, AcqRel);
           return Ok(global_index);
       }
       // CAS failed, retry
   }
   ```

3. **Fallback to larger classes**:
   If class 1 is exhausted, try class 2, 3, 4...

   This wastes space but prevents allocation failure when large slots are available.

### Freeing

Reverse process - push onto Treiber stack:

```rust
fn free(class, global_index, expected_generation) {
    let meta = slot_meta(class, global_index);

    // Verify generation (prevents use-after-free)
    assert_eq!(meta.generation.load(Acquire), expected_generation);

    loop {
        let old_head = free_head.load(Acquire);
        let (old_index, tag) = unpack(old_head);

        meta.next_free.store(old_index, Release);
        let new_head = pack(global_index, tag + 1);

        if free_head.compare_exchange_weak(old_head, new_head, AcqRel, Acquire).is_ok() {
            meta.state.store(Free, Release);
            return;
        }
    }
}
```

### ABA Problem Prevention

The **tag** in the free_head prevents the ABA problem:

```
Thread 1: Reads head = (A, tag=0)
Thread 2: Pops A, pops B, pushes A back → head = (A, tag=2)
Thread 1: CAS(old=(A,0), new=(B,1)) → FAILS because tag changed!
```

Without the tag, Thread 1's CAS would succeed incorrectly.

---

## Thread Safety Guarantees

### Two-Peer Sessions (SPSC)

- **Each ring**: Single producer, single consumer
  - Producer updates `tail` (atomic)
  - Consumer updates `head` (atomic)
  - No contention, no CAS needed

- **Slot allocator**: Protected by the SPSC rings
  - Only one side allocates at a time (producer)
  - Only one side frees at a time (consumer)
  - State transitions: Free → Allocated → InFlight → Free

### Hub Architecture (MPMC)

- **Per-peer rings**: SPSC semantics
  - Host owns `send_ring.tail` (host → peer)
  - Peer owns `send_ring.head` (consumes)
  - Peer owns `recv_ring.tail` (peer → host)
  - Host owns `recv_ring.head` (consumes)

- **Shared slot pool**: MPMC lock-free
  - Treiber stacks are **wait-free for reads**, **lock-free for writes**
  - CAS loops have bounded retry (exponential backoff possible)
  - All state transitions use `compare_exchange`

- **Generation counters**: Detect stale references
  - Incremented on every alloc/free
  - Checked on `mark_in_flight()` and `free()`
  - Prevents use-after-free across peer crashes

### Data Races

None! All shared state uses atomics:
- Ring head/tail: `AtomicU64`
- Slot state: `AtomicU32` (Free/Allocated/InFlight)
- Free list heads: `AtomicU64` (tagged pointer)
- Generation counters: `AtomicU32`

### Memory Ordering

- **Acquire/Release** for slot ownership transfer
  - Producer: `Allocated` with `Release` → consumer sees payload
  - Consumer: `InFlight` check with `Acquire` → sees payload

- **AcqRel** for CAS operations (free list push/pop)

---

## Resource Exhaustion

### What Happens When Slots Run Out?

Currently: **`HubSlotError::NoFreeSlots`** is returned.

The allocator tries:
1. Requested size class
2. Fallback to larger classes (avoids waste when possible)
3. If all classes exhausted → error

**Future enhancement**: Futex-based waiting
```rust
// Allocator would block on futex:
while let Err(NoFreeSlots) = alloc(size, peer) {
    futex_wait(&size_class.slot_available);
}

// Freeing side would wake waiters:
free(class, index, gen);
futex_wake(&size_class.slot_available);
```

### Preventing Exhaustion

1. **Right-size your hub**:
   - More peers → more concurrent allocations → increase slot counts
   - Large payloads → ensure class 3/4 have enough slots

2. **Monitor metrics** (via `HubAllocator::status()`):
   ```rust
   for class in 0..5 {
       let status = allocator.status(class);
       eprintln!("Class {}: {}/{} free",
                 class, status.free_count, status.total_count);
   }
   ```

3. **Backpressure**:
   - Limit in-flight requests per peer
   - Use streaming for large transfers
   - Free slots promptly after processing

### Growing the Hub (Future)

The layout supports appending new extents:

```rust
// Future API (not yet implemented):
hub_host.add_extent(class)?;
// Appends a new extent to the file, updates size_class.extent_offsets
```

This requires:
- File truncation + `mremap()` on Linux
- Atomic updates to `HubHeader.current_size`
- Initializing new extent's free list
- Thread-safe coordination with existing allocators

---

## When to Use Which

### Use **Two-Peer Sessions** when:

- ✅ Communicating with a **single plugin** only
- ✅ Message size distribution is uniform (fixed size works)
- ✅ Want maximum simplicity (no multi-peer coordination)
- ✅ Don't need dynamic peer addition/removal

### Use **Hub Architecture** when:

- ✅ Hosting **multiple plugins** (e.g., browser with many extensions)
- ✅ Variable message sizes (small RPCs + large assets)
- ✅ Need dynamic peer lifecycle (plugins connect/disconnect)
- ✅ Want efficient memory usage (size classes avoid waste)
- ✅ Okay with slightly more complexity

### Concrete Example: Browser Architecture

**Two-Peer**: Simple enough for host ↔ GPU process
```rust
// GPU process handles rendering only, fixed message sizes
let session = ShmSession::create_pair()?;
```

**Hub**: Better for host ↔ many content processes
```rust
// Each tab is a peer, varying payload sizes (tiny RPCs, huge images)
let hub = HubHost::create("/tmp/browser-hub.shm", HubConfig::default())?;
for tab in tabs {
    let peer_info = hub.add_peer()?;
    spawn_content_process(peer_info);
}
```

---

## Code Examples

### Two-Peer Session

```rust
use rapace_transport_shm::{ShmSession, ShmSessionConfig};

let config = ShmSessionConfig {
    ring_capacity: 256,
    slot_size: 65536,   // 64KB
    slot_count: 128,    // 8MB total
};

let (session_a, session_b) = ShmSession::create_pair_with_config(config)?;

// session_a: peer A (host)
// session_b: peer B (plugin)
```

### Hub Architecture

```rust
use rapace_transport_shm::{HubHost, HubPeer, HubConfig};

// Host side
let hub = HubHost::create("/tmp/my-hub.shm", HubConfig::default())?;

let peer_info = hub.add_peer()?;
println!("Peer {} allocated", peer_info.peer_id);

// Pass peer_info.peer_doorbell_fd to plugin via CLI args
spawn_plugin(&[
    "--shm-path", "/tmp/my-hub.shm",
    "--peer-id", &peer_info.peer_id.to_string(),
    "--doorbell-fd", &peer_info.peer_doorbell_fd.to_string(),
]);

// Plugin side (in plugin binary)
let peer_id = parse_arg("--peer-id")?;
let hub_peer = HubPeer::connect("/tmp/my-hub.shm", peer_id)?;

// Both can now use the shared slot pool
let transport = HubPeerTransport::new(hub_peer);
```

---

## Performance Characteristics

### Two-Peer Session

- **Ring operations**: O(1), wait-free
- **Slot allocation**: O(1), scan free bitmap
- **Memory overhead**: ~192 bytes per ring + SlotMeta per slot
- **Cache efficiency**: Excellent (SPSC = no false sharing)

### Hub Architecture

- **Ring operations**: O(1), SPSC per peer
- **Slot allocation**: O(1) expected, lock-free CAS
  - Worst case: O(num_size_classes) with fallback
- **Memory overhead**:
  - Peer table: 64 bytes × max_peers
  - Rings: ~17KB × 2 × max_peers
  - Slot metadata: 32 bytes per slot
- **Cache efficiency**: Good (tagged pointers reduce false sharing)

### Benchmarks (TODO)

- Allocation latency: < 100ns (no contention)
- Throughput: > 10M ops/sec (MPMC pool)
- Tail latency: < 1µs (p99)

---

## Further Reading

- **Source Files**:
  - Two-peer: `src/session.rs`, `src/layout.rs`
  - Hub: `src/hub_session.rs`, `src/hub_layout.rs`, `src/hub_alloc.rs`

- **Papers**:
  - Treiber Stack: "Systems Programming: Coping with Parallelism" (1986)
  - Tagged Pointers for ABA: "Practical lock-freedom" (2004)

- **Related Projects**:
  - DPDK: SPSC ring buffers for packet I/O
  - Disruptor: High-performance ring buffer pattern

---

## Migration Guide

### From Two-Peer to Hub

If you're currently using two-peer sessions and need to support multiple peers:

**Before**:
```rust
let (host_session, plugin_session) = ShmSession::create_pair()?;
```

**After**:
```rust
// Host
let hub = HubHost::create(path, HubConfig::default())?;
let peer_info = hub.add_peer()?;

// Plugin
let peer = HubPeer::connect(path, peer_id)?;
let transport = HubPeerTransport::new(peer);
```

The RPC layer (`RpcSession`) is unchanged - only the transport changes.

---

**Last Updated**: 2024-12-15
**Rapace Version**: 0.4.0
