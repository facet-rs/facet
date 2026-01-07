#![deny(unsafe_code)]

//! Spec-level wire types.
//!
//! Canonical definitions live in `docs/content/spec/_index.md` and `docs/content/shm-spec/_index.md`.

use facet::Facet;

/// Hello message for handshake.
// r[impl message.hello.structure]
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum Hello {
    /// Spec v1 Hello.
    V1 {
        max_payload_size: u32,
        initial_stream_credit: u32,
    } = 0,
}

/// Metadata value.
// r[impl unary.metadata.type]
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum MetadataValue {
    String(String) = 0,
    Bytes(Vec<u8>) = 1,
    U64(u64) = 2,
}

/// Protocol message.
///
/// Variant order is wire-significant (postcard enum discriminants).
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum Message {
    // Control
    Hello(Hello) = 0,
    Goodbye {
        reason: String,
    } = 1,

    // RPC
    /// r[impl core.metadata] - Request carries metadata key-value pairs.
    /// r[impl unary.metadata.unknown] - Unknown keys are ignored.
    Request {
        request_id: u64,
        method_id: u64,
        metadata: Vec<(String, MetadataValue)>,
        payload: Vec<u8>,
    } = 2,
    /// r[impl core.metadata] - Response carries metadata key-value pairs.
    /// r[impl unary.metadata.unknown] - Unknown keys are ignored.
    Response {
        request_id: u64,
        metadata: Vec<(String, MetadataValue)>,
        payload: Vec<u8>,
    } = 3,
    /// r[impl unary.cancel.message] - Cancel message requests callee stop processing.
    /// r[impl unary.cancel.no-response-required] - Caller should timeout, not wait indefinitely.
    Cancel {
        request_id: u64,
    } = 4,

    // Streams
    // rs[impl wire.stream] - Stream<T> encoded as u64 stream ID on wire
    Data {
        stream_id: u64,
        payload: Vec<u8>,
    } = 5,
    Close {
        stream_id: u64,
    } = 6,
    Reset {
        stream_id: u64,
    } = 7,
    Credit {
        stream_id: u64,
        bytes: u32,
    } = 8,
}
