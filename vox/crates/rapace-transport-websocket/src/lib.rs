//! rapace-transport-websocket: WebSocket transport for rapace.
//!
//! For browser clients or WebSocket-based infrastructure.
//!
//! # Wire Format
//!
//! Each WebSocket binary message contains one frame:
//! - `[u8; 64]`: MsgDescHot as raw bytes (repr(C), POD)
//! - `[u8; payload_len]`: payload bytes
//!
//! Unlike the stream transport, no length prefix is needed since WebSocket
//! already provides message framing.
//!
//! # Characteristics
//!
//! - Each WebSocket message is one rapace frame
//! - Uses binary messages (not text/JSON)
//! - Everything is owned buffers (no zero-copy on receive)
//! - Same RPC semantics as other transports

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex as SyncMutex;
use rapace_core::{
    DecodeError, EncodeCtx, EncodeError, Frame, FrameView, MsgDescHot, Transport, TransportError,
    INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex as AsyncMutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// Size of MsgDescHot in bytes (must be 64).
const DESC_SIZE: usize = 64;

// Compile-time check that MsgDescHot is exactly 64 bytes
const _: () = assert!(std::mem::size_of::<MsgDescHot>() == DESC_SIZE);

/// WebSocket-based transport implementation.
///
/// Works with any WebSocket stream (TCP, TLS, etc.).
pub struct WebSocketTransport<S> {
    inner: Arc<WebSocketInner<S>>,
}

struct WebSocketInner<S> {
    /// Write half of the WebSocket (async mutex for holding across awaits).
    sink: AsyncMutex<SplitSink<WebSocketStream<S>, Message>>,
    /// Read half of the WebSocket (async mutex for holding across awaits).
    stream: AsyncMutex<SplitStream<WebSocketStream<S>>>,
    /// Buffer for the last received frame (for FrameView lifetime).
    last_frame: SyncMutex<Option<ReceivedFrame>>,
    /// Whether the transport is closed.
    closed: AtomicBool,
}

/// Internal storage for a received frame.
struct ReceivedFrame {
    desc: MsgDescHot,
    payload: Vec<u8>,
}

impl<S> WebSocketTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Create a new WebSocket transport wrapping the given WebSocket stream.
    pub fn new(ws: WebSocketStream<S>) -> Self {
        let (sink, stream) = ws.split();
        Self {
            inner: Arc::new(WebSocketInner {
                sink: AsyncMutex::new(sink),
                stream: AsyncMutex::new(stream),
                last_frame: SyncMutex::new(None),
                closed: AtomicBool::new(false),
            }),
        }
    }

    /// Check if the transport is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }
}

impl WebSocketTransport<tokio::io::DuplexStream> {
    /// Create a connected pair of WebSocket transports for testing.
    ///
    /// Uses `tokio::io::duplex` with WebSocket framing internally.
    pub async fn pair() -> (Self, Self) {
        // 64KB buffer should be plenty for testing
        let (client_stream, server_stream) = tokio::io::duplex(65536);

        // Wrap both ends with WebSocket framing.
        // We use the client/server handshake over the duplex streams.
        let (ws_a, ws_b) = tokio::join!(
            async {
                tokio_tungstenite::client_async("ws://localhost/", client_stream)
                    .await
                    .expect("client handshake failed")
                    .0
            },
            async {
                tokio_tungstenite::accept_async(server_stream)
                    .await
                    .expect("server handshake failed")
            }
        );

        (Self::new(ws_a), Self::new(ws_b))
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

impl<S> Transport for WebSocketTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let payload = frame.payload();

        // Build message: descriptor + payload
        let mut data = Vec::with_capacity(DESC_SIZE + payload.len());
        data.extend_from_slice(&desc_to_bytes(&frame.desc));
        data.extend_from_slice(payload);

        // Send as binary WebSocket message
        let mut sink = self.inner.sink.lock().await;
        sink.send(Message::Binary(data.into())).await.map_err(|e| {
            TransportError::Io(std::io::Error::other(format!("websocket send: {}", e)))
        })?;

        Ok(())
    }

