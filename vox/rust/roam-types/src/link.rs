#![allow(async_fn_in_trait)]

use std::future::Future;

use crate::Backing;

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
}

/// A permit for allocating exactly one outbound payload.
///
/// Returned by [`LinkTx::reserve`]. The permit represents *message-level*
/// capacity (not bytes). Once you have a permit, turning it into a concrete
/// buffer for a specific payload size is synchronous.
// r[impl link.tx.permit.drop]
pub trait LinkTxPermit {
    type Slot: WriteSlot;

    /// Allocate a writable buffer of exactly `len` bytes.
    ///
    /// This is synchronous once the permit has been acquired.
    // r[impl link.tx.alloc.limits]
    // r[impl link.message.empty]
    fn alloc(self, len: usize) -> std::io::Result<Self::Slot>;
}

/// Sending half of a [`Link`].
///
/// Uses a two-phase write:
///
/// 1. [`reserve`](LinkTx::reserve) awaits until the transport can accept *one*
///    more payload and returns a [`LinkTxPermit`].
/// 2. [`LinkTxPermit::alloc`] allocates a [`WriteSlot`] backed by the
///    transport's own buffer (bipbuffer slot, kernel write buffer, etc.),
///    then the caller fills it and calls [`WriteSlot::commit`].
///
/// `reserve` is the backpressure point.
pub trait LinkTx: MaybeSend + MaybeSync + 'static {
    type Permit: LinkTxPermit + MaybeSend;

    /// Reserve capacity to send exactly one payload.
    ///
    /// Backpressure lives here — it awaits until the transport can accept a
    /// payload (or errors).
    ///
    /// Dropping the returned permit without allocating/committing MUST
    /// release the reservation.
    // r[impl link.tx.reserve]
    // r[impl link.tx.cancel-safe]
    fn reserve(&self) -> impl Future<Output = std::io::Result<Self::Permit>> + MaybeSend + '_;

    /// Graceful close of the outbound direction.
    // r[impl link.tx.close]
    fn close(self) -> impl Future<Output = std::io::Result<()>> + MaybeSend
    where
        Self: Sized;
}

/// A writable slot in the transport's output buffer.
///
/// Obtained from [`LinkTxPermit::alloc`]. The caller writes encoded bytes into
/// [`as_mut_slice`](WriteSlot::as_mut_slice), then calls
/// [`commit`](WriteSlot::commit) to make them visible to the receiver.
///
/// Dropping without commit = discard (no bytes sent, space reclaimed).
// r[impl link.tx.discard]
// r[impl zerocopy.framing.link]
pub trait WriteSlot {
    /// The writable buffer, exactly the size requested in `alloc`.
    // r[impl link.tx.slot.len]
    fn as_mut_slice(&mut self) -> &mut [u8];

    /// Commit the written bytes. After this, the receiver can see them.
    /// Sync — the bytes are already in the transport's buffer.
    // r[impl link.tx.commit]
    fn commit(self);
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
