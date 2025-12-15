//! rapace-transport-stream: TCP/Unix socket transport for rapace.
//!
//! For cross-machine or cross-container communication.
//!
//! # Wire Format
//!
//! Each frame is sent as:
//! - `u32 LE`: total frame length (64 + payload_len)
//! - `[u8; 64]`: MsgDescHot as raw bytes (repr(C), POD)
//! - `[u8; payload_len]`: payload bytes
//!
//! # Characteristics
//!
//! - Length-prefixed frames for easy framing
//! - Everything is owned buffers (no zero-copy on receive)
//! - Full-duplex: send and receive can happen concurrently
//! - Same RPC semantics as other transports

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rapace_core::{
    DecodeError, EncodeCtx, EncodeError, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT,
    MsgDescHot, RecvFrame, Transport, TransportError,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::Mutex as AsyncMutex;

/// Size of MsgDescHot in bytes (must be 64).
const DESC_SIZE: usize = 64;

// Compile-time check that MsgDescHot is exactly 64 bytes
const _: () = assert!(std::mem::size_of::<MsgDescHot>() == DESC_SIZE);

/// Stream-based transport implementation.
///
/// Works with any `AsyncRead + AsyncWrite` stream (TCP, Unix socket, duplex, etc.).
/// Uses split read/write halves for full-duplex operation.
pub struct StreamTransport<R, W> {
    inner: Arc<StreamInner<R, W>>,
}

struct StreamInner<R, W> {
    /// Read half of the stream (async mutex for holding across awaits).
    reader: AsyncMutex<R>,
    /// Write half of the stream (async mutex for holding across awaits).
    writer: AsyncMutex<W>,
    /// Whether the transport is closed.
    closed: AtomicBool,
}

impl<S> StreamTransport<ReadHalf<S>, WriteHalf<S>>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    /// Create a new stream transport by splitting the given stream.
    ///
    /// The stream is split into read and write halves, allowing concurrent
    /// send and receive operations.
    pub fn new(stream: S) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            inner: Arc::new(StreamInner {
                reader: AsyncMutex::new(reader),
                writer: AsyncMutex::new(writer),
                closed: AtomicBool::new(false),
            }),
        }
    }
}

impl StreamTransport<ReadHalf<tokio::io::DuplexStream>, WriteHalf<tokio::io::DuplexStream>> {
    /// Create a connected pair of stream transports for testing.
    ///
    /// Uses `tokio::io::duplex` internally.
    pub fn pair() -> (Self, Self) {
        // 64KB buffer should be plenty for testing
        let (a, b) = tokio::io::duplex(65536);
        (Self::new(a), Self::new(b))
    }
}

/// Convert MsgDescHot to raw bytes.
///
/// # Safety
///
/// MsgDescHot is `#[repr(C, align(64))]` and contains only POD types
/// (integers, bitflags which is a u32, and a byte array). This makes
/// it safe to transmute to/from bytes on the same platform.
///
/// Note: This is NOT portable across platforms with different endianness
/// or struct padding. For cross-platform wire format, use explicit
/// field serialization instead.
fn desc_to_bytes(desc: &MsgDescHot) -> [u8; DESC_SIZE] {
    // SAFETY: MsgDescHot is repr(C), Copy, and exactly 64 bytes.
    // All fields are primitive types with well-defined layout.
    unsafe { std::mem::transmute_copy(desc) }
}

/// Convert raw bytes to MsgDescHot.
///
/// # Safety
///
/// See `desc_to_bytes` for safety discussion.
fn bytes_to_desc(bytes: &[u8; DESC_SIZE]) -> MsgDescHot {
    // SAFETY: Same as desc_to_bytes - MsgDescHot is repr(C), Copy, 64 bytes.
    unsafe { std::mem::transmute_copy(bytes) }
}

