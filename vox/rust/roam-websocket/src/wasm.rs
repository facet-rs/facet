//! WASM (web_sys) WebSocket transport.

use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::time::Duration;

use futures_util::StreamExt;
use roam_session::MessageTransport;
use roam_wire::Message;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, ErrorEvent, MessageEvent, WebSocket};

/// WebSocket transport for roam messages (WASM implementation).
///
/// Wraps a browser [`WebSocket`] and implements [`MessageTransport`].
/// Messages are postcard-encoded and sent as binary WebSocket frames.
pub struct WsTransport {
    ws: WebSocket,
    /// Receiver for incoming messages and events.
    rx: futures_channel::mpsc::UnboundedReceiver<WsEvent>,
    /// Keep closures alive.
    _closures: WsClosures,
    /// Last decoded bytes for error detection.
    last_decoded: Vec<u8>,
}

/// Internal events from WebSocket callbacks.
enum WsEvent {
    Message(Vec<u8>),
    Close,
    Error(String),
}

/// Closures that need to stay alive for the WebSocket callbacks.
struct WsClosures {
    _onmessage: Closure<dyn FnMut(MessageEvent)>,
    _onclose: Closure<dyn FnMut(CloseEvent)>,
    _onerror: Closure<dyn FnMut(ErrorEvent)>,
}

impl WsTransport {
    /// Create a new WebSocket transport from an existing WebSocket.
    ///
    /// The WebSocket should be in the OPEN state or about to open.
    /// This constructor sets up the necessary callbacks.
    pub fn new(ws: WebSocket) -> Self {
        // Ensure we receive binary data as ArrayBuffer
        ws.set_binary_type(BinaryType::Arraybuffer);

        let (tx, rx) = futures_channel::mpsc::unbounded();

        // Set up message handler
        let tx_msg = tx.clone();
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                let array = js_sys::Uint8Array::new(&abuf);
                let data = array.to_vec();
                let _ = tx_msg.unbounded_send(WsEvent::Message(data));
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

        // Set up close handler
        let tx_close = tx.clone();
        let onclose = Closure::wrap(Box::new(move |_: CloseEvent| {
            let _ = tx_close.unbounded_send(WsEvent::Close);
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        // Set up error handler
        let tx_error = tx;
        let onerror = Closure::wrap(Box::new(move |e: ErrorEvent| {
            let msg = e.message();
            let _ = tx_error.unbounded_send(WsEvent::Error(msg));
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        Self {
            ws,
            rx,
            _closures: WsClosures {
                _onmessage: onmessage,
                _onclose: onclose,
                _onerror: onerror,
            },
            last_decoded: Vec::new(),
        }
    }

    /// Create a new WebSocket connection to the given URL.
    ///
    /// Returns the transport once the connection is established.
    pub async fn connect(url: &str) -> io::Result<Self> {
        let ws = WebSocket::new(url)
            .map_err(|e| io::Error::other(format!("failed to create WebSocket: {e:?}")))?;

        // Set up temporary open/error handlers to wait for connection
        let (open_tx, open_rx) = futures_channel::oneshot::channel::<Result<(), String>>();
        let open_tx = Rc::new(RefCell::new(Some(open_tx)));

        let open_tx_clone = open_tx.clone();
        let onopen = Closure::once(Box::new(move || {
            if let Some(tx) = open_tx_clone.borrow_mut().take() {
                let _ = tx.send(Ok(()));
            }
        }) as Box<dyn FnOnce()>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));

        let open_tx_clone = open_tx.clone();
        let onerror_temp = Closure::once(Box::new(move |e: ErrorEvent| {
            if let Some(tx) = open_tx_clone.borrow_mut().take() {
                let _ = tx.send(Err(e.message()));
            }
        }) as Box<dyn FnOnce(ErrorEvent)>);
        ws.set_onerror(Some(onerror_temp.as_ref().unchecked_ref()));

        // Wait for connection
        let result = open_rx
            .await
            .map_err(|_| io::Error::other("connection cancelled"))?;

        // Clear temporary handlers
        ws.set_onopen(None);
        ws.set_onerror(None);

        // Keep closures alive until connection completes
        drop(onopen);
        drop(onerror_temp);

        result.map_err(|e| io::Error::other(format!("connection failed: {e}")))?;

        Ok(Self::new(ws))
    }

    /// Get a reference to the underlying WebSocket.
    pub fn websocket(&self) -> &WebSocket {
        &self.ws
    }

    /// Close the WebSocket connection.
    pub fn close(&self) -> io::Result<()> {
        self.ws
            .close()
            .map_err(|e| io::Error::other(format!("close failed: {e:?}")))
    }
}

impl MessageTransport for WsTransport {
    /// Send a message over WebSocket.
    ///
    /// r[impl transport.message.binary] - Send as binary frame.
    /// r[impl transport.message.one-to-one] - One message per frame.
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        self.ws
            .send_with_u8_array(&payload)
            .map_err(|e| io::Error::other(format!("send failed: {e:?}")))?;

        Ok(())
    }

    /// Receive a message with timeout.
    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        roam_session::runtime::timeout(timeout, self.recv())
            .await
            .unwrap_or(Ok(None))
    }

    /// Receive a message (blocking until one arrives or connection closes).
    async fn recv(&mut self) -> io::Result<Option<Message>> {
        loop {
            match self.rx.next().await {
                Some(WsEvent::Message(data)) => {
                    // r[impl transport.message.binary] - Process binary frames.
                    self.last_decoded = data.clone();
                    let msg: Message = facet_postcard::from_slice(&data).map_err(|e| {
                        // Log the failed bytes for debugging
                        web_sys::console::error_1(
                            &format!(
                                "postcard decode failed: {e}, bytes ({} total): {:?}",
                                data.len(),
                                &data[..data.len().min(100)]
                            )
                            .into(),
                        );
                        io::Error::new(io::ErrorKind::InvalidData, format!("postcard: {e}"))
                    })?;
                    return Ok(Some(msg));
                }
                Some(WsEvent::Close) => {
                    return Ok(None);
                }
                Some(WsEvent::Error(e)) => {
                    return Err(io::Error::other(format!("WebSocket error: {e}")));
                }
                None => {
                    // Channel closed (shouldn't happen normally)
                    return Ok(None);
                }
            }
        }
    }

    fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

impl Drop for WsTransport {
    fn drop(&mut self) {
        // Clear handlers before dropping
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        self.ws.set_onerror(None);
        // Best-effort close
        let _ = self.ws.close();
    }
}
