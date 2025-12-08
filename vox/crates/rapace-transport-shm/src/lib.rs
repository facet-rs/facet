//! rapace-transport-shm: Shared memory transport for rapace.
//!
//! This is the **performance reference** implementation. It defines the
//! canonical memory layout and zero-copy patterns.
//!
//! Characteristics:
//! - SPSC rings for descriptors
//! - Slab allocator for payloads
//! - Zero-copy when data is already in SHM
//! - eventfd doorbells for async notification
//! - Generation counters for crash safety

// TODO: implement SHM transport
