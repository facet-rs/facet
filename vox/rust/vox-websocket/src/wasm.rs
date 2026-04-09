//! WASM (web_sys) WebSocket transport implementing [`Link`].

use std::cell::RefCell;
use std::io;

use futures_channel::mpsc;
use futures_util::{StreamExt, lock::Mutex};
use js_sys::ArrayBuffer;
use vox_types::{Backing, Link, LinkRx, LinkTx};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, ErrorEvent, MessageEvent, WebSocket};

enum WsEvent {
    Message(Vec<u8>),
    Close,
    Error(String),
}

struct WsClosures {
    _onmessage: Closure<dyn FnMut(MessageEvent)>,
    _onclose: Closure<dyn FnMut(CloseEvent)>,
    _onerror: Closure<dyn FnMut(ErrorEvent)>,
}

struct ScratchBuffer {
    buf_tx: mpsc::Sender<Vec<u8>>,
    buf: Option<Vec<u8>>,
}

impl ScratchBuffer {
    fn new(buf_tx: mpsc::Sender<Vec<u8>>, buf: Vec<u8>) -> Self {
        Self {
            buf_tx,
            buf: Some(buf),
        }
    }
}

impl std::ops::Deref for ScratchBuffer {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        self.buf
            .as_ref()
            .expect("scratch buffer should exist while borrowed")
    }
}

impl std::ops::DerefMut for ScratchBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buf
            .as_mut()
            .expect("scratch buffer should exist while mutably borrowed")
    }
}

impl Drop for ScratchBuffer {
    fn drop(&mut self) {
        if let Some(mut buf) = self.buf.take() {
            buf.clear();
            let _ = self.buf_tx.clone().try_send(buf);
        }
    }
}

/// A [`Link`] over a browser WebSocket.
// r[impl transport.websocket]
// r[impl transport.websocket.platforms]
// r[impl zerocopy.framing.link.websocket]
pub struct WsLink(WsLinkTx, WsLinkRx);

impl WsLink {
    /// Wrap an already-open [`WebSocket`].
    pub fn new(ws: WebSocket) -> Self {
        ws.set_binary_type(BinaryType::Arraybuffer);

        let (tx, rx) = mpsc::unbounded();

        let tx_msg = tx.clone();
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(abuf) = e.data().dyn_into::<ArrayBuffer>() {
                let array = js_sys::Uint8Array::new(&abuf);
                let _ = tx_msg.unbounded_send(WsEvent::Message(array.to_vec()));
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

        let tx_close = tx.clone();
        let onclose = Closure::wrap(Box::new(move |_: CloseEvent| {
            let _ = tx_close.unbounded_send(WsEvent::Close);
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        let tx_error = tx;
        let onerror = Closure::wrap(Box::new(move |e: ErrorEvent| {
            let _ = tx_error.unbounded_send(WsEvent::Error(e.message()));
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        let (buf_tx, buf_rx) = mpsc::channel::<Vec<u8>>(1);
        buf_tx.clone().try_send(Vec::new()).ok();

        Self(
            WsLinkTx {
                ws,
                buf_tx,
                buf_rx: Mutex::new(buf_rx),
            },
            WsLinkRx {
                rx,
                _closures: WsClosures {
                    _onmessage: onmessage,
                    _onclose: onclose,
                    _onerror: onerror,
                },
            },
        )
    }

    /// Connect to `url`, waiting until the WebSocket is open.
    pub async fn connect(url: &str) -> io::Result<Self> {
        use futures_channel::oneshot;
        use std::rc::Rc;

        let ws = WebSocket::new(url)
            .map_err(|e| io::Error::other(format!("WebSocket::new failed: {e:?}")))?;

        let (open_tx, open_rx) = oneshot::channel::<Result<(), String>>();
        let open_tx = Rc::new(RefCell::new(Some(open_tx)));

        let tx1 = open_tx.clone();
        let onopen = Closure::once(Box::new(move || {
            if let Some(tx) = tx1.borrow_mut().take() {
                let _ = tx.send(Ok(()));
            }
        }) as Box<dyn FnOnce()>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));

        let tx2 = open_tx;
        let onerror = Closure::once(Box::new(move |e: ErrorEvent| {
            if let Some(tx) = tx2.borrow_mut().take() {
                let _ = tx.send(Err(e.message()));
            }
        }) as Box<dyn FnOnce(ErrorEvent)>);
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        open_rx
            .await
            .map_err(|_| io::Error::other("connection cancelled"))?
            .map_err(|e| io::Error::other(format!("WebSocket open failed: {e}")))?;

        ws.set_onopen(None);
        ws.set_onerror(None);
        drop(onopen);
        drop(onerror);

        Ok(Self::new(ws))
    }
}

// ---------------------------------------------------------------------------
// Link split
// ---------------------------------------------------------------------------

impl Link for WsLink {
    type Tx = WsLinkTx;
    type Rx = WsLinkRx;

    fn split(self) -> (WsLinkTx, WsLinkRx) {
        (self.0, self.1)
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

/// Sending half of a [`WsLink`].
pub struct WsLinkTx {
    ws: WebSocket,
    buf_tx: mpsc::Sender<Vec<u8>>,
    buf_rx: Mutex<mpsc::Receiver<Vec<u8>>>,
}

impl LinkTx for WsLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> io::Result<()> {
        let scratch = {
            let mut buf_rx = self.buf_rx.lock().await;
            buf_rx.next().await
        }
        .ok_or_else(|| io::Error::other("ws send buffer channel closed"))?;
        let mut scratch = ScratchBuffer::new(self.buf_tx.clone(), scratch);
        scratch.clear();
        scratch.extend_from_slice(&bytes);

        // Copy into a JS-owned typed array before recycling the Rust buffer.
        let payload = js_sys::Uint8Array::from(scratch.as_slice());
        self.ws
            .send_with_array_buffer(&payload.buffer())
            .map_err(|e| io::Error::other(format!("ws send failed: {e:?}")))?;
        Ok(())
    }

    async fn close(self) -> io::Result<()> {
        self.ws
            .close()
            .map_err(|e| io::Error::other(format!("ws close failed: {e:?}")))
    }
}

impl Drop for WsLinkTx {
    fn drop(&mut self) {
        let _ = self.ws.close();
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

/// Receiving half of a [`WsLink`].
pub struct WsLinkRx {
    rx: mpsc::UnboundedReceiver<WsEvent>,
    _closures: WsClosures,
}

/// Error type for [`WsLinkRx`].
#[derive(Debug)]
pub struct WsLinkRxError(String);

impl std::fmt::Display for WsLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WsLinkRxError {}

// r[impl zerocopy.recv.websocket]
impl LinkRx for WsLinkRx {
    type Error = WsLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, WsLinkRxError> {
        loop {
            match self.rx.next().await {
                Some(WsEvent::Message(data)) => {
                    return Ok(Some(Backing::Boxed(data.into_boxed_slice())));
                }
                Some(WsEvent::Close) | None => {
                    return Ok(None);
                }
                Some(WsEvent::Error(e)) => {
                    return Err(WsLinkRxError(format!("WebSocket error: {e}")));
                }
            }
        }
    }
}
