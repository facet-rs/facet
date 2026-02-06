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

use std::io;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cobs::decode_vec as cobs_decode_vec;
use roam_wire::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use roam_session::MessageTransport;

/// Enable wire-level message logging for debugging.
/// Set ROAM_WIRE_SPY=1 to enable.
static WIRE_SPY_ENABLED: AtomicBool = AtomicBool::new(false);

static WIRE_SPY_INIT: OnceLock<()> = OnceLock::new();

fn wire_spy_enabled() -> bool {
    WIRE_SPY_INIT.get_or_init(|| {
        if std::env::var("ROAM_WIRE_SPY").is_ok() {
            WIRE_SPY_ENABLED.store(true, Ordering::Relaxed);
        }
    });

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
    /// Buffer for encoding messages to avoid reallocations.
    encode_buf: Vec<u8>,
}

impl<S> CobsFramed<S> {
    /// Create a new framed connection from an async stream.
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            last_decoded: Vec::new(),
            encode_buf: Vec::with_capacity(1024),
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

struct CobsWriter<'a> {
    out: &'a mut Vec<u8>,
    code_idx: usize,
    block_len: u8,
}

impl<'a> CobsWriter<'a> {
    fn new(out: &'a mut Vec<u8>) -> Self {
        let code_idx = out.len();
        out.push(0);
        Self {
            out,
            code_idx,
            block_len: 0,
        }
    }

    fn finalize(self) {
        self.out[self.code_idx] = self.block_len + 1;
    }
}

impl facet_postcard::Writer for CobsWriter<'_> {
    fn write_byte(&mut self, byte: u8) -> Result<(), facet_postcard::SerializeError> {
        if self.block_len == 254 {
            self.code_idx = self.out.len();
            self.out.push(0);
            self.block_len = 0;
        }

        if byte == 0 {
            self.out[self.code_idx] = self.block_len + 1;
            self.code_idx = self.out.len();
            self.out.push(0);
            self.block_len = 0;
        } else {
            self.out.push(byte);
            self.block_len += 1;
            if self.block_len == 254 {
                self.out[self.code_idx] = 0xFF;
            }
        }
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), facet_postcard::SerializeError> {
        for &b in bytes {
            self.write_byte(b)?;
        }
        Ok(())
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

        self.encode_buf.clear();
        let mut writer = CobsWriter::new(&mut self.encode_buf);
        facet_postcard::to_writer_fallible(msg, &mut writer)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        writer.finalize();
        self.encode_buf.push(0x00); // Delimiter

        wire_spy_bytes("-->", &self.encode_buf);
        self.stream.write_all(&self.encode_buf).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use cobs::encode_vec as cobs_encode_vec;
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, duplex};

    #[test]
    fn test_cobs_writer_matches_cobs_crate() {
        let mut input_254_zero = vec![0x11; 254];
        input_254_zero.push(0x00);

        let mut input_254_11 = vec![0x11; 254];
        input_254_11.push(0x11);

        let cases = vec![
            vec![],
            vec![0x00],
            vec![0x11, 0x00, 0x22],
            vec![0x11; 253],
            vec![0x11; 254],
            vec![0x11; 255],
            input_254_zero,
            input_254_11,
            vec![0x00; 10],
            (0..500).map(|i| (i % 256) as u8).collect::<Vec<_>>(),
        ];

        for input in cases {
            let mut out = Vec::new();
            let mut writer = CobsWriter::new(&mut out);
            for &b in &input {
                facet_postcard::Writer::write_byte(&mut writer, b).unwrap();
            }
            writer.finalize();

            let expected = cobs_encode_vec(&input);
            assert_eq!(out, expected, "Failed for input of length {}", input.len());
        }
    }

    #[tokio::test]
    async fn recv_invalid_postcard_payload_returns_invalid_data() {
        // Minimized from AFL crash corpus.
        let input = [
            0x17, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x30, 0x30, 0xfd,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x05, 0x05, 0x30, 0x30, 0x30, 0x30,
            0x00,
        ];

        let (mut writer, reader) = duplex(input.len() + 1);
        writer.write_all(&input).await.unwrap();
        writer.shutdown().await.unwrap();

        let mut framed = CobsFramed::new(reader);
        let err = framed.recv().await.expect_err("expected invalid data");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn roundtrip_large_response_payload() {
        let payload: Vec<u8> = (0..(32 * 1024))
            .map(|i| (i as u8).wrapping_mul(31))
            .collect();
        let msg = Message::Response {
            conn_id: roam_wire::ConnectionId::ROOT,
            request_id: 42,
            metadata: vec![],
            channels: vec![],
            payload,
        };

        let (left, right) = duplex(256 * 1024);
        let mut sender = CobsFramed::new(left);
        let mut receiver = CobsFramed::new(right);

        sender.send(&msg).await.unwrap();
        let decoded = receiver.recv().await.unwrap().expect("expected frame");
        assert_eq!(decoded, msg);
    }

    #[tokio::test]
    async fn recv_does_not_spin_on_delimiter_heavy_invalid_input() {
        let mut input = Vec::with_capacity(64 * 1024);
        for i in 0..(64 * 1024) {
            if i % 2 == 0 {
                input.push(0x00);
            } else {
                input.push(0xff);
            }
        }

        let (mut writer, reader) = duplex(input.len() + 1);
        writer.write_all(&input).await.unwrap();
        writer.shutdown().await.unwrap();

        let mut framed = CobsFramed::new(reader);
        let completed = tokio::time::timeout(Duration::from_millis(250), framed.recv())
            .await
            .expect("recv should complete without spinning");

        match completed {
            Ok(None) | Err(_) => {}
            Ok(Some(msg)) => panic!("unexpectedly decoded message: {msg:?}"),
        }
    }
}
