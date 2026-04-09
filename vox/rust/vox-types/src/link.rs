#![allow(async_fn_in_trait)]

use std::future::Future;

use crate::Backing;

/// Requested conduit mode for the transport prologue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    Bare,
    Stable,
}

/// Marker trait that requires [`Send`] on native targets, nothing on wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}

#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}

/// Marker trait that requires [`Sync`] on native targets, nothing on wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSync: Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync> MaybeSync for T {}

#[cfg(target_arch = "wasm32")]
pub trait MaybeSync {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSync for T {}

/// A future that is `Send` on native targets, nothing on wasm32.
/// Unlike `MaybeSend`, this can be used as `dyn MaybeSendFuture` because
/// it's a single trait (not `dyn Future + MaybeSend`).
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSendFuture: Future + Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Future + Send> MaybeSendFuture for T {}

#[cfg(target_arch = "wasm32")]
pub trait MaybeSendFuture: Future {}
#[cfg(target_arch = "wasm32")]
impl<T: Future> MaybeSendFuture for T {}

/// Bidirectional raw-bytes transport.
///
/// TCP, WebSocket, SHM all implement this. No knowledge of what's being
/// sent — just bytes in, bytes out. The transport provides write buffers
/// so callers can encode directly into the destination (zero-copy for SHM).
// r[impl link]
// r[impl link.message]
// r[impl link.order]
pub trait Link {
    type Tx: LinkTx;
    type Rx: LinkRx;

    // r[impl link.split]
    fn split(self) -> (Self::Tx, Self::Rx);

    /// Whether this link supports the requested transport mode.
    ///
    /// Most links support both `bare` and `stable`. Special transports may
    /// override this to reject unsupported modes during the transport
    /// prologue.
    fn supports_transport_mode(mode: TransportMode) -> bool
    where
        Self: Sized,
    {
        let _ = mode;
        true
    }
}

/// Sending half of a [`Link`].
///
/// Callers provide an owned payload buffer; the transport applies any framing
/// and enqueues or writes it asynchronously. Backpressure lives in [`LinkTx::send`].
pub trait LinkTx: MaybeSend + MaybeSync + 'static {
    /// Send one payload.
    ///
    /// The `Vec<u8>` is caller-owned until the transport accepts it into its
    /// internal queue or write path.
    fn send(&self, bytes: Vec<u8>) -> impl Future<Output = std::io::Result<()>> + MaybeSend + '_;

    /// Graceful close of the outbound direction.
    // r[impl link.tx.close]
    fn close(self) -> impl Future<Output = std::io::Result<()>> + MaybeSend
    where
        Self: Sized;
}

/// Receiving half of a [`Link`].
///
/// Yields [`Backing`] values: the raw bytes plus their ownership handle.
/// The transport handles framing (length-prefix, WebSocket frames, etc.)
/// and returns exactly one message's bytes per `recv` call.
///
/// For SHM: the Backing might be a VarSlot reference.
/// For TCP: the Backing is a heap-allocated buffer.
pub trait LinkRx: MaybeSend + 'static {
    type Error: std::error::Error + MaybeSend + MaybeSync + 'static;

    /// Receive the next message's raw bytes.
    ///
    /// Returns `Ok(None)` when the peer has closed the connection.
    // r[impl link.rx.recv]
    // r[impl link.rx.error]
    // r[impl link.rx.eof]
    fn recv(
        &mut self,
    ) -> impl Future<Output = Result<Option<Backing>, Self::Error>> + MaybeSend + '_;
}

/// A [`Link`] assembled from pre-split Tx and Rx halves.
pub struct SplitLink<Tx, Rx> {
    pub tx: Tx,
    pub rx: Rx,
}

impl<Tx: LinkTx, Rx: LinkRx> Link for SplitLink<Tx, Rx> {
    type Tx = Tx;
    type Rx = Rx;

    fn split(self) -> (Tx, Rx) {
        (self.tx, self.rx)
    }
}
