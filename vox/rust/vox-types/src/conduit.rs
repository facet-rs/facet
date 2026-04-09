#![allow(async_fn_in_trait)]

use std::future::Future;

use facet::Facet;
use facet_core::Shape;

use crate::{MaybeSend, SelfRef};

/// Maps a lifetime to a concrete message type.
///
/// Rust doesn't have higher-kinded types, so this trait bridges the gap:
/// `F::Msg<'a>` gives you the message type for any lifetime `'a`.
///
/// The send path uses `Msg<'a>` (borrowed data serialized in place).
/// The recv path uses `Msg<'static>` (owned, via `SelfRef`).
pub trait MsgFamily: 'static {
    type Msg<'a>: Facet<'a> + 'a;

    fn shape() -> &'static Shape {
        <Self::Msg<'static> as Facet<'static>>::SHAPE
    }
}

/// Bidirectional typed transport. Wraps a [`Link`](crate::Link) and owns serialization.
///
/// Uses a `MsgFamily` so that the same type family serves both sides:
/// - Send: `MsgFamily::Msg<'a>` for any `'a` (borrowed data serialized in place)
/// - Recv: `MsgFamily::Msg<'static>` (owned, via `SelfRef`)
///
/// Two implementations:
/// - `BareConduit`: Link + postcard. If the link dies, it's dead.
/// - `StableConduit`: Link + postcard + seq/ack/replay. Handles reconnect
///   transparently. Replay buffer stores encoded bytes (no clone needed).
// r[impl conduit]
pub trait Conduit {
    type Msg: MsgFamily;
    type Tx: ConduitTx<Msg = Self::Msg>;
    type Rx: ConduitRx<Msg = Self::Msg>;

    // r[impl conduit.split]
    fn split(self) -> (Self::Tx, Self::Rx);
}

/// Sending half of a [`Conduit`].
///
/// Sending is split into a synchronous preparation phase and an async enqueue
/// phase. Preparation may borrow from the input value, but it must produce an
/// owned representation that survives across the first await point.
pub trait ConduitTx {
    type Msg: MsgFamily;
    type Prepared: MaybeSend + 'static;
    type Error: std::error::Error + MaybeSend + 'static;

    /// Serialize one outbound message into an owned representation.
    fn prepare_send(
        &self,
        item: <Self::Msg as MsgFamily>::Msg<'_>,
    ) -> Result<Self::Prepared, Self::Error>;

    /// Enqueue a previously prepared outbound message.
    fn send_prepared(
        &self,
        prepared: Self::Prepared,
    ) -> impl Future<Output = Result<(), Self::Error>> + MaybeSend + '_;

    /// Graceful close of the outbound direction.
    async fn close(self) -> std::io::Result<()>
    where
        Self: Sized;
}

/// Receiving half of a [`Conduit`].
///
/// Yields decoded values as [`SelfRef<Msg<'static>>`](SelfRef) (value + backing storage).
/// Uses a precomputed `TypePlanCore` for fast plan-driven deserialization.
/// The result of receiving a message from a conduit.
pub type RecvResult<M, E> = Result<Option<SelfRef<<M as MsgFamily>::Msg<'static>>>, E>;

pub trait ConduitRx {
    type Msg: MsgFamily;
    type Error: std::error::Error + MaybeSend + 'static;

    /// Receive and decode the next message.
    ///
    /// Returns `Ok(None)` when the peer has closed.
    fn recv(&mut self)
    -> impl Future<Output = RecvResult<Self::Msg, Self::Error>> + MaybeSend + '_;
}

/// Yields new conduits from inbound connections.
pub trait ConduitAcceptor {
    type Conduit: Conduit;

    async fn accept(&mut self) -> std::io::Result<Self::Conduit>;
}

/// Whether the session is acting as initiator or acceptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    Initiator,
    Acceptor,
}
