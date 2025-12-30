// SAFETY: This module contains `unsafe impl Send/Sync` for WASM types.
//
// On WASM, there's only one thread, so Send/Sync are trivially satisfied.
// These impls allow using the WebSocket transport with async runtimes that
// require Send bounds on futures.
#![allow(unsafe_code)]

use crate::MsgDescHot;

/// Size of MsgDescHot in bytes (must be 64).
const DESC_SIZE: usize = 64;
const _: () = assert!(std::mem::size_of::<MsgDescHot>() == DESC_SIZE);

fn desc_to_bytes(desc: &MsgDescHot) -> [u8; DESC_SIZE] {
    desc.to_bytes()
}

fn bytes_to_desc(bytes: &[u8; DESC_SIZE]) -> MsgDescHot {
    MsgDescHot::from_bytes(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{DESC_SIZE, bytes_to_desc, desc_to_bytes};
    use bytes::Bytes;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use futures_util::{SinkExt, StreamExt};
    use tokio::sync::Mutex as AsyncMutex;
    use tokio::sync::mpsc;

    use crate::{
        BufferPool, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, Payload, TransportError,
    };

    use super::super::Transport;

    #[cfg(feature = "websocket-axum")]
    use axum::extract::ws::{Message as AxumMessage, WebSocket as AxumWebSocket};

    #[cfg(feature = "websocket")]
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    enum OutMsg {
        Data(Vec<u8>),
        Close,
    }

    struct WebSocketInner {
        send: mpsc::Sender<OutMsg>,
        recv: AsyncMutex<mpsc::Receiver<Bytes>>,
        closed: AtomicBool,
        buffer_pool: BufferPool,
    }

    #[derive(Clone)]
    pub struct WebSocketTransport {
        inner: Arc<WebSocketInner>,
    }

    impl std::fmt::Debug for WebSocketTransport {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WebSocketTransport")
                .field("closed", &self.inner.closed.load(Ordering::Acquire))
                .finish_non_exhaustive()
        }
    }

    impl WebSocketTransport {
        #[cfg(feature = "websocket")]
        pub fn new<S>(ws: tokio_tungstenite::WebSocketStream<S>) -> Self
        where
            S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
        {
            Self::with_buffer_pool(ws, BufferPool::new())
        }

        #[cfg(feature = "websocket")]
        pub fn with_buffer_pool<S>(
            ws: tokio_tungstenite::WebSocketStream<S>,
            buffer_pool: BufferPool,
        ) -> Self
        where
            S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
        {
            let (send_tx, mut send_rx) = mpsc::channel::<OutMsg>(64);
            let (recv_tx, recv_rx) = mpsc::channel::<Bytes>(64);
            let inner = Arc::new(WebSocketInner {
                send: send_tx,
                recv: AsyncMutex::new(recv_rx),
                closed: AtomicBool::new(false),
                buffer_pool,
            });

            let (mut sink, mut stream) = ws.split();

            let inner_for_writer = inner.clone();
            tokio::spawn(async move {
                while let Some(msg) = send_rx.recv().await {
                    match msg {
                        OutMsg::Data(data) => {
                            if sink
                                .send(TungsteniteMessage::Binary(data.into()))
                                .await
                                .is_err()
                            {
                                inner_for_writer.closed.store(true, Ordering::Release);
                                break;
                            }
                        }
                        OutMsg::Close => {
                            let _ = sink.send(TungsteniteMessage::Close(None)).await;
                            inner_for_writer.closed.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            });

            let inner_for_reader = inner.clone();
            tokio::spawn(async move {
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(TungsteniteMessage::Binary(data)) => {
                            // Keep the Bytes from tungstenite - zero copy!
                            if recv_tx.send(data).await.is_err() {
                                break;
                            }
                        }
                        Ok(TungsteniteMessage::Close(_)) => {
                            inner_for_reader.closed.store(true, Ordering::Release);
                            break;
                        }
                        Ok(TungsteniteMessage::Ping(_))
                        | Ok(TungsteniteMessage::Pong(_))
                        | Ok(TungsteniteMessage::Text(_))
                        | Ok(TungsteniteMessage::Frame(_)) => {}
                        Err(_) => {
                            inner_for_reader.closed.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            });

            Self { inner }
        }

        #[cfg(feature = "websocket")]
        pub async fn pair() -> (Self, Self) {
            let (client_stream, server_stream) = tokio::io::duplex(65536);

            let client_fut = tokio_tungstenite::client_async("ws://localhost/", client_stream);
            let server_fut = tokio_tungstenite::accept_async(server_stream);

            let (client_result, server_result) =
                futures_util::future::join(client_fut, server_fut).await;

            let ws_a = client_result.expect("client handshake failed").0;
            let ws_b = server_result.expect("server handshake failed");

            (Self::new(ws_a), Self::new(ws_b))
        }

        #[cfg(feature = "websocket-axum")]
        pub fn from_axum(ws: AxumWebSocket) -> Self {
            Self::from_axum_with_buffer_pool(ws, BufferPool::new())
        }

        #[cfg(feature = "websocket-axum")]
        pub fn from_axum_with_buffer_pool(ws: AxumWebSocket, buffer_pool: BufferPool) -> Self {
            let (send_tx, mut send_rx) = mpsc::channel::<OutMsg>(64);
            let (recv_tx, recv_rx) = mpsc::channel::<Bytes>(64);
            let inner = Arc::new(WebSocketInner {
                send: send_tx,
                recv: AsyncMutex::new(recv_rx),
                closed: AtomicBool::new(false),
                buffer_pool,
            });

            let (mut sink, mut stream) = ws.split();

            let inner_for_writer = inner.clone();
            tokio::spawn(async move {
                while let Some(msg) = send_rx.recv().await {
                    match msg {
                        OutMsg::Data(data) => {
                            if sink.send(AxumMessage::Binary(data.into())).await.is_err() {
                                inner_for_writer.closed.store(true, Ordering::Release);
                                break;
                            }
                        }
                        OutMsg::Close => {
                            let _ = sink.send(AxumMessage::Close(None)).await;
                            inner_for_writer.closed.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            });

            let inner_for_reader = inner.clone();
            tokio::spawn(async move {
                while let Some(item) = stream.next().await {
                    let msg = match item {
                        Ok(msg) => msg,
                        Err(_) => {
                            inner_for_reader.closed.store(true, Ordering::Release);
                            break;
                        }
                    };

                    match msg {
                        AxumMessage::Binary(data) => {
                            // Keep the Bytes from axum - zero copy!
                            if recv_tx.send(data).await.is_err() {
                                break;
                            }
                        }
                        AxumMessage::Close(_) => {
                            inner_for_reader.closed.store(true, Ordering::Release);
                            break;
                        }
                        AxumMessage::Ping(_) | AxumMessage::Pong(_) | AxumMessage::Text(_) => {}
                    }
                }
            });

            Self { inner }
        }

        fn is_closed_inner(&self) -> bool {
            self.inner.closed.load(Ordering::Acquire)
        }
    }

    impl Transport for WebSocketTransport {
        async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
            if self.is_closed_inner() {
                return Err(TransportError::Closed);
            }

            let payload = frame.payload_bytes();
            let mut data = Vec::with_capacity(DESC_SIZE + payload.len());
            data.extend_from_slice(&desc_to_bytes(&frame.desc));
            data.extend_from_slice(payload);

            self.inner
                .send
                .send(OutMsg::Data(data))
                .await
                .map_err(|_| TransportError::Closed)?;
            Ok(())
        }

        async fn recv_frame(&self) -> Result<Frame, TransportError> {
            if self.is_closed_inner() {
                return Err(TransportError::Closed);
            }

            let mut recv = self.inner.recv.lock().await;
            let data = recv.recv().await.ok_or(TransportError::Closed)?;

            if data.len() < DESC_SIZE {
                return Err(TransportError::Io(
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("frame too small: {} < {}", data.len(), DESC_SIZE),
                    )
                    .into(),
                ));
            }

            let desc_bytes: [u8; DESC_SIZE] = data[..DESC_SIZE].try_into().unwrap();
            let mut desc = bytes_to_desc(&desc_bytes);

            let payload_len = data.len() - DESC_SIZE;
            desc.payload_len = payload_len as u32;

            if payload_len <= INLINE_PAYLOAD_SIZE {
                // Small payloads go inline (copy is fine for small data)
                desc.payload_slot = INLINE_PAYLOAD_SLOT;
                desc.payload_generation = 0;
                desc.payload_offset = 0;
                desc.inline_payload[..payload_len].copy_from_slice(&data[DESC_SIZE..]);
                return Ok(Frame {
                    desc,
                    payload: Payload::Inline,
                });
            }

            // Large payloads: zero-copy slice from the Bytes
            desc.payload_slot = 0;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            Ok(Frame {
                desc,
                payload: Payload::Bytes(data.slice(DESC_SIZE..)),
            })
        }

        fn close(&self) {
            self.inner.closed.store(true, Ordering::Release);
            let _ = self.inner.send.try_send(OutMsg::Close);
        }

        fn is_closed(&self) -> bool {
            self.is_closed_inner()
        }

        fn buffer_pool(&self) -> &crate::BufferPool {
            &self.inner.buffer_pool
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::WebSocketTransport;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{DESC_SIZE, bytes_to_desc, desc_to_bytes};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll};

    use gloo_timers::future::TimeoutFuture;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{BinaryType, CloseEvent, ErrorEvent, MessageEvent, WebSocket};

    use crate::{
        BufferPool, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, Payload, TransportError,
    };

    use super::super::Transport;

    pub struct WebSocketTransport {
        inner: Arc<WebSocketInner>,
    }

    struct WebSocketInner {
        ws: WasmWebSocket,
        closed: AtomicBool,
        buffer_pool: BufferPool,
    }

    impl Clone for WebSocketTransport {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }

    impl std::fmt::Debug for WebSocketTransport {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WebSocketTransport")
                .field("closed", &self.inner.closed.load(Ordering::Acquire))
                .finish_non_exhaustive()
        }
    }

    impl WebSocketTransport {
        pub async fn connect(url: &str) -> Result<Self, TransportError> {
            Self::connect_with_buffer_pool(url, BufferPool::new()).await
        }

        pub async fn connect_with_buffer_pool(
            url: &str,
            buffer_pool: BufferPool,
        ) -> Result<Self, TransportError> {
            let ws = WasmWebSocket::connect(url).await?;
            Ok(Self {
                inner: Arc::new(WebSocketInner {
                    ws,
                    closed: AtomicBool::new(false),
                    buffer_pool,
                }),
            })
        }

        fn is_closed_inner(&self) -> bool {
            self.inner.closed.load(Ordering::Acquire)
        }
    }

    impl Transport for WebSocketTransport {
        async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
            if self.is_closed_inner() {
                return Err(TransportError::Closed);
            }

            let payload = frame.payload_bytes();
            let mut data = Vec::with_capacity(DESC_SIZE + payload.len());
            data.extend_from_slice(&desc_to_bytes(&frame.desc));
            data.extend_from_slice(payload);

            self.inner.ws.send(&data)?;
            Ok(())
        }

        async fn recv_frame(&self) -> Result<Frame, TransportError> {
            if self.is_closed_inner() {
                return Err(TransportError::Closed);
            }

            let data = self.inner.ws.recv().await?;
            if data.len() < DESC_SIZE {
                return Err(TransportError::Io(
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("frame too small: {} < {}", data.len(), DESC_SIZE),
                    )
                    .into(),
                ));
            }

            let desc_bytes: [u8; DESC_SIZE] = data[..DESC_SIZE].try_into().unwrap();
            let mut desc = bytes_to_desc(&desc_bytes);

            let payload_slice = &data[DESC_SIZE..];
            let payload_len = payload_slice.len();
            desc.payload_len = payload_len as u32;

            if payload_len <= INLINE_PAYLOAD_SIZE {
                desc.payload_slot = INLINE_PAYLOAD_SLOT;
                desc.payload_generation = 0;
                desc.payload_offset = 0;
                desc.inline_payload[..payload_len].copy_from_slice(payload_slice);
                Ok(Frame {
                    desc,
                    payload: Payload::Inline,
                })
            } else {
                // Use pooled buffer for non-inline payloads
                let mut pooled_buf = self.inner.buffer_pool.get();
                pooled_buf.extend_from_slice(payload_slice);

                desc.payload_slot = 0;
                desc.payload_generation = 0;
                desc.payload_offset = 0;
                Ok(Frame {
                    desc,
                    payload: Payload::Pooled(pooled_buf),
                })
            }
        }

        fn close(&self) {
            self.inner.closed.store(true, Ordering::Release);
            self.inner.ws.close();
        }

        fn is_closed(&self) -> bool {
            self.is_closed_inner()
        }

        fn buffer_pool(&self) -> &crate::BufferPool {
            &self.inner.buffer_pool
        }
    }

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
        TransportError::Io(std::io::Error::other(msg).into())
    }

    fn js_error_from_msg<S: Into<String>>(msg: S) -> TransportError {
        TransportError::Io(std::io::Error::other(msg.into()).into())
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WebSocketTransport;
