//! rapace-transport-websocket: WebSocket transport for rapace.
//!
//! For browser clients or WebSocket-based infrastructure.
//!
//! # Features
//!
//! - `tungstenite` (default): Support for `tokio-tungstenite` WebSocket streams
//! - `axum`: Support for `axum::extract::ws::WebSocket`

use rapace_core::{
    DecodeError, EncodeCtx, EncodeError, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT,
    MsgDescHot, RecvFrame, Transport, TransportError,
};

mod shared {
    use super::*;

    /// Size of MsgDescHot in bytes (must be 64).
    pub const DESC_SIZE: usize = 64;
    const _: () = assert!(std::mem::size_of::<MsgDescHot>() == DESC_SIZE);

    /// Convert MsgDescHot to raw bytes.
    pub fn desc_to_bytes(desc: &MsgDescHot) -> [u8; DESC_SIZE] {
        // SAFETY: MsgDescHot is repr(C), Copy, and exactly 64 bytes.
        unsafe { std::mem::transmute_copy(desc) }
    }

    /// Convert raw bytes to MsgDescHot.
    pub fn bytes_to_desc(bytes: &[u8; DESC_SIZE]) -> MsgDescHot {
        // SAFETY: Same as desc_to_bytes.
        unsafe { std::mem::transmute_copy(bytes) }
    }

    /// Encoder for WebSocket transport.
    ///
    /// Simply accumulates bytes into a Vec.
    pub struct WebSocketEncoder {
        desc: MsgDescHot,
        payload: Vec<u8>,
    }

    impl Default for WebSocketEncoder {
        fn default() -> Self {
            Self::new()
        }
    }

    impl WebSocketEncoder {
        pub fn new() -> Self {
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
    pub struct WebSocketDecoder<'a> {
        data: &'a [u8],
        pos: usize,
    }

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

    pub use WebSocketDecoder as Decoder;
    pub use WebSocketEncoder as Encoder;
    pub use {bytes_to_desc as to_desc, desc_to_bytes as to_bytes};
}

pub use shared::{Decoder as WebSocketDecoder, Encoder as WebSocketEncoder};

/// Abstraction over WebSocket message types.
///
/// This trait allows [`WebSocketTransport`] to work with different WebSocket
/// implementations (tokio-tungstenite, axum, etc.).
#[cfg(not(target_arch = "wasm32"))]
pub trait WsMessage: Sized + Send {
    /// Create a binary message from data.
    fn binary(data: Vec<u8>) -> Self;

    /// Create a close message.
    fn close() -> Self;

    /// Returns `true` if this is a close message.
    fn is_close(&self) -> bool;

    /// Try to extract binary data. Returns `None` if not a binary message.
    fn into_binary(self) -> Option<Vec<u8>>;

    /// Returns `true` if this message should be skipped (ping, pong, text, etc.).
    fn should_skip(&self) -> bool;
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::shared::{DESC_SIZE, to_bytes, to_desc};
    use super::*;
    use futures::stream::{SplitSink, SplitStream};
    use futures::{Sink, SinkExt, Stream, StreamExt};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::Mutex as AsyncMutex;

    /// Helper trait to name the error type from the Stream impl.
    pub trait StreamErrorType: Stream {
        type Error: std::error::Error + Send + Sync + 'static;
    }

    impl<T, I, E> StreamErrorType for T
    where
        T: Stream<Item = Result<I, E>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        type Error = E;
    }

    /// WebSocket-based transport implementation.
    ///
    /// Generic over the WebSocket stream type `WS` and message type `M`.
    /// Use the feature-specific type aliases for convenience:
    /// - `tungstenite` feature: [`TungsteniteTransport`]
    /// - `axum` feature: [`AxumTransport`]
    pub struct WebSocketTransport<WS, M>
    where
        WS: Stream<Item = Result<M, <WS as StreamErrorType>::Error>>
            + Sink<M>
            + StreamErrorType
            + Unpin
            + Send
            + 'static,
        M: WsMessage,
    {
        inner: Arc<WebSocketInner<WS, M>>,
    }

    struct WebSocketInner<WS, M>
    where
        WS: Stream<Item = Result<M, <WS as StreamErrorType>::Error>>
            + Sink<M>
            + StreamErrorType
            + Unpin
            + Send
            + 'static,
        M: WsMessage,
    {
        /// Write half of the WebSocket (async mutex for holding across awaits).
        sink: AsyncMutex<SplitSink<WS, M>>,
        /// Read half of the WebSocket (async mutex for holding across awaits).
        stream: AsyncMutex<SplitStream<WS>>,
        /// Whether the transport is closed.
        closed: AtomicBool,
    }

