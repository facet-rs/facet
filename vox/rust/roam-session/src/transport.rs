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

// ============================================================================
// Diagnostic transport wrapper
// ============================================================================

use crate::diagnostic::DiagnosticState;
use std::sync::Arc;

/// A transport wrapper that records frame-level statistics into a [`DiagnosticState`].
///
/// Wraps any `MessageTransport` and transparently records:
/// - Frame counts (sent/received)
/// - Byte counts (payload bytes sent/received)
/// - Timestamps of last frame sent/received
///
/// When `diag` is `None`, this is a zero-overhead passthrough.
pub struct DiagnosticTransport<T> {
    inner: T,
    diag: Option<Arc<DiagnosticState>>,
}

impl<T> DiagnosticTransport<T> {
    /// Wrap a transport with optional diagnostic recording.
    pub fn new(inner: T, diag: Option<Arc<DiagnosticState>>) -> Self {
        Self { inner, diag }
    }

    /// Get a reference to the inner transport.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    fn record_sent(&self, msg: &Message) {
        if let Some(ref diag) = self.diag {
            diag.record_frame_sent(estimate_message_size(msg));
        }
    }

    fn record_received(&self, msg: &Message) {
        if let Some(ref diag) = self.diag {
            diag.record_frame_received(estimate_message_size(msg));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: MessageTransport> MessageTransport for DiagnosticTransport<T> {
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        self.inner.send(msg).await?;
        self.record_sent(msg);
        Ok(())
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        let result = self.inner.recv_timeout(timeout).await?;
        if let Some(ref msg) = result {
            self.record_received(msg);
        }
        Ok(result)
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        let result = self.inner.recv().await?;
        if let Some(ref msg) = result {
            self.record_received(msg);
        }
        Ok(result)
    }

    fn last_decoded(&self) -> &[u8] {
        self.inner.last_decoded()
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: MessageTransport> MessageTransport for DiagnosticTransport<T> {
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        self.inner.send(msg).await?;
        self.record_sent(msg);
        Ok(())
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        let result = self.inner.recv_timeout(timeout).await?;
        if let Some(ref msg) = result {
            self.record_received(msg);
        }
        Ok(result)
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        let result = self.inner.recv().await?;
        if let Some(ref msg) = result {
            self.record_received(msg);
        }
        Ok(result)
    }

    fn last_decoded(&self) -> &[u8] {
        self.inner.last_decoded()
    }
}

/// Estimate the size of a message's payload for diagnostic byte counting.
fn estimate_message_size(msg: &Message) -> usize {
    match msg {
        Message::Hello(_) => 64, // rough estimate
        Message::Request {
            payload,
            metadata,
            channels,
            ..
        } => payload.len() + metadata.len() * 32 + channels.len() * 8,
        Message::Response {
            payload,
            metadata,
            channels,
            ..
        } => payload.len() + metadata.len() * 32 + channels.len() * 8,
        Message::Data { payload, .. } => payload.len(),
        Message::Close { .. } => 16,
        Message::Reset { .. } => 16,
        Message::Credit { .. } => 16,
        Message::Cancel { .. } => 16,
        Message::Goodbye { reason, .. } => reason.len() + 16,
        Message::Connect { metadata, .. } => metadata.len() * 32 + 16,
        Message::Accept { metadata, .. } => metadata.len() * 32 + 16,
        Message::Reject {
            reason, metadata, ..
        } => reason.len() + metadata.len() * 32 + 16,
    }
}
