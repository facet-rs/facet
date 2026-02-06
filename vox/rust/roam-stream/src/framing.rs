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
static WIRE_SPY_BYTES_ONLY: AtomicBool = AtomicBool::new(false);

static WIRE_SPY_INIT: OnceLock<()> = OnceLock::new();

fn wire_spy_enabled() -> bool {
    WIRE_SPY_INIT.get_or_init(|| {
        if std::env::var("ROAM_WIRE_SPY").is_ok() {
            WIRE_SPY_ENABLED.store(true, Ordering::Relaxed);
        }
        if std::env::var("ROAM_WIRE_SPY_BYTES_ONLY").is_ok() {
            WIRE_SPY_BYTES_ONLY.store(true, Ordering::Relaxed);
        }
    });

    WIRE_SPY_ENABLED.load(Ordering::Relaxed)
}

fn wire_spy_log(direction: &str, msg: &Message) {
    if wire_spy_enabled() && !WIRE_SPY_BYTES_ONLY.load(Ordering::Relaxed) {
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

const RECV_BUF_COMPACT_THRESHOLD: usize = 64 * 1024;

fn compact_recv_buffer(buf: &mut Vec<u8>, unread_start: &mut usize, scan_from: &mut usize) {
    if *unread_start == buf.len() {
        buf.clear();
        *unread_start = 0;
        *scan_from = 0;
        return;
    }

    if *unread_start >= RECV_BUF_COMPACT_THRESHOLD && *unread_start >= buf.len() / 2 {
        buf.drain(..*unread_start);
        *scan_from = scan_from.saturating_sub(*unread_start);
        *unread_start = 0;
    }
}

fn advance_past_frame(
    buf: &mut Vec<u8>,
    unread_start: &mut usize,
    scan_from: &mut usize,
    frame_end: usize,
) {
    *unread_start = frame_end + 1;
    *scan_from = *unread_start;
    compact_recv_buffer(buf, unread_start, scan_from);
}

fn try_decode_one_from_buffer(
    buf: &mut Vec<u8>,
    unread_start: &mut usize,
    scan_from: &mut usize,
    last_decoded: &mut Vec<u8>,
) -> io::Result<Option<Message>> {
    if *scan_from < *unread_start {
        *scan_from = *unread_start;
    }
    if *scan_from > buf.len() {
        *scan_from = buf.len();
    }

    let Some(rel_idx) = buf[*scan_from..].iter().position(|b| *b == 0x00) else {
        *scan_from = buf.len();
        return Ok(None);
    };

    let frame_end = *scan_from + rel_idx;
    let frame = &buf[*unread_start..frame_end];

    wire_spy_bytes("<-- frame", frame);

    let decoded = match cobs_decode_vec(frame) {
        Ok(decoded) => decoded,
        Err(e) => {
            last_decoded.clear();
            advance_past_frame(buf, unread_start, scan_from, frame_end);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("cobs: {e}"),
            ));
        }
    };

    wire_spy_bytes("<-- decoded", &decoded);

    let msg: Message = match facet_postcard::from_slice(&decoded) {
        Ok(msg) => msg,
        Err(e) => {
            *last_decoded = decoded;
            advance_past_frame(buf, unread_start, scan_from, frame_end);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("postcard: {e}"),
            ));
        }
    };

    *last_decoded = decoded;
    wire_spy_log("<--", &msg);
    advance_past_frame(buf, unread_start, scan_from, frame_end);
    Ok(Some(msg))
}