    async fn recv_frame(&self) -> Result<FrameView<'_>, TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let mut stream = self.inner.stream.lock().await;

        // Read next message
        loop {
            let msg = stream
                .next()
                .await
                .ok_or(TransportError::Closed)?
                .map_err(|e| {
                    TransportError::Io(std::io::Error::other(format!("websocket recv: {}", e)))
                })?;

            match msg {
                Message::Binary(data) => {
                    // Validate minimum length
                    if data.len() < DESC_SIZE {
                        return Err(TransportError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("frame too small: {} < {}", data.len(), DESC_SIZE),
                        )));
                    }

                    // Parse descriptor
                    let desc_bytes: [u8; DESC_SIZE] = data[..DESC_SIZE].try_into().unwrap();
                    let mut desc = bytes_to_desc(&desc_bytes);

                    // Extract payload
                    let payload = data[DESC_SIZE..].to_vec();
                    let payload_len = payload.len();

                    // Drop stream lock before storing frame
                    drop(stream);

                    // Update desc.payload_len to match actual received payload
                    desc.payload_len = payload_len as u32;

                    // If payload fits inline, mark it as inline
                    if payload_len <= INLINE_PAYLOAD_SIZE {
                        desc.payload_slot = INLINE_PAYLOAD_SLOT;
                        desc.inline_payload[..payload_len].copy_from_slice(&payload);
                    } else {
                        // Mark as external payload
                        desc.payload_slot = 0;
                    }

                    // Store frame for FrameView lifetime
                    {
                        let mut last = self.inner.last_frame.lock();
                        *last = Some(ReceivedFrame { desc, payload });
                    }

                    // Create FrameView from stored frame
                    // SAFETY: The frame is stored in self.inner which lives as long as self.
                    // The returned FrameView borrows &self, preventing another recv_frame call.
                    let last = self.inner.last_frame.lock();
                    let frame_ref = last.as_ref().unwrap();

                    let desc_ptr = &frame_ref.desc as *const MsgDescHot;
                    let payload_slice = if frame_ref.desc.is_inline() {
                        frame_ref.desc.inline_payload()
                    } else {
                        &frame_ref.payload
                    };
                    let payload_ptr = payload_slice.as_ptr();
                    let payload_len = payload_slice.len();

                    // SAFETY: Extending lifetime is safe because:
                    // - Data lives in Arc<WebSocketInner> which outlives &self
                    // - FrameView borrows &self, preventing concurrent recv_frame
                    let desc: &MsgDescHot = unsafe { &*desc_ptr };
                    let payload: &[u8] =
                        unsafe { std::slice::from_raw_parts(payload_ptr, payload_len) };

                    return Ok(FrameView::new(desc, payload));
                }
                Message::Close(_) => {
                    self.inner.closed.store(true, Ordering::Release);
                    return Err(TransportError::Closed);
                }
                Message::Ping(_) | Message::Pong(_) | Message::Text(_) | Message::Frame(_) => {
                    // Ignore ping/pong/text frames, continue reading
                    continue;
                }
            }
        }
    }

    fn encoder(&self) -> Box<dyn EncodeCtx + '_> {
        Box::new(WebSocketEncoder::new())
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.inner.closed.store(true, Ordering::Release);

        // Send WebSocket close frame
        let mut sink = self.inner.sink.lock().await;
        let _ = sink.send(Message::Close(None)).await;

        Ok(())
    }
}

/// Encoder for WebSocket transport.
///
/// Simply accumulates bytes into a Vec.
pub struct WebSocketEncoder {
    desc: MsgDescHot,
    payload: Vec<u8>,
}

impl WebSocketEncoder {
    fn new() -> Self {
        Self {
            desc: MsgDescHot::new(),
            payload: Vec::new(),
        }
    }
}

impl EncodeCtx for WebSocketEncoder {
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.payload.extend_from_slice(bytes);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Frame, EncodeError> {
        Ok(Frame::with_payload(self.desc, self.payload))
    }
}

/// Decoder for WebSocket transport.
#[allow(dead_code)]
pub struct WebSocketDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

