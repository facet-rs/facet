//! Fuzzing harnesses for rapace SHM transport components.
//!
//! These fuzzers operate on in-memory replicas of the SHM structures,
//! without touching real mmap, to test invariants of the ring and slab algorithms.

pub mod ring_model;
pub mod session_model;
pub mod shm_integration;
pub mod slab_model;