#[cfg(feature = "fuzzing")]
pub fn try_decode_one_from_buffer_for_fuzz(
    buf: &mut Vec<u8>,
    unread_start: &mut usize,
    scan_from: &mut usize,
    last_decoded: &mut Vec<u8>,
) -> io::Result<Option<Message>> {
    try_decode_one_from_buffer(buf, unread_start, scan_from, last_decoded)
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
    unread_start: usize,
    scan_from: usize,
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
            unread_start: 0,
            scan_from: 0,
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
            if let Some(msg) = try_decode_one_from_buffer(
                &mut self.buf,
                &mut self.unread_start,
                &mut self.scan_from,
                &mut self.last_decoded,
            )? {
                return Ok(Some(msg));
            }

            // Read more data
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await?;
            if n == 0 {
                let trailing = self.buf.len().saturating_sub(self.unread_start);
                if wire_spy_enabled() {
                    eprintln!("[WIRE] <-- EOF (read 0 bytes)");
                }
                if trailing != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        format!("eof with {trailing} trailing bytes and no frame delimiter"),
                    ));
                }
                return Ok(None);
            }
            if wire_spy_enabled() {
                eprintln!("[WIRE] <-- read {} bytes: {:02x?}", n, &tmp[..n.min(64)]);
            }
            compact_recv_buffer(&mut self.buf, &mut self.unread_start, &mut self.scan_from);
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
    use std::process::Command;
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

    #[test]
    fn compact_recv_buffer_compacts_large_consumed_prefix() {
        let mut buf = vec![0xaa; RECV_BUF_COMPACT_THRESHOLD + 32];
        let mut unread_start = RECV_BUF_COMPACT_THRESHOLD;
        let mut scan_from = RECV_BUF_COMPACT_THRESHOLD + 7;

        compact_recv_buffer(&mut buf, &mut unread_start, &mut scan_from);

        assert_eq!(unread_start, 0);
        assert_eq!(scan_from, 7);
        assert_eq!(buf, vec![0xaa; 32]);
    }

    #[test]
    fn try_decode_normalizes_scan_bounds() {
        // scan_from > len should clamp to len
        let mut buf = Vec::new();
        let mut unread_start = 0usize;
        let mut scan_from = 123usize;
        let mut last_decoded = Vec::new();
        let decoded = try_decode_one_from_buffer(
            &mut buf,
            &mut unread_start,
            &mut scan_from,
            &mut last_decoded,
        )
        .unwrap();
        assert!(decoded.is_none());
        assert_eq!(scan_from, 0);

        // scan_from < unread_start should move scan start forward before searching.
        let mut buf = vec![0x42, 0x00];
        let mut unread_start = 1usize;
        let mut scan_from = 0usize;
        let mut last_decoded = vec![1, 2, 3];
        let err = try_decode_one_from_buffer(
            &mut buf,
            &mut unread_start,
            &mut scan_from,
            &mut last_decoded,
        )
        .expect_err("empty frame should fail postcard decode");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(last_decoded.is_empty());
    }

    #[test]
    fn accessors_roundtrip_underlying_stream() {
        let (left, _right) = duplex(1024);
        let mut framed = CobsFramed::new(left);
        let _ = framed.stream();
        let _ = framed.stream_mut();
        let _inner = framed.into_inner();
    }

    #[test]
    fn wire_spy_env_init_and_logging_paths() {
        const SUBPROCESS_KEY: &str = "ROAM_WIRE_SPY_TEST_SUBPROCESS";
        if std::env::var_os(SUBPROCESS_KEY).is_some() {
            assert!(wire_spy_enabled());
            assert!(WIRE_SPY_ENABLED.load(Ordering::Relaxed));
            assert!(WIRE_SPY_BYTES_ONLY.load(Ordering::Relaxed));

            // Exercise both logging branches.
            WIRE_SPY_BYTES_ONLY.store(false, Ordering::Relaxed);
            wire_spy_log(
                "test",
                &Message::Cancel {
                    conn_id: roam_wire::ConnectionId::ROOT,
                    request_id: 1,
                },
            );
            WIRE_SPY_BYTES_ONLY.store(true, Ordering::Relaxed);
            wire_spy_bytes("test", &[1, 2, 3, 4]);
            return;
        }

        let status = Command::new(std::env::current_exe().expect("test binary path"))
            .arg("--exact")
            .arg("framing::tests::wire_spy_env_init_and_logging_paths")
            .arg("--nocapture")
            .env(SUBPROCESS_KEY, "1")
            .env("ROAM_WIRE_SPY", "1")
            .env("ROAM_WIRE_SPY_BYTES_ONLY", "1")
            .status()
            .expect("spawn subprocess");
        assert!(status.success(), "wire spy subprocess test failed");
    }

    #[tokio::test]
    async fn recv_reports_unexpected_eof_for_partial_frame() {
        WIRE_SPY_INIT.get_or_init(|| {});
        WIRE_SPY_ENABLED.store(true, Ordering::Relaxed);

        let input = [0x11, 0x22, 0x33, 0x44];
        let (mut writer, reader) = duplex(input.len() + 1);
        writer.write_all(&input).await.unwrap();
        writer.shutdown().await.unwrap();

        let mut framed = CobsFramed::new(reader);
        let err = framed
            .recv()
            .await
            .expect_err("expected EOF for partial frame");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert!(err.to_string().contains("trailing bytes"));
    }

    #[tokio::test]
    async fn message_transport_last_decoded_tracks_last_payload() {
        let msg = Message::Response {
            conn_id: roam_wire::ConnectionId::ROOT,
            request_id: 7,
            metadata: vec![],
            channels: vec![],
            payload: vec![1, 2, 3, 4],
        };
        let postcard = facet_postcard::to_vec(&msg).unwrap();
        let mut frame = cobs_encode_vec(&postcard);
        frame.push(0x00);

        let (mut writer, reader) = duplex(frame.len() + 1);
        writer.write_all(&frame).await.unwrap();
        writer.shutdown().await.unwrap();

        let mut framed = CobsFramed::new(reader);
        let decoded = framed
            .recv()
            .await
            .unwrap()
            .expect("expected decoded frame");
        assert_eq!(decoded, msg);
        assert_eq!(MessageTransport::last_decoded(&framed), postcard.as_slice());
    }
}
