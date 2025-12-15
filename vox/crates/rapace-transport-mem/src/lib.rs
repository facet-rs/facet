//! rapace-transport-mem: In-process transport for rapace.
//!
//! This is the **semantic reference** implementation. All other transports
//! must behave identically to this one. If behavior differs, the other
//! transport has a bug.
//!
//! # Characteristics
//!
//! - Frames are passed through async channels (no serialization)
//! - Real Rust lifetimes for in-process calls
//! - Full RPC semantics (channels, deadlines, cancellation)
//!
//! # Usage
//!
//! ```ignore
//! let (client_transport, server_transport) = InProcTransport::pair();
//! ```

#![forbid(unsafe_code)]

use std::sync::Arc;

use rapace_core::{
    DecodeError, EncodeCtx, EncodeError, Frame, MsgDescHot, RecvFrame, Transport, TransportError,
};
use tokio::sync::mpsc;

/// Channel capacity for the in-proc transport.
const CHANNEL_CAPACITY: usize = 64;

/// In-process transport implementation.
///
/// This transport passes frames through async channels without serialization.
/// It serves as the semantic reference for correct RPC behavior.
pub struct InProcTransport {
    inner: Arc<InProcInner>,
}

struct InProcInner {
    /// Channel to send frames to the peer.
    tx: mpsc::Sender<Frame>,
    /// Channel to receive frames from the peer.
    rx: tokio::sync::Mutex<mpsc::Receiver<Frame>>,
    /// Whether the transport is closed.
    closed: std::sync::atomic::AtomicBool,
}

impl InProcTransport {
    /// Create a connected pair of in-proc transports.
    ///
    /// Returns (A, B) where frames sent on A are received on B and vice versa.
    pub fn pair() -> (Self, Self) {
        let (tx_a, rx_a) = mpsc::channel(CHANNEL_CAPACITY);
        let (tx_b, rx_b) = mpsc::channel(CHANNEL_CAPACITY);

        let inner_a = Arc::new(InProcInner {
            tx: tx_b, // A sends to B's receiver
            rx: tokio::sync::Mutex::new(rx_a),
            closed: std::sync::atomic::AtomicBool::new(false),
        });

        let inner_b = Arc::new(InProcInner {
            tx: tx_a, // B sends to A's receiver
            rx: tokio::sync::Mutex::new(rx_b),
            closed: std::sync::atomic::AtomicBool::new(false),
        });

        (Self { inner: inner_a }, Self { inner: inner_b })
    }

    /// Check if the transport is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl Transport for InProcTransport {
    /// Payload type for received frames.
    /// For now we use `Vec<u8>`; Phase 2 will introduce pooled buffers.
    type RecvPayload = Vec<u8>;

    async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        // Clone the frame for sending (in-proc still needs ownership transfer)
        self.inner
            .tx
            .send(frame.clone())
            .await
            .map_err(|_| TransportError::Closed)
    }

    async fn recv_frame(&self) -> Result<RecvFrame<Self::RecvPayload>, TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        // Receive the frame from the channel
        let frame = {
            let mut rx = self.inner.rx.lock().await;
            rx.recv().await.ok_or(TransportError::Closed)?
        };

        // Convert Frame to RecvFrame
        if frame.desc.is_inline() {
            Ok(RecvFrame::inline(frame.desc))
        } else {
            // Take ownership of the payload
            let payload = frame.payload.unwrap_or_default();
            Ok(RecvFrame::with_payload(frame.desc, payload))
        }
    }

    fn encoder(&self) -> Box<dyn EncodeCtx + '_> {
        Box::new(InProcEncoder::new())
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}

/// Encoder for in-proc transport.
///
/// Simply accumulates bytes into a Vec since no serialization is needed.
pub struct InProcEncoder {
    desc: MsgDescHot,
    payload: Vec<u8>,
}

impl InProcEncoder {
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

impl EncodeCtx for InProcEncoder {
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.payload.extend_from_slice(bytes);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Frame, EncodeError> {
        Ok(Frame::with_payload(self.desc, self.payload))
    }
}

/// Decoder for in-proc transport.
pub struct InProcDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> InProcDecoder<'a> {
    /// Create a new decoder from a byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
}

impl<'a> rapace_core::DecodeCtx<'a> for InProcDecoder<'a> {
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
        let (a, b) = InProcTransport::pair();
        assert!(!a.is_closed());
        assert!(!b.is_closed());
    }

    #[tokio::test]
    async fn test_send_recv_inline() {
        let (a, b) = InProcTransport::pair();

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
        let (a, b) = InProcTransport::pair();

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
        let (a, b) = InProcTransport::pair();

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
    async fn test_close() {
        let (a, _b) = InProcTransport::pair();

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
        let (a, _b) = InProcTransport::pair();

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
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_tracing() {
        INIT.call_once(|| {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive(tracing::Level::DEBUG.into()),
                )
                .with_test_writer()
                .init();
        });
    }

    struct InProcFactory;

    impl TransportFactory for InProcFactory {
        type Transport = InProcTransport;

        async fn connect_pair() -> Result<(Self::Transport, Self::Transport), TestError> {
            Ok(InProcTransport::pair())
        }
    }

    #[tokio::test]
    async fn unary_happy_path() {
        init_tracing();
        rapace_testkit::run_unary_happy_path::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn unary_multiple_calls() {
        rapace_testkit::run_unary_multiple_calls::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn ping_pong() {
        rapace_testkit::run_ping_pong::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn deadline_success() {
        rapace_testkit::run_deadline_success::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn deadline_exceeded() {
        rapace_testkit::run_deadline_exceeded::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn cancellation() {
        rapace_testkit::run_cancellation::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn credit_grant() {
        rapace_testkit::run_credit_grant::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn error_response() {
        rapace_testkit::run_error_response::<InProcFactory>().await;
    }

    // Session-level tests (semantic enforcement)

    #[tokio::test]
    async fn session_credit_exhaustion() {
        rapace_testkit::run_session_credit_exhaustion::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn session_cancelled_channel_drop() {
        rapace_testkit::run_session_cancelled_channel_drop::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn session_cancel_control_frame() {
        rapace_testkit::run_session_cancel_control_frame::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn session_grant_credits_control_frame() {
        rapace_testkit::run_session_grant_credits_control_frame::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn session_deadline_check() {
        rapace_testkit::run_session_deadline_check::<InProcFactory>().await;
    }

    // Streaming tests

    #[tokio::test]
    async fn server_streaming_happy_path() {
        rapace_testkit::run_server_streaming_happy_path::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn client_streaming_happy_path() {
        rapace_testkit::run_client_streaming_happy_path::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn bidirectional_streaming() {
        rapace_testkit::run_bidirectional_streaming::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn streaming_cancellation() {
        rapace_testkit::run_streaming_cancellation::<InProcFactory>().await;
    }

    // Macro-generated streaming tests

    #[tokio::test]
    async fn macro_server_streaming() {
        rapace_testkit::run_macro_server_streaming::<InProcFactory>().await;
    }

    // Large blob tests

    #[tokio::test]
    async fn large_blob_echo() {
        rapace_testkit::run_large_blob_echo::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn large_blob_transform() {
        rapace_testkit::run_large_blob_transform::<InProcFactory>().await;
    }

    #[tokio::test]
    async fn large_blob_checksum() {
        rapace_testkit::run_large_blob_checksum::<InProcFactory>().await;
    }
}
