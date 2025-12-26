# SHM Tunnel Collapse Diagnosis

## Problem Statement

HTTP tunnel benchmark shows SHM transport collapses at high concurrency with large responses:

| Scenario | Baseline | Stream | SHM |
|----------|----------|--------|-----|
| /small c=1 | 43k RPS | 14k RPS | 15k RPS |
| /small c=8 | 131k RPS | 49k RPS | 15k RPS ← no scaling |
| /large c=1 | 20k RPS | 2.6k RPS | 5k RPS |
| /large c=8 | 38k RPS | 1.8k RPS | **6 RPS** ← collapse |

## Root Cause: Slot Exhaustion

### The Math

Default SHM configuration:
- `slot_size`: 4096 bytes (4KB)
- `slot_count`: 64 slots
- **Total data capacity**: 64 × 4KB = 256KB

Tunnel chunk size:
- `CHUNK_SIZE`: 4096 bytes (in `host.rs` and `plugin.rs`)

/large response:
- Size: ~256KB (262,125 bytes)
- Frames needed: 256KB ÷ 4KB = **~64 frames per response**

At c=8 concurrency:
- Frames needed: 8 × 64 = **512 frames simultaneously**
- Slots available: **64**
- **Shortfall: 448 slots!**

### Why It Collapses

1. **Sender allocates slot** via `data_segment.alloc()` before sending
2. **Receiver frees slot** via `data_segment.free()` only when `recv_frame()` is called *again*
3. With 64 slots and 64+ frames in flight, we immediately hit `SlotError::NoFreeSlots`
4. The `alloc()` failure returns `TransportError::Encode(NoSlotAvailable)`
5. Higher layers may retry or propagate error, causing backpressure spiral

### Additional Factor: Slot Allocation is Linear Scan

From `layout.rs`:
```rust
// TODO: This could be made lock-free with a free list.
// For now, we just scan linearly.
for slot_idx in 0..self.header().slot_count {
    // Try to CAS from Free to Allocated...
}
```

At high contention, multiple threads scanning 64 slots causes CPU thrashing even when slots are available.

## Why Stream Transport Handles It Better

Stream transport (`rapace-transport-stream`) uses:
- TCP/Unix socket buffering (kernel manages flow control)
- No fixed slot pool - can buffer arbitrary amounts
- Backpressure via TCP flow control, not hard allocation failure

Stream still degrades at c=8/large (1.8k RPS vs 2.6k c=1) but doesn't collapse like SHM.

## Instrumentation Added

`ShmMetrics` now tracks:
- `alloc_success` / `alloc_failures` - slot allocation outcomes
- `slot_frees` - slot releases
- `ring_enqueues` / `ring_dequeues` / `ring_full_errors` - ring buffer operations
- `inline_sends` / `slot_sends` - frame types
- `slot_copy_bytes` - data copied to slots

Use `metrics.summary()` to print all metrics.

## Potential Fixes

### 1. Increase Slot Count (Quick Fix)
```rust
ShmSessionConfig {
    slot_count: 512,  // 8x more slots
    slot_size: 4096,
    ring_capacity: 512,  // Match ring to slots
}
```
Cost: 512 × 4KB = 2MB shared memory per session

### 2. Larger Slots, Fewer of Them
```rust
ShmSessionConfig {
    slot_count: 32,
    slot_size: 65536,  // 64KB slots
    ring_capacity: 256,
}
```
Better for large transfers, worse for small messages.

### 3. Eager Slot Freeing
Currently slots are freed lazily in `recv_frame()` when processing the *next* frame.
Could free immediately after reading, but requires more careful lifetime management.

### 4. Chunked/Streaming Transfer
Instead of one frame per 4KB chunk, use a streaming protocol where:
- Open channel, negotiate buffer
- Stream data with flow control
- Single slot can carry multiple logical chunks

### 5. Lock-Free Slot Allocator
Replace linear scan with lock-free free list (already a TODO in code).

## Recommended Next Steps

1. **Short term**: Increase slot_count to 256 or 512 for tunnel use cases
2. **Medium term**: Implement lock-free slot allocator
3. **Long term**: Design proper streaming protocol for bulk transfers

## Verification

Run instrumented benchmark:
```bash
cargo xtask bench --duration=5s --concurrency=1,8
```

Check metrics output from host/plugin stderr for `alloc_failures` count.

## Results with Large Config

Using `--shm-large` flag (256 slots × 16KB = 4MB):

| Scenario | Default SHM | Large SHM | Stream | Notes |
|----------|-------------|-----------|--------|-------|
| /small c=1 | 15k RPS | 15k RPS | 14k RPS | No change (small fits inline) |
| /small c=8 | 15k RPS | 15k RPS | 49k RPS | SHM still doesn't scale for small |
| /large c=1 | 694 RPS | **4,271 RPS** | 2,517 RPS | 6x faster, beats stream |
| /large c=8 | **5 RPS** | **4,781 RPS** | 1,745 RPS | **956x faster**, beats stream by 2.7x |

**Key findings:**
1. Large SHM config completely fixes the collapse issue
2. SHM now outperforms stream for large responses at all concurrency levels
3. Small message throughput is unchanged (they use inline frames, not slots)
4. The /small endpoint still doesn't scale with concurrency - this is a separate issue (likely dispatcher/RPC overhead, not slot exhaustion)
