//! Message transport abstraction.
//!
//! This module defines the [`MessageTransport`] trait that abstracts over different
//! transport mechanisms for sending and receiving roam messages.
//!
//! Implementations:
//! - [`CobsFramed`](crate::CobsFramed) for byte streams (TCP, Unix sockets)
//! - `WsTransport` in `roam-websocket` crate for WebSocket

use std::io;
use std::time::Duration;

use roam_wire::Message;

/// Trait for transports that can send and receive roam messages.
///
/// This abstracts over the framing mechanism:
/// - Byte streams need COBS framing to delimit messages
/// - Message-oriented transports (WebSocket) have built-in framing
///
/// Both cases share the same protocol logic in [`Driver`](crate::Driver).
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
