#![allow(async_fn_in_trait)]

use std::future::Future;

use crate::Backing;

/// Requested conduit mode for the transport prologue.
///
/// Historically this enum had a `Stable` variant for the reconnect /
/// replay-buffer-backed `StableConduit`; that conduit shape was removed,
/// leaving only `Bare`. The enum is preserved for now so the wire-level
/// transport prologue remains backwards-compatible with peers that still
/// negotiate it; new transports always select `Bare`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    Bare,
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

    /// Whether this transport can carry file descriptors (`SCM_RIGHTS` over a
    /// Unix-domain socket). Only such links may carry [`Fd`](crate::fd::Fd)
    /// values; everything else (TCP, WebSocket, wasm) returns `false` and the
    /// encoder refuses fd-bearing messages.
    fn supports_fd_passing(&self) -> bool {
        false
    }

    /// Send one payload that carries `fds` out-of-band via `SCM_RIGHTS`.
    ///
    /// The default errors if any fds are present (a transport that cannot
    /// pass descriptors must never be handed one); with no fds it is exactly
    /// [`send`](Self::send), so existing transports need no change. Off-Unix
    /// [`FrameFds`](crate::FrameFds) is `()` and this is always plain
    /// [`send`](Self::send).
    fn send_with_fds(
        &self,
        bytes: Vec<u8>,
        fds: crate::FrameFds,
    ) -> impl Future<Output = std::io::Result<()>> + MaybeSend + '_ {
        async move {
            #[cfg(unix)]
            if !fds.is_empty() {
                return Err(std::io::Error::other(
                    "transport does not support fd passing",
                ));
            }
            let _ = &fds;
            self.send(bytes).await
        }
    }
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

    /// Take the file descriptors that arrived with the frame returned by the
    /// most recent [`recv`](Self::recv).
    ///
    /// Descriptors travel out-of-band in `SCM_RIGHTS`; an fd-capable link
    /// attributes each batch to the frame whose bytes completed it (one
    /// fd-bearing frame == one `sendmsg`). The default returns none, so
    /// non-fd transports need no change. The conduit threads these to the
    /// typed-decode site as the [`provide_fds`](crate::provide_fds) source.
    fn take_frame_fds(&mut self) -> crate::FrameFds {
        crate::FrameFds::default()
    }
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
