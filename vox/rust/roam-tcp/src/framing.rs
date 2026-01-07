//! COBS framing for TCP streams.
//!
//! r[impl transport.bytestream.cobs] - Messages are COBS-encoded with 0x00 delimiter.

use std::io;

use cobs::{decode_vec as cobs_decode_vec, encode_vec as cobs_encode_vec};
use roam_wire::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// A COBS-framed TCP connection.
///
/// Handles encoding/decoding of roam messages over a TCP stream using
/// COBS (Consistent Overhead Byte Stuffing) framing with 0x00 delimiters.
pub struct CobsFramed {
    stream: TcpStream,
    buf: Vec<u8>,
    /// Last successfully decoded frame bytes (for error recovery/debugging).
    pub last_decoded: Vec<u8>,
}

impl CobsFramed {
    /// Create a new framed connection from a TCP stream.
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            last_decoded: Vec::new(),
        }
    }

    /// Get a reference to the underlying TCP stream.
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }

    /// Get a mutable reference to the underlying TCP stream.
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    /// Send a message over the connection.
    ///
    /// r[impl transport.bytestream.cobs] - COBS encode with 0x00 delimiter.
    pub async fn send(&mut self, msg: &Message) -> io::Result<()> {
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let mut framed = cobs_encode_vec(&payload);
        framed.push(0x00);
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

                // r[impl transport.bytestream.cobs] - decode COBS-encoded frame
                let decoded = cobs_decode_vec(&frame).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("cobs: {e}"))
                })?;
                self.last_decoded = decoded.clone();

                let msg: Message = facet_postcard::from_slice(&decoded).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("postcard: {e}"))
                })?;
                return Ok(Some(msg));
            }

            // Read more data
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await?;
            if n == 0 {
                return Ok(None);
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }
}