impl<R, W> Transport for StreamTransport<R, W>
where
    R: AsyncRead + Unpin + Send + Sync + 'static,
    W: AsyncWrite + Unpin + Send + Sync + 'static,
{
    /// Payload type for received frames.
    /// For now we use `Vec<u8>`; Phase 2 will introduce pooled buffers.
    type RecvPayload = Vec<u8>;

    async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let payload = frame.payload();
        let frame_len = DESC_SIZE + payload.len();

        // Serialize descriptor
        let desc_bytes = desc_to_bytes(&frame.desc);

        // Write: length prefix + descriptor + payload
        let mut writer = self.inner.writer.lock().await;

        // Length prefix (u32 LE)
        writer
            .write_all(&(frame_len as u32).to_le_bytes())
            .await
            .map_err(TransportError::Io)?;

        // Descriptor (64 bytes)
        writer
            .write_all(&desc_bytes)
            .await
            .map_err(TransportError::Io)?;

        // Payload
        if !payload.is_empty() {
            writer
                .write_all(payload)
                .await
                .map_err(TransportError::Io)?;
        }

        // Flush to ensure frame is sent
        writer.flush().await.map_err(TransportError::Io)?;

        Ok(())
    }

    async fn recv_frame(&self) -> Result<RecvFrame<Self::RecvPayload>, TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let mut reader = self.inner.reader.lock().await;

        // Read length prefix
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                TransportError::Closed
            } else {
                TransportError::Io(e)
            }
        })?;
        let frame_len = u32::from_le_bytes(len_buf) as usize;

        // Validate frame length
        if frame_len < DESC_SIZE {
            return Err(TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too small: {} < {}", frame_len, DESC_SIZE),
            )));
        }

        // Read descriptor
        let mut desc_buf = [0u8; DESC_SIZE];
        reader
            .read_exact(&mut desc_buf)
            .await
            .map_err(TransportError::Io)?;

        let mut desc = bytes_to_desc(&desc_buf);

        // Read payload
        let payload_len = frame_len - DESC_SIZE;
        let payload = if payload_len > 0 {
            let mut buf = vec![0u8; payload_len];
            reader
                .read_exact(&mut buf)
                .await
                .map_err(TransportError::Io)?;
            buf
        } else {
            Vec::new()
        };

        // Update desc.payload_len to match actual received payload
        desc.payload_len = payload_len as u32;

        // If payload fits inline, mark it as inline
        if payload_len <= INLINE_PAYLOAD_SIZE {
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.inline_payload[..payload_len].copy_from_slice(&payload);
            Ok(RecvFrame::inline(desc))
        } else {
            // Mark as external payload
            desc.payload_slot = 0;
            Ok(RecvFrame::with_payload(desc, payload))
        }
    }

    fn encoder(&self) -> Box<dyn EncodeCtx + '_> {
        Box::new(StreamEncoder::new())
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.inner.closed.store(true, Ordering::Release);
        Ok(())
    }
}

impl<R, W> StreamTransport<R, W> {
    /// Check if the transport is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }
}

/// Encoder for stream transport.
///
/// Simply accumulates bytes into a Vec.
pub struct StreamEncoder {
    desc: MsgDescHot,
    payload: Vec<u8>,
}

impl StreamEncoder {
    fn new() -> Self {
        Self {
            desc: MsgDescHot::new(),
            payload: Vec::new(),
        }
    }

    /// Set the descriptor for this frame.
    pub fn set_desc(&mut self, desc: MsgDescHot) {
        self.desc = desc;
    }
}

impl EncodeCtx for StreamEncoder {
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.payload.extend_from_slice(bytes);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Frame, EncodeError> {
        Ok(Frame::with_payload(self.desc, self.payload))
    }
}

/// Decoder for stream transport.
pub struct StreamDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> StreamDecoder<'a> {
    /// Create a new decoder from a byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
}