    impl<WS, M> WebSocketTransport<WS, M>
    where
        WS: Stream<Item = Result<M, <WS as StreamErrorType>::Error>>
            + Sink<M>
            + StreamErrorType
            + Unpin
            + Send
            + 'static,
        M: WsMessage,
    {
        /// Create a new WebSocket transport wrapping the given WebSocket stream.
        pub fn new(ws: WS) -> Self {
            let (sink, stream) = ws.split();
            Self {
                inner: Arc::new(WebSocketInner {
                    sink: AsyncMutex::new(sink),
                    stream: AsyncMutex::new(stream),
                    closed: AtomicBool::new(false),
                }),
            }
        }

        /// Check if the transport is closed.
        pub fn is_closed(&self) -> bool {
            self.inner.closed.load(Ordering::Acquire)
        }
    }

    impl<WS, M> Transport for WebSocketTransport<WS, M>
    where
        WS: Stream<Item = Result<M, <WS as StreamErrorType>::Error>>
            + Sink<M>
            + StreamErrorType
            + Unpin
            + Send
            + Sync
            + 'static,
        <WS as Sink<M>>::Error: std::error::Error + Send + Sync + 'static,
        M: WsMessage,
    {
        /// Payload type for received frames.
        /// For now we use `Vec<u8>`; Phase 2 will introduce pooled buffers.
        type RecvPayload = Vec<u8>;

        async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            let payload = frame.payload();

            // Build message: descriptor + payload
            let mut data = Vec::with_capacity(DESC_SIZE + payload.len());
            data.extend_from_slice(&to_bytes(&frame.desc));
            data.extend_from_slice(payload);

            // Send as binary WebSocket message
            let mut sink = self.inner.sink.lock().await;
            sink.send(M::binary(data)).await.map_err(|e| {
                TransportError::Io(std::io::Error::other(format!("websocket send: {}", e)))
            })?;

            Ok(())
        }

        async fn recv_frame(&self) -> Result<RecvFrame<Self::RecvPayload>, TransportError> {
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

                if msg.is_close() {
                    self.inner.closed.store(true, Ordering::Release);
                    return Err(TransportError::Closed);
                }

                if msg.should_skip() {
                    continue;
                }

                if let Some(data) = msg.into_binary() {
                    // Validate minimum length
                    if data.len() < DESC_SIZE {
                        return Err(TransportError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("frame too small: {} < {}", data.len(), DESC_SIZE),
                        )));
                    }

                    // Parse descriptor
                    let desc_bytes: [u8; DESC_SIZE] = data[..DESC_SIZE].try_into().unwrap();
                    let mut desc = to_desc(&desc_bytes);

                    // Extract payload
                    let payload = data[DESC_SIZE..].to_vec();
                    let payload_len = payload.len();

                    // Update desc.payload_len to match actual received payload
                    desc.payload_len = payload_len as u32;

                    // If payload fits inline, mark it as inline
                    if payload_len <= INLINE_PAYLOAD_SIZE {
                        desc.payload_slot = INLINE_PAYLOAD_SLOT;
                        desc.inline_payload[..payload_len].copy_from_slice(&payload);
                        return Ok(RecvFrame::inline(desc));
                    } else {
                        // Mark as external payload
                        desc.payload_slot = 0;
                        return Ok(RecvFrame::with_payload(desc, payload));
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
            let _ = sink.send(M::close()).await;

            Ok(())
        }
    }
}

// Feature: tungstenite
#[cfg(all(not(target_arch = "wasm32"), feature = "tungstenite"))]
mod tungstenite_impl {
    use super::*;
    use tokio_tungstenite::tungstenite::Message;

    impl WsMessage for Message {
        fn binary(data: Vec<u8>) -> Self {
            Message::Binary(data.into())
        }

        fn close() -> Self {
            Message::Close(None)
        }

        fn is_close(&self) -> bool {
            matches!(self, Message::Close(_))
        }

        fn into_binary(self) -> Option<Vec<u8>> {
            match self {
                Message::Binary(data) => Some(data.into()),
                _ => None,
            }
        }

