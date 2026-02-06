//! Message transport abstraction.
//!
//! This module defines the [`MessageTransport`] trait that abstracts over different
//! transport mechanisms for sending and receiving roam messages.
//!
//! Implementations:
//! - `LengthPrefixedFramed` from `roam-stream` for byte streams (TCP, Unix sockets) - native only
//! - `WsTransport` from `roam-websocket` for WebSocket (native and WASM)

use std::io;
use std::time::Duration;

use roam_wire::Message;

/// Trait for transports that can send and receive roam messages.
///
/// This abstracts over the framing mechanism:
/// - Byte streams need length-prefixed framing to delimit messages
/// - Message-oriented transports (WebSocket) have built-in framing
///
/// Both cases share the same protocol logic in the Driver.
///
/// # Platform-specific Send bounds
///
/// On native (tokio), the trait and its async methods require `Send` for
/// multi-threaded executors. On WASM, everything is single-threaded so
/// `Send` bounds are not required.
#[cfg(not(target_arch = "wasm32"))]
pub trait MessageTransport: Send {
    /// Send a message over the transport.
    fn send(&mut self, msg: &Message) -> impl std::future::Future<Output = io::Result<()>> + Send;

    /// Receive a message with a timeout.
    ///
    /// Returns `Ok(None)` if:
    /// - Timeout expires
    /// - Connection is closed cleanly
    fn recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> impl std::future::Future<Output = io::Result<Option<Message>>> + Send;

    /// Receive a message (blocking until one arrives or connection closes).
    fn recv(&mut self) -> impl std::future::Future<Output = io::Result<Option<Message>>> + Send;

    /// Get the last decoded bytes (for error detection).
    ///
    /// Used to detect specific error conditions like unknown message variants.
    fn last_decoded(&self) -> &[u8];
}

/// Trait for transports that can send and receive roam messages (WASM version).
///
/// On WASM, `Send` bounds are not required since everything is single-threaded.
#[cfg(target_arch = "wasm32")]
pub trait MessageTransport {
    /// Send a message over the transport.
    fn send(&mut self, msg: &Message) -> impl std::future::Future<Output = io::Result<()>>;

    /// Receive a message with a timeout.
    ///
    /// Returns `Ok(None)` if:
    /// - Timeout expires
    /// - Connection is closed cleanly
    fn recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> impl std::future::Future<Output = io::Result<Option<Message>>>;

    /// Receive a message (blocking until one arrives or connection closes).
    fn recv(&mut self) -> impl std::future::Future<Output = io::Result<Option<Message>>>;

    /// Get the last decoded bytes (for error detection).
    ///
    /// Used to detect specific error conditions like unknown message variants.
    fn last_decoded(&self) -> &[u8];
}