#[allow(dead_code)]
impl<'a> WebSocketDecoder<'a> {
    /// Create a new decoder from a byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
}

impl<'a> rapace_core::DecodeCtx<'a> for WebSocketDecoder<'a> {
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
        let (a, b) = WebSocketTransport::pair().await;
        assert!(!a.is_closed());
        assert!(!b.is_closed());
    }

    #[tokio::test]
    async fn test_send_recv_inline() {
        let (a, b) = WebSocketTransport::pair().await;

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
        let view = b.recv_frame().await.unwrap();
        assert_eq!(view.desc.msg_id, 1);
        assert_eq!(view.desc.channel_id, 1);
        assert_eq!(view.desc.method_id, 42);
        assert_eq!(view.payload, b"hello");
    }

    #[tokio::test]
    async fn test_send_recv_external_payload() {
        let (a, b) = WebSocketTransport::pair().await;

        let mut desc = MsgDescHot::new();
        desc.msg_id = 2;
        desc.flags = FrameFlags::DATA;

        let payload = vec![0u8; 1000]; // Larger than inline
        let frame = Frame::with_payload(desc, payload.clone());

        a.send_frame(&frame).await.unwrap();

        let view = b.recv_frame().await.unwrap();
        assert_eq!(view.desc.msg_id, 2);
        assert_eq!(view.payload.len(), 1000);
    }

    #[tokio::test]
    async fn test_bidirectional() {
        let (a, b) = WebSocketTransport::pair().await;

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
        let view_b = b.recv_frame().await.unwrap();
        assert_eq!(view_b.payload, b"from A");

        let view_a = a.recv_frame().await.unwrap();
        assert_eq!(view_a.payload, b"from B");
    }

    #[tokio::test]
    async fn test_close() {
        let (a, _b) = WebSocketTransport::pair().await;

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
        let (a, _b) = WebSocketTransport::pair().await;

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

    struct WebSocketFactory;

    impl TransportFactory for WebSocketFactory {
        type Transport = WebSocketTransport<tokio::io::DuplexStream>;

        async fn connect_pair() -> Result<(Self::Transport, Self::Transport), TestError> {
            Ok(WebSocketTransport::pair().await)
        }
    }

    #[tokio::test]
    async fn unary_happy_path() {
        rapace_testkit::run_unary_happy_path::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn unary_multiple_calls() {
        rapace_testkit::run_unary_multiple_calls::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn ping_pong() {
        rapace_testkit::run_ping_pong::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn deadline_success() {
        rapace_testkit::run_deadline_success::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn deadline_exceeded() {
        rapace_testkit::run_deadline_exceeded::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn cancellation() {
        rapace_testkit::run_cancellation::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn credit_grant() {
        rapace_testkit::run_credit_grant::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn error_response() {
        rapace_testkit::run_error_response::<WebSocketFactory>().await;
    }

    // Session-level tests (semantic enforcement)

    #[tokio::test]
    async fn session_credit_exhaustion() {
        rapace_testkit::run_session_credit_exhaustion::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn session_cancelled_channel_drop() {
        rapace_testkit::run_session_cancelled_channel_drop::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn session_cancel_control_frame() {
        rapace_testkit::run_session_cancel_control_frame::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn session_grant_credits_control_frame() {
        rapace_testkit::run_session_grant_credits_control_frame::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn session_deadline_check() {
        rapace_testkit::run_session_deadline_check::<WebSocketFactory>().await;
    }

    // Streaming tests

    #[tokio::test]
    async fn server_streaming_happy_path() {
        rapace_testkit::run_server_streaming_happy_path::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn client_streaming_happy_path() {
        rapace_testkit::run_client_streaming_happy_path::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn bidirectional_streaming() {
        rapace_testkit::run_bidirectional_streaming::<WebSocketFactory>().await;
    }

    #[tokio::test]
    async fn streaming_cancellation() {
        rapace_testkit::run_streaming_cancellation::<WebSocketFactory>().await;
    }

    // Macro-generated streaming tests

    #[tokio::test]
    async fn macro_server_streaming() {
        rapace_testkit::run_macro_server_streaming::<WebSocketFactory>().await;
    }
}
