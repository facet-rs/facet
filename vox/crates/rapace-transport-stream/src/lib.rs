//! rapace-transport-stream: TCP/Unix socket transport for rapace.
//!
//! For cross-machine or cross-container communication.
//!
//! Characteristics:
//! - Length-prefixed frames: `[u32 length][frame bytes]`
//! - Everything is owned buffers (no zero-copy)
//! - Same RPC semantics as other transports

// TODO: implement stream transport
