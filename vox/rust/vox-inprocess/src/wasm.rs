//! WASM in-process transport implementing [`Link`].

use std::io;

use futures_channel::mpsc;
use futures_util::{StreamExt, lock::Mutex};
use vox_types::{Backing, Link, LinkRx, LinkTx};
use wasm_bindgen::prelude::*;

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

/// JS-visible handle for an in-process link.
///
/// Created by JS, holds the Rust side of the bidirectional channel.
/// JS calls `deliver()` to send bytes into Rust, and the `on_message`
/// callback is invoked when Rust sends bytes to JS.
#[wasm_bindgen]
pub struct JsInProcessLink {
    /// Sender for JS→Rust messages.
    tx: mpsc::UnboundedSender<Vec<u8>>,
    /// The actual Link, taken once by Rust via `take_link()`.
    link: Option<InProcessLink>,
}

#[wasm_bindgen]
impl JsInProcessLink {
    /// Create a new in-process link.
    ///
    /// `on_message` is called with a `Uint8Array` each time Rust sends a message to JS.
    #[wasm_bindgen(constructor)]
    pub fn new(on_message: js_sys::Function) -> Self {
        let (rust_tx, rust_rx) = mpsc::unbounded::<Vec<u8>>();

        let (buf_tx, buf_rx) = mpsc::channel::<Vec<u8>>(1);
        buf_tx.clone().try_send(Vec::new()).ok();

        Self {
            tx: rust_tx,
            link: Some(InProcessLink(
                InProcessLinkTx {
                    on_message,
                    buf_tx,
                    buf_rx: Mutex::new(buf_rx),
                },
                InProcessLinkRx { rx: rust_rx },
            )),
        }
    }

    /// JS → Rust: push a message payload into the Rust receive channel.
    pub fn deliver(&self, payload: &[u8]) {
        let _ = self.tx.unbounded_send(payload.to_vec());
    }

    /// Signal that JS is done sending (EOF).
    pub fn close(&self) {
        self.tx.close_channel();
    }
}

impl JsInProcessLink {
    /// Extract the [`InProcessLink`] for use on the Rust side.
    ///
    /// Can only be called once — returns `None` on subsequent calls.
    pub fn take_link(&mut self) -> Option<InProcessLink> {
        self.link.take()
    }
}

// ---------------------------------------------------------------------------
// Link
// ---------------------------------------------------------------------------

/// An in-process [`Link`] for WASM ↔ JS communication.
// r[impl transport.inprocess]
// r[impl transport.inprocess.platforms]
// r[impl zerocopy.framing.link.inprocess]
pub struct InProcessLink(InProcessLinkTx, InProcessLinkRx);

impl Link for InProcessLink {
    type Tx = InProcessLinkTx;
    type Rx = InProcessLinkRx;

    fn split(self) -> (InProcessLinkTx, InProcessLinkRx) {
        (self.0, self.1)
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

/// Sending half of an [`InProcessLink`].
pub struct InProcessLinkTx {
    on_message: js_sys::Function,
    buf_tx: mpsc::Sender<Vec<u8>>,
    buf_rx: Mutex<mpsc::Receiver<Vec<u8>>>,
}

impl LinkTx for InProcessLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> io::Result<()> {
        let scratch = {
            let mut buf_rx = self.buf_rx.lock().await;
            buf_rx.next().await
        }
        .ok_or_else(|| io::Error::other("in-process send buffer channel closed"))?;
        let mut scratch = ScratchBuffer::new(self.buf_tx.clone(), scratch);
        scratch.clear();
        scratch.extend_from_slice(&bytes);
        let array = js_sys::Uint8Array::from(scratch.as_slice());
        self.on_message
            .call1(&JsValue::NULL, &array)
            .map_err(|e| io::Error::other(format!("in-process send failed: {e:?}")))?;
        Ok(())
    }

    async fn close(self) -> io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

/// Receiving half of an [`InProcessLink`].
pub struct InProcessLinkRx {
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

#[derive(Debug)]
pub struct InProcessLinkRxError;

impl std::fmt::Display for InProcessLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "in-process link rx error (unreachable)")
    }
}

impl std::error::Error for InProcessLinkRxError {}

// r[impl zerocopy.recv.inprocess]
impl LinkRx for InProcessLinkRx {
    type Error = InProcessLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, InProcessLinkRxError> {
        match self.rx.next().await {
            Some(data) => Ok(Some(Backing::Boxed(data.into_boxed_slice()))),
            None => Ok(None),
        }
    }
}
