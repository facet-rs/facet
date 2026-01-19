//! COBS framing for async streams.
//!
//! r[impl transport.bytestream.cobs] - Messages are COBS-encoded with 0x00 delimiter.
//! r[impl transport.message.binary] - All messages are binary (not text).
//! r[impl transport.message.one-to-one] - Each frame contains exactly one roam message.
//! r[impl transport.message.multiplexing] - channel_id field provides multiplexing.
//!
//! This module is generic over the transport type - it works with any type that
//! implements `AsyncRead + AsyncWrite + Unpin`, including:
//! - `TcpStream` (TCP sockets)
//! - `UnixStream` (Unix domain sockets)
//! - Any other async byte stream
//!
//! TODO: Currently we do facet_postcard::to_vec() then cobs_encode_vec() - two allocations
//! and two passes over the data. Should switch to a streaming encoder that does a single pass.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cobs::{decode_vec as cobs_decode_vec, encode_vec as cobs_encode_vec};
use roam_wire::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use roam_session::MessageTransport;

/// Enable wire-level message logging for debugging.
/// Set ROAM_WIRE_SPY=1 to enable.
static WIRE_SPY_ENABLED: AtomicBool = AtomicBool::new(false);

#[ctor::ctor]
fn init_wire_spy() {
    if std::env::var("ROAM_WIRE_SPY").is_ok() {
        WIRE_SPY_ENABLED.store(true, Ordering::Relaxed);
    }
}

fn wire_spy_enabled() -> bool {
    WIRE_SPY_ENABLED.load(Ordering::Relaxed)
}

fn wire_spy_log(direction: &str, msg: &Message) {
    if wire_spy_enabled() {
        eprintln!("[WIRE] {direction} {msg:?}");
    }
}

fn wire_spy_bytes(direction: &str, bytes: &[u8]) {
    if wire_spy_enabled() {
        eprintln!(
            "[WIRE] {direction} {} bytes: {:02x?}",
            bytes.len(),
            &bytes[..bytes.len().min(64)]
        );
    }
}

/// A COBS-framed async stream connection.
///
/// Handles encoding/decoding of roam messages over any async byte stream using
/// COBS (Consistent Overhead Byte Stuffing) framing with 0x00 delimiters.
///
/// Generic over the transport type `S` which must implement `AsyncRead + AsyncWrite + Unpin`.
/// This allows the same framing logic to work with TCP sockets, Unix domain sockets,
/// or any other async byte stream.
pub struct CobsFramed<S> {
    stream: S,
    buf: Vec<u8>,
    /// Last successfully decoded frame bytes (for error recovery/debugging).
    pub last_decoded: Vec<u8>,
}

impl<S> CobsFramed<S> {
    /// Create a new framed connection from an async stream.
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            last_decoded: Vec::new(),
        }
    }

    /// Get a reference to the underlying stream.
    pub fn stream(&self) -> &S {
        &self.stream
    }

    /// Get a mutable reference to the underlying stream.
    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Consume the framed wrapper and return the underlying stream.
    pub fn into_inner(self) -> S {
        self.stream
    }
}

impl<S> CobsFramed<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Send a message over the connection.
    ///
    /// r[impl transport.bytestream.cobs] - COBS encode with 0x00 delimiter.
    pub async fn send(&mut self, msg: &Message) -> io::Result<()> {
        wire_spy_log("-->", msg);
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let mut framed = cobs_encode_vec(&payload);
        framed.push(0x00);
        wire_spy_bytes("-->", &framed);
        self.stream.write_all(&framed).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Receive a message with a timeout.
    ///
    /// Returns `Ok(None)` if no message received within timeout or connection closed.
    pub async fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> io::Result<Option<Message>> {
        tokio::time::timeout(timeout, self.recv_inner())
            .await
            .unwrap_or(Ok(None))
    }

    /// Receive a message (blocking until one arrives or connection closes).
    pub async fn recv(&mut self) -> io::Result<Option<Message>> {
        self.recv_inner().await
    }

    async fn recv_inner(&mut self) -> io::Result<Option<Message>> {
        loop {
            // Look for frame delimiter
            if let Some(idx) = self.buf.iter().position(|b| *b == 0x00) {
                let frame = self.buf.drain(..idx).collect::<Vec<_>>();
                self.buf.drain(..1); // Remove delimiter

                wire_spy_bytes("<-- frame", &frame);

                // r[impl transport.bytestream.cobs] - decode COBS-encoded frame
                let decoded = cobs_decode_vec(&frame).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("cobs: {e}"))
                })?;
                self.last_decoded = decoded.clone();

                wire_spy_bytes("<-- decoded", &decoded);

                let msg: Message = facet_postcard::from_slice(&decoded).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("postcard: {e}"))
                })?;
                wire_spy_log("<--", &msg);
                return Ok(Some(msg));
            }

            // Read more data
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await?;
            if n == 0 {
                if wire_spy_enabled() {
                    eprintln!("[WIRE] <-- EOF (read 0 bytes)");
                }
                return Ok(None);
            }
            if wire_spy_enabled() {
                eprintln!("[WIRE] <-- read {} bytes: {:02x?}", n, &tmp[..n.min(64)]);
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }
}

impl<S> MessageTransport for CobsFramed<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        CobsFramed::send(self, msg).await
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        CobsFramed::recv_timeout(self, timeout).await
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        CobsFramed::recv(self).await
    }

    fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}