        fn should_skip(&self) -> bool {
            matches!(
                self,
                Message::Ping(_) | Message::Pong(_) | Message::Text(_) | Message::Frame(_)
            )
        }
    }

    /// WebSocket transport using `tokio-tungstenite`.
    ///
    /// This is the default WebSocket transport for native (non-WASM) targets.
    pub type TungsteniteTransport<S> =
        native::WebSocketTransport<tokio_tungstenite::WebSocketStream<S>, Message>;

    // Static assertions: TungsteniteTransport must be Send + Sync
    static_assertions::assert_impl_all!(TungsteniteTransport<tokio::io::DuplexStream>: Send, Sync);

    impl TungsteniteTransport<tokio::io::DuplexStream> {
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use rapace_core::FrameFlags;

        #[tokio::test]
        async fn test_pair_creation() {
            let (a, b) = TungsteniteTransport::pair().await;
            assert!(!a.is_closed());
            assert!(!b.is_closed());
        }

        #[tokio::test]
        async fn test_send_recv_inline() {
            let (a, b) = TungsteniteTransport::pair().await;

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
            let (a, b) = TungsteniteTransport::pair().await;

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
            let (a, b) = TungsteniteTransport::pair().await;

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
            let (a, _b) = TungsteniteTransport::pair().await;

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
            let (a, _b) = TungsteniteTransport::pair().await;

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

        struct TungsteniteFactory;

        impl TransportFactory for TungsteniteFactory {
            type Transport = TungsteniteTransport<tokio::io::DuplexStream>;

            async fn connect_pair() -> Result<(Self::Transport, Self::Transport), TestError> {
                Ok(TungsteniteTransport::pair().await)
            }
        }

        #[tokio::test]
        async fn unary_happy_path() {
            rapace_testkit::run_unary_happy_path::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn unary_multiple_calls() {
            rapace_testkit::run_unary_multiple_calls::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn ping_pong() {
            rapace_testkit::run_ping_pong::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn deadline_success() {
            rapace_testkit::run_deadline_success::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn deadline_exceeded() {
            rapace_testkit::run_deadline_exceeded::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn cancellation() {
            rapace_testkit::run_cancellation::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn credit_grant() {
            rapace_testkit::run_credit_grant::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn error_response() {
            rapace_testkit::run_error_response::<TungsteniteFactory>().await;
        }

        // Session-level tests (semantic enforcement)

        #[tokio::test]
        async fn session_credit_exhaustion() {
            rapace_testkit::run_session_credit_exhaustion::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn session_cancelled_channel_drop() {
            rapace_testkit::run_session_cancelled_channel_drop::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn session_cancel_control_frame() {
            rapace_testkit::run_session_cancel_control_frame::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn session_grant_credits_control_frame() {
            rapace_testkit::run_session_grant_credits_control_frame::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn session_deadline_check() {
            rapace_testkit::run_session_deadline_check::<TungsteniteFactory>().await;
        }

        // Streaming tests

        #[tokio::test]
        async fn server_streaming_happy_path() {
            rapace_testkit::run_server_streaming_happy_path::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn client_streaming_happy_path() {
            rapace_testkit::run_client_streaming_happy_path::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn bidirectional_streaming() {
            rapace_testkit::run_bidirectional_streaming::<TungsteniteFactory>().await;
        }

        #[tokio::test]
        async fn streaming_cancellation() {
            rapace_testkit::run_streaming_cancellation::<TungsteniteFactory>().await;
        }

        // Macro-generated streaming tests

        #[tokio::test]
        async fn macro_server_streaming() {
            rapace_testkit::run_macro_server_streaming::<TungsteniteFactory>().await;
        }
    }
}

// Feature: axum
#[cfg(all(not(target_arch = "wasm32"), feature = "axum"))]
mod axum_impl {
    use super::*;
    use axum::extract::ws::Message;

    impl WsMessage for Message {
        fn binary(data: Vec<u8>) -> Self {
            Message::Binary(data.into())
        }

        fn close() -> Self {
            Message::Close(None)
        }

        fn is_close(&self) -> bool {
            matches!(self, Message::Close(_))
        }

        fn into_binary(self) -> Option<Vec<u8>> {
            match self {
                Message::Binary(data) => Some(data.into()),
                _ => None,
            }
        }

        fn should_skip(&self) -> bool {
            matches!(self, Message::Ping(_) | Message::Pong(_) | Message::Text(_))
        }
    }

    /// WebSocket transport using `axum::extract::ws::WebSocket`.
    ///
    /// Use this when building WebSocket handlers with the axum web framework.
    pub type AxumTransport = native::WebSocketTransport<axum::extract::ws::WebSocket, Message>;

    // Static assertions: AxumTransport must be Send + Sync
    // Note: axum::extract::ws::WebSocket itself is NOT Sync (contains Box<dyn Io + Send>),
    // but our WebSocketTransport wrapper is Sync because AsyncMutex<T> is Sync when T: Send.
    static_assertions::assert_impl_all!(AxumTransport: Send, Sync);
}

// Handle + Driver implementation for native targets
#[cfg(not(target_arch = "wasm32"))]
mod handle;

#[cfg(not(target_arch = "wasm32"))]
pub use handle::{
    DEFAULT_INBOUND_QUEUE_SIZE, DEFAULT_OUTBOUND_QUEUE_SIZE, DriverState, OutboundMsg,
    WebSocketConfig, WebSocketHandle, into_parts, into_parts_with_config,
};

// Re-exports for native targets
#[cfg(not(target_arch = "wasm32"))]
pub use native::{StreamErrorType, WebSocketTransport};

#[cfg(all(not(target_arch = "wasm32"), feature = "tungstenite"))]
pub use tungstenite_impl::TungsteniteTransport;

#[cfg(all(not(target_arch = "wasm32"), feature = "axum"))]
pub use axum_impl::AxumTransport;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::shared::{DESC_SIZE, to_bytes, to_desc};
    use super::*;
    use gloo_timers::future::TimeoutFuture;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{BinaryType, CloseEvent, ErrorEvent, MessageEvent, WebSocket};

    /// WebSocket transport implementation for browser environments.
    pub struct WebSocketTransport {
        inner: Arc<WebSocketInner>,
    }

    struct WebSocketInner {
        ws: WasmWebSocket,
        closed: AtomicBool,
    }

    impl WebSocketTransport {
        /// Connect to a WebSocket server at the given URL.
        pub async fn connect(url: &str) -> Result<Self, TransportError> {
            let ws = WasmWebSocket::connect(url).await?;
            Ok(Self {
                inner: Arc::new(WebSocketInner {
                    ws,
                    closed: AtomicBool::new(false),
                }),
            })
        }

        fn is_closed(&self) -> bool {
            self.inner.closed.load(Ordering::Acquire)
        }
    }

    impl Transport for WebSocketTransport {
        /// Payload type for received frames.
        type RecvPayload = Vec<u8>;

        async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            let payload = frame.payload();
            let mut data = Vec::with_capacity(DESC_SIZE + payload.len());
            data.extend_from_slice(&to_bytes(&frame.desc));
            data.extend_from_slice(payload);

            self.inner.ws.send(&data)?;
            Ok(())
        }

        async fn recv_frame(&self) -> Result<RecvFrame<Self::RecvPayload>, TransportError> {
            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            let data = self.inner.ws.recv().await?;

            if data.len() < DESC_SIZE {
                return Err(TransportError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("frame too small: {} < {}", data.len(), DESC_SIZE),
                )));
            }

            let desc_bytes: [u8; DESC_SIZE] = data[..DESC_SIZE].try_into().unwrap();
            let mut desc = to_desc(&desc_bytes);

            let payload = data[DESC_SIZE..].to_vec();
            let payload_len = payload.len();
            desc.payload_len = payload_len as u32;

            if payload_len <= INLINE_PAYLOAD_SIZE {
                desc.payload_slot = INLINE_PAYLOAD_SLOT;
                desc.inline_payload[..payload_len].copy_from_slice(&payload);
                Ok(RecvFrame::inline(desc))
            } else {
                desc.payload_slot = 0;
                Ok(RecvFrame::with_payload(desc, payload))
            }
        }

        fn encoder(&self) -> Box<dyn EncodeCtx + '_> {
            Box::new(WebSocketEncoder::new())
        }

        async fn close(&self) -> Result<(), TransportError> {
            self.inner.closed.store(true, Ordering::Release);
            self.inner.ws.close();
            Ok(())
        }
    }

    /// A wasm-compatible WebSocket wrapper.
    struct WasmWebSocket {
        ws: WebSocket,
        received: Rc<RefCell<VecDeque<Vec<u8>>>>,
        error: Rc<RefCell<Option<String>>>,
        closed: Rc<Cell<bool>>,
    }

    unsafe impl Send for WasmWebSocket {}
    unsafe impl Sync for WasmWebSocket {}

    impl WasmWebSocket {
        async fn connect(url: &str) -> Result<Self, TransportError> {
            let ws = WebSocket::new(url).map_err(js_error_from_value)?;
            ws.set_binary_type(BinaryType::Arraybuffer);

            let received = Rc::new(RefCell::new(VecDeque::new()));
            let error: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
            let closed = Rc::new(Cell::new(false));

            let open_result: Rc<RefCell<Option<Result<(), String>>>> = Rc::new(RefCell::new(None));

            {
                let open_result_clone = Rc::clone(&open_result);
                let onopen = Closure::<dyn FnMut()>::once(move || {
                    *open_result_clone.borrow_mut() = Some(Ok(()));
                });
                ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
                onopen.forget();
            }

            {
                let open_result_clone = Rc::clone(&open_result);
                let onerror = Closure::<dyn FnMut(ErrorEvent)>::once(move |e: ErrorEvent| {
                    let msg = e.message();
                    let err_msg = if msg.is_empty() {
                        "WebSocket connection failed".to_string()
                    } else {
                        msg
                    };
                    *open_result_clone.borrow_mut() = Some(Err(err_msg));
                });
                ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
                onerror.forget();
            }

            loop {
                if let Some(result) = open_result.borrow_mut().take() {
                    match result {
                        Ok(()) => break,
                        Err(msg) => return Err(js_error_from_msg(msg)),
                    }
                }
                SendTimeoutFuture::new(10).await;
            }

            {
                let received = Rc::clone(&received);
                let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                    if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                        let array = js_sys::Uint8Array::new(&abuf);
                        received.borrow_mut().push_back(array.to_vec());
                    }
                });
                ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
                onmessage.forget();
            }

            {
                let error = Rc::clone(&error);
                let onerror = Closure::<dyn FnMut(ErrorEvent)>::new(move |e: ErrorEvent| {
                    *error.borrow_mut() = Some(format!("WebSocket error: {}", e.message()));
                });
                ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
                onerror.forget();
            }

            {
                let closed_clone = Rc::clone(&closed);
                let onclose = Closure::<dyn FnMut(CloseEvent)>::new(move |_e: CloseEvent| {
                    closed_clone.set(true);
                });
                ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
                onclose.forget();
            }

            Ok(Self {
                ws,
                received,
                error,
                closed,
            })
        }

        fn send(&self, data: &[u8]) -> Result<(), TransportError> {
            if self.closed.get() {
                return Err(TransportError::Closed);
            }

            if let Some(err) = self.error.borrow().as_ref() {
                return Err(js_error_from_msg(err.clone()));
            }

            self.ws
                .send_with_u8_array(data)
                .map_err(js_error_from_value)
        }

        async fn recv(&self) -> Result<Vec<u8>, TransportError> {
            loop {
                if let Some(err) = self.error.borrow().as_ref() {
                    return Err(js_error_from_msg(err.clone()));
                }

                if let Some(data) = self.received.borrow_mut().pop_front() {
                    return Ok(data);
                }

                if self.closed.get() {
                    return Err(TransportError::Closed);
                }

                SendTimeoutFuture::new(1).await;
            }
        }

        fn close(&self) {
            let _ = self.ws.close();
            self.closed.set(true);
        }
    }

    struct SendTimeoutFuture {
        inner: TimeoutFuture,
    }

    impl SendTimeoutFuture {
        fn new(ms: u32) -> Self {
            Self {
                inner: TimeoutFuture::new(ms),
            }
        }
    }

    impl Future for SendTimeoutFuture {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            Pin::new(&mut self.inner).poll(cx)
        }
    }

    unsafe impl Send for SendTimeoutFuture {}

    fn js_error_from_value(err: JsValue) -> TransportError {
        let msg = if let Some(s) = err.as_string() {
            s
        } else if let Ok(js_string) = js_sys::JSON::stringify(&err) {
            js_string.as_string().unwrap_or_else(|| format!("{err:?}"))
        } else {
            format!("{err:?}")
        };
        TransportError::Io(std::io::Error::other(msg))
    }

    fn js_error_from_msg<S: Into<String>>(msg: S) -> TransportError {
        TransportError::Io(std::io::Error::other(msg.into()))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WebSocketTransport;
