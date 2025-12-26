# shm-primitives

[![crates.io](https://img.shields.io/crates/v/shm-primitives.svg)](https://crates.io/crates/shm-primitives)
[![documentation](https://docs.rs/shm-primitives/badge.svg)](https://docs.rs/shm-primitives)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/shm-primitives.svg)](./LICENSE)

Lock-free primitives for shared memory IPC.

This crate provides `no_std`-compatible, lock-free data structures designed for use in shared memory contexts where you work with raw pointers to memory-mapped regions.

## Primitives

- **SpscRing** / **SpscRingRaw**: Single-producer single-consumer ring buffer with wait-free operations
- **TreiberSlab** / **TreiberSlabRaw**: Treiber stack-based slab allocator with generation counting for ABA protection

## Raw vs Region APIs

Each primitive has two variants:

- **Raw** (`SpscRingRaw`, `TreiberSlabRaw`): Work with raw pointers, suitable for shared memory where you have `*mut` pointers from mmap. Caller manages memory lifetime.

- **Region** (`SpscRing`, `TreiberSlab`): Convenience wrappers that own their backing memory via a `Region`. These delegate to the Raw implementations internally.

## Features

- `no_std` by default
- `alloc` - Enables `HeapRegion` for heap-backed testing
- `std` - Enables std (implies `alloc`)
- `loom` - Enables loom-based concurrency testing

## Loom Testing

All algorithms are tested under [loom](https://github.com/tokio-rs/loom) to verify correctness across all possible thread interleavings:

```bash
cargo test -p shm-primitives --features loom
```

## Example

```rust,ignore
use shm_primitives::{SpscRing, HeapRegion, PushResult};

// Create a ring buffer with capacity 16
let region = HeapRegion::new_zeroed(SpscRing::<u64>::required_size(16));
let ring = SpscRing::<u64>::init(region.region(), 16);

// Split into producer and consumer
let (mut producer, mut consumer) = ring.split();

// Push some values
assert!(matches!(producer.push(42), PushResult::Ok));
assert!(matches!(producer.push(43), PushResult::Ok));

// Pop them back
assert_eq!(consumer.pop(), Some(42));
assert_eq!(consumer.pop(), Some(43));
assert_eq!(consumer.pop(), None);
```

## License

MIT OR Apache-2.0
