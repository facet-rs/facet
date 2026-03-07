//! WASM in-process transport implementing [`Link`].

use std::cell::RefCell;
use std::io;
use std::mem::ManuallyDrop;

use futures_channel::mpsc;
use futures_util::StreamExt;
use roam_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, WriteSlot};
use wasm_bindgen::prelude::*;

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
                    buf_rx: RefCell::new(buf_rx),
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
    /// Returned here after each send to be reused by the next permit.
    buf_tx: mpsc::Sender<Vec<u8>>,
    /// Awaited to obtain the reusable buffer (and provide backpressure).
    /// RefCell is safe: wasm is single-threaded, MaybeSync removes Sync bound.
    buf_rx: RefCell<mpsc::Receiver<Vec<u8>>>,
}

/// Permit for one outbound send.
///
/// Uses `ManuallyDrop` for its fields so that `alloc` can move them out
/// into `InProcessWriteSlot` without conflicting with the `Drop` impl.
pub struct InProcessLinkTxPermit {
    on_message: ManuallyDrop<js_sys::Function>,
    buf: ManuallyDrop<Vec<u8>>,
    buf_tx: ManuallyDrop<mpsc::Sender<Vec<u8>>>,
    consumed: bool,
}

/// Write slot backed by the reusable send buffer.
// r[impl zerocopy.send.inprocess]
pub struct InProcessWriteSlot {
    on_message: js_sys::Function,
    buf: Vec<u8>,
    buf_tx: mpsc::Sender<Vec<u8>>,
    committed: bool,
}

impl LinkTx for InProcessLinkTx {
    type Permit = InProcessLinkTxPermit;

    async fn reserve(&self) -> io::Result<Self::Permit> {
        let buf = self
            .buf_rx
            .borrow_mut()
            .next()
            .await
            .ok_or_else(|| io::Error::other("in-process send buffer channel closed"))?;
        Ok(InProcessLinkTxPermit {
            on_message: ManuallyDrop::new(self.on_message.clone()),
            buf: ManuallyDrop::new(buf),
            buf_tx: ManuallyDrop::new(self.buf_tx.clone()),
            consumed: false,
        })
    }

    async fn close(self) -> io::Result<()> {
        Ok(())
    }
}

impl LinkTxPermit for InProcessLinkTxPermit {
    type Slot = InProcessWriteSlot;

    fn alloc(mut self, len: usize) -> io::Result<InProcessWriteSlot> {
        self.consumed = true;
        // SAFETY: we set `consumed`, so Drop will not touch these fields.
        let on_message = unsafe { ManuallyDrop::take(&mut self.on_message) };
        let mut buf = unsafe { ManuallyDrop::take(&mut self.buf) };
        let buf_tx = unsafe { ManuallyDrop::take(&mut self.buf_tx) };
        buf.clear();
        buf.resize(len, 0);
        Ok(InProcessWriteSlot {
            on_message,
            buf,
            buf_tx,
            committed: false,
        })
    }
}

impl Drop for InProcessLinkTxPermit {
    fn drop(&mut self) {
        if self.consumed {
            return;
        }
        // SAFETY: not consumed, so fields are still valid.
        unsafe {
            let buf = ManuallyDrop::take(&mut self.buf);
            let _ = self.buf_tx.try_send(buf);
            ManuallyDrop::drop(&mut self.on_message);
            ManuallyDrop::drop(&mut self.buf_tx);
        }
    }
}

impl WriteSlot for InProcessWriteSlot {
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn commit(mut self) {
        self.committed = true;
        let array = js_sys::Uint8Array::from(self.buf.as_slice());
        let _ = self.on_message.call1(&JsValue::NULL, &array);
        self.buf.clear();
        let _ = self.buf_tx.try_send(std::mem::take(&mut self.buf));
    }
}

impl Drop for InProcessWriteSlot {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let _ = self.buf_tx.try_send(std::mem::take(&mut self.buf));
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
