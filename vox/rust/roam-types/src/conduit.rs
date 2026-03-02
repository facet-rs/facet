#![allow(async_fn_in_trait)]

use std::future::Future;

use facet::Facet;
use facet_core::Shape;

use crate::{MaybeSend, RpcPlan, SelfRef};

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

    fn rpc_plan() -> &'static RpcPlan {
        RpcPlan::for_shape(Self::shape())
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
/// Permit-based: `reserve()` is the backpressure point, `permit.send()`
/// serializes and writes.
// r[impl conduit.permit]
pub trait ConduitTx {
    type Msg: MsgFamily;
    type Permit<'a>: for<'m> ConduitTxPermit<Msg = Self::Msg> + MaybeSend
    where
        Self: 'a;

    /// Reserve capacity for one outbound message.
    ///
    /// Backpressure lives here — this may block waiting for:
    /// - StableConduit: replay buffer capacity (bounded outstanding)
    /// - Flow control from the peer
    ///
    /// Dropping the permit without sending releases the reservation.
    fn reserve(&self) -> impl Future<Output = std::io::Result<Self::Permit<'_>>> + MaybeSend + '_;

    /// Graceful close of the outbound direction.
    async fn close(self) -> std::io::Result<()>
    where
        Self: Sized;
}

/// Permit for sending exactly one message through a [`ConduitTx`].
// r[impl conduit.permit.send]
// r[impl zerocopy.framing.conduit]
pub trait ConduitTxPermit {
    type Msg: MsgFamily;
    type Error: std::error::Error + MaybeSend + 'static;

    fn send(self, item: <Self::Msg as MsgFamily>::Msg<'_>) -> Result<(), Self::Error>;
}

/// Receiving half of a [`Conduit`].
///
/// Yields decoded values as [`SelfRef<Msg<'static>>`](SelfRef) (value + backing storage).
/// Uses a precomputed `TypePlanCore` for fast plan-driven deserialization.
pub trait ConduitRx {
    type Msg: MsgFamily;
    type Error: std::error::Error + MaybeSend + 'static;

    /// Receive and decode the next message.
    ///
    /// Returns `Ok(None)` when the peer has closed.
    async fn recv(
        &mut self,
    ) -> Result<Option<SelfRef<<Self::Msg as MsgFamily>::Msg<'static>>>, Self::Error>;
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