impl<'a> rapace_core::DecodeCtx<'a> for StreamDecoder<'a> {
    fn decode_bytes(&mut self) -> Result<&'a [u8], DecodeError> {
        let result = &self.data[self.pos..];
        self.pos = self.data.len();
        Ok(result)
    }

    fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapace_core::FrameFlags;

    #[tokio::test]
    async fn test_pair_creation() {
        let (a, b) = StreamTransport::pair();
        assert!(!a.is_closed());
        assert!(!b.is_closed());
    }

    #[tokio::test]
    async fn test_send_recv_inline() {
        let (a, b) = StreamTransport::pair();

        // Create a frame with inline payload
        let mut desc = MsgDescHot::new();
        desc.msg_id = 1;
        desc.channel_id = 1;
        desc.method_id = 42;
        desc.flags = FrameFlags::DATA;

        let frame = Frame::with_inline_payload(desc, b"hello").unwrap();

        // Send from A
        a.send_frame(&frame).await.unwrap();

        // Receive on B
        let recv = b.recv_frame().await.unwrap();
        assert_eq!(recv.desc.msg_id, 1);
        assert_eq!(recv.desc.channel_id, 1);
        assert_eq!(recv.desc.method_id, 42);
        assert_eq!(recv.payload_bytes(), b"hello");
    }

    #[tokio::test]
    async fn test_send_recv_external_payload() {
        let (a, b) = StreamTransport::pair();

        let mut desc = MsgDescHot::new();
        desc.msg_id = 2;
        desc.flags = FrameFlags::DATA;

        let payload = vec![0u8; 1000]; // Larger than inline
        let frame = Frame::with_payload(desc, payload.clone());

        a.send_frame(&frame).await.unwrap();

        let recv = b.recv_frame().await.unwrap();
        assert_eq!(recv.desc.msg_id, 2);
        assert_eq!(recv.payload_bytes().len(), 1000);
    }

    #[tokio::test]
    async fn test_bidirectional() {
        let (a, b) = StreamTransport::pair();

        // A -> B
        let mut desc_a = MsgDescHot::new();
        desc_a.msg_id = 1;
        let frame_a = Frame::with_inline_payload(desc_a, b"from A").unwrap();
        a.send_frame(&frame_a).await.unwrap();

        // B -> A
        let mut desc_b = MsgDescHot::new();
        desc_b.msg_id = 2;
        let frame_b = Frame::with_inline_payload(desc_b, b"from B").unwrap();
        b.send_frame(&frame_b).await.unwrap();

        // Receive both
        let recv_b = b.recv_frame().await.unwrap();
        assert_eq!(recv_b.payload_bytes(), b"from A");

        let recv_a = a.recv_frame().await.unwrap();
        assert_eq!(recv_a.payload_bytes(), b"from B");
    }

    #[tokio::test]
    async fn test_concurrent_send_recv() {
        // This test verifies that send and recv can happen concurrently
        let (a, b) = StreamTransport::pair();
        let a = Arc::new(a);
        let b = Arc::new(b);

        // Spawn a task that sends multiple frames
        let a_sender = a.clone();
        let send_handle = tokio::spawn(async move {
            for i in 0..10u64 {
                let mut desc = MsgDescHot::new();
                desc.msg_id = i;
                let frame = Frame::with_inline_payload(desc, b"ping").unwrap();
                a_sender.send_frame(&frame).await.unwrap();
            }
        });

        // Spawn a task that receives and echoes back
        let b_clone = b.clone();
        let echo_handle = tokio::spawn(async move {
            for _ in 0..10 {
                let recv = b_clone.recv_frame().await.unwrap();
                let mut desc = MsgDescHot::new();
                desc.msg_id = recv.desc.msg_id;
                let frame = Frame::with_inline_payload(desc, b"pong").unwrap();
                b_clone.send_frame(&frame).await.unwrap();
            }
        });

        // Receive the echoed frames
        let a_receiver = a.clone();
        let recv_handle = tokio::spawn(async move {
            for _ in 0..10 {
                let recv = a_receiver.recv_frame().await.unwrap();
                assert_eq!(recv.payload_bytes(), b"pong");
            }
        });

        // Wait for all tasks
        send_handle.await.unwrap();
        echo_handle.await.unwrap();
        recv_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_close() {
        let (a, _b) = StreamTransport::pair();

        a.close().await.unwrap();
        assert!(a.is_closed());

        // Sending on closed transport should fail
        let frame = Frame::new(MsgDescHot::new());
        assert!(matches!(
            a.send_frame(&frame).await,
            Err(TransportError::Closed)
        ));
    }

    #[tokio::test]
    async fn test_encoder() {
        let (a, _b) = StreamTransport::pair();

        let mut encoder = a.encoder();
        encoder.encode_bytes(b"test data").unwrap();
        let frame = encoder.finish().unwrap();

        assert_eq!(frame.payload(), b"test data");
    }
}

/// Conformance tests using rapace-testkit.
#[cfg(test)]
mod conformance_tests {
    use super::*;
    use rapace_testkit::{TestError, TransportFactory};
    use tokio::io::{ReadHalf, WriteHalf};

    struct StreamFactory;

    impl TransportFactory for StreamFactory {
        type Transport =
            StreamTransport<ReadHalf<tokio::io::DuplexStream>, WriteHalf<tokio::io::DuplexStream>>;

        async fn connect_pair() -> Result<(Self::Transport, Self::Transport), TestError> {
            Ok(StreamTransport::pair())
        }
    }

    #[tokio::test]
    async fn unary_happy_path() {
        rapace_testkit::run_unary_happy_path::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn unary_multiple_calls() {
        rapace_testkit::run_unary_multiple_calls::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn ping_pong() {
        rapace_testkit::run_ping_pong::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn deadline_success() {
        rapace_testkit::run_deadline_success::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn deadline_exceeded() {
        rapace_testkit::run_deadline_exceeded::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn cancellation() {
        rapace_testkit::run_cancellation::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn credit_grant() {
        rapace_testkit::run_credit_grant::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn error_response() {
        rapace_testkit::run_error_response::<StreamFactory>().await;
    }

    // Session-level tests (semantic enforcement)

    #[tokio::test]
    async fn session_credit_exhaustion() {
        rapace_testkit::run_session_credit_exhaustion::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn session_cancelled_channel_drop() {
        rapace_testkit::run_session_cancelled_channel_drop::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn session_cancel_control_frame() {
        rapace_testkit::run_session_cancel_control_frame::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn session_grant_credits_control_frame() {
        rapace_testkit::run_session_grant_credits_control_frame::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn session_deadline_check() {
        rapace_testkit::run_session_deadline_check::<StreamFactory>().await;
    }

    // Streaming tests

    #[tokio::test]
    async fn server_streaming_happy_path() {
        rapace_testkit::run_server_streaming_happy_path::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn client_streaming_happy_path() {
        rapace_testkit::run_client_streaming_happy_path::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn bidirectional_streaming() {
        rapace_testkit::run_bidirectional_streaming::<StreamFactory>().await;
    }

    #[tokio::test]
    async fn streaming_cancellation() {
        rapace_testkit::run_streaming_cancellation::<StreamFactory>().await;
    }

    // Macro-generated streaming tests

    #[tokio::test]
    async fn macro_server_streaming() {
        rapace_testkit::run_macro_server_streaming::<StreamFactory>().await;
    }
}
