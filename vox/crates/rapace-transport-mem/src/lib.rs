//! rapace-transport-mem: In-process transport for rapace.
//!
//! This is the **semantic reference** implementation. All other transports
//! must behave identically to this one. If behavior differs, the other
//! transport has a bug.
//!
//! Characteristics:
//! - No serialization for direct calls
//! - Real Rust lifetimes (`&[u8]` is a real borrow)
//! - Still participates in RPC semantics (channels, deadlines, cancellation)

// TODO: implement in-proc transport
