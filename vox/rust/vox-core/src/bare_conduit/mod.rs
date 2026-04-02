use std::marker::PhantomData;

use facet_core::{PtrConst, Shape};
use facet_reflect::Peek;

use vox_types::{
    Conduit, ConduitRx, ConduitTx, ConduitTxPermit, Link, LinkTx, LinkTxPermit, MaybeSend,
    MsgFamily, SelfRef, WriteSlot,
};

use crate::MessagePlan;

/// Wraps a [`Link`] with postcard serialization. No reconnect, no reliability.
///
/// If the link dies, the conduit is dead. For localhost, SHM, or any
/// transport where reconnect isn't needed.
///
/// `F` is a [`MsgFamily`] — it maps lifetimes to concrete message types.
/// The send path accepts `F::Msg<'a>` (borrowed data serialized in place
/// via `Peek`). The recv path yields `SelfRef<F::Msg<'static>>` (owned).
// r[impl conduit.bare]
// r[impl conduit.typeplan]
// r[impl zerocopy.framing.conduit.bare]
pub struct BareConduit<F: MsgFamily, L: Link> {
    link: L,
    shape: &'static Shape,
    message_plan: Option<MessagePlan>,
    _phantom: PhantomData<fn(F) -> F>,
}

impl<F: MsgFamily, L: Link> BareConduit<F, L> {
    /// Create a new BareConduit (identity plan — no schema translation).
    pub fn new(link: L) -> Self {
        Self {
            link,
            shape: F::shape(),
            message_plan: None,
            _phantom: PhantomData,
        }
    }

    /// Create a new BareConduit with a pre-built message translation plan.
    pub fn with_message_plan(link: L, message_plan: MessagePlan) -> Self {
        Self {
            link,
            shape: F::shape(),
            message_plan: Some(message_plan),
            _phantom: PhantomData,
        }
    }
}

impl<F: MsgFamily, L: Link> Conduit for BareConduit<F, L>
where
    L::Tx: MaybeSend + 'static,
    L::Rx: MaybeSend + 'static,
{
    type Msg = F;
    type Tx = BareConduitTx<F, L::Tx>;
    type Rx = BareConduitRx<F, L::Rx>;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (tx, rx) = self.link.split();
        (
            BareConduitTx {
                link_tx: tx,
                shape: self.shape,
                _phantom: PhantomData,
            },
            BareConduitRx {
                link_rx: rx,
                message_plan: self.message_plan,
                _phantom: PhantomData,
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

pub struct BareConduitTx<F: MsgFamily, LTx: LinkTx> {
    link_tx: LTx,
    shape: &'static Shape,
    _phantom: PhantomData<fn(F)>,
}

impl<F: MsgFamily, LTx: LinkTx + MaybeSend + 'static> ConduitTx for BareConduitTx<F, LTx> {
    type Msg = F;
    type Permit<'a>
        = BareConduitPermit<'a, F, LTx>
    where
        Self: 'a;

    async fn reserve(&self) -> std::io::Result<Self::Permit<'_>> {
        let permit = self.link_tx.reserve().await?;
        Ok(BareConduitPermit {
            permit,
            shape: self.shape,
            _phantom: PhantomData,
        })
    }

    async fn close(self) -> std::io::Result<()> {
        self.link_tx.close().await
    }
}

// ---------------------------------------------------------------------------
// Permit
// ---------------------------------------------------------------------------

pub struct BareConduitPermit<'a, F: MsgFamily, LTx: LinkTx> {
    permit: LTx::Permit,
    shape: &'static Shape,
    _phantom: PhantomData<fn(F, &'a ())>,
}

impl<F: MsgFamily, LTx: LinkTx> ConduitTxPermit for BareConduitPermit<'_, F, LTx> {
    type Msg = F;
    type Error = BareConduitError;

    // r[impl zerocopy.framing.single-pass]
    // r[impl zerocopy.framing.no-double-serialize]
    // r[impl zerocopy.scatter]
    // r[impl zerocopy.scatter.plan]
    // r[impl zerocopy.scatter.plan.size]
    // r[impl zerocopy.scatter.write]
    // r[impl zerocopy.scatter.lifetime]
    fn send(self, item: F::Msg<'_>) -> Result<(), Self::Error> {
        // SAFETY: shape was set from F::shape() at construction time.
        // The item is a valid instance of F::Msg<'_>, which shares the same
        // layout and shape as F::Msg<'static>.
        #[allow(unsafe_code)]
        let peek = unsafe {
            Peek::unchecked_new(PtrConst::new((&raw const item).cast::<u8>()), self.shape)
        };
        let plan = vox_postcard::peek_to_scatter_plan(peek).map_err(BareConduitError::Encode)?;

        let mut slot = self
            .permit
            .alloc(plan.total_size())
            .map_err(BareConduitError::Io)?;
        plan.write_into(slot.as_mut_slice());
        slot.commit();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

pub struct BareConduitRx<F: MsgFamily, LRx> {
    link_rx: LRx,
    message_plan: Option<MessagePlan>,
    _phantom: PhantomData<fn() -> F>,
}

impl<F: MsgFamily, LRx> ConduitRx for BareConduitRx<F, LRx>
where
    LRx: vox_types::LinkRx + MaybeSend + 'static,
{
    type Msg = F;
    type Error = BareConduitError;

    // r[impl zerocopy.recv]
    #[moire::instrument]
    async fn recv(&mut self) -> Result<Option<SelfRef<F::Msg<'static>>>, Self::Error> {
        let backing = match self.link_rx.recv().await.map_err(|error| {
            BareConduitError::Io(std::io::Error::other(format!("link recv failed: {error}")))
        })? {
            Some(b) => b,
            None => return Ok(None),
        };

        match &self.message_plan {
            Some(plan) => crate::deserialize_postcard_with_plan::<F::Msg<'static>>(
                backing,
                &plan.plan,
                &plan.registry,
            ),
            None => crate::deserialize_postcard::<F::Msg<'static>>(backing),
        }
        .map_err(BareConduitError::Decode)
        .map(Some)
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum BareConduitError {
    Encode(vox_postcard::SerializeError),
    Decode(vox_postcard::DeserializeError),
    Io(std::io::Error),
    LinkDead,
}

impl std::fmt::Display for BareConduitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encode(e) => write!(f, "encode error: {e}"),
            Self::Decode(e) => write!(f, "decode error: {e}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::LinkDead => write!(f, "link dead"),
        }
    }
}

impl std::error::Error for BareConduitError {}

#[cfg(test)]
mod tests {
    use vox_types::*;

    use super::*;
    use crate::memory_link_pair;

    #[test]
    fn connection_reject_with_nonempty_metadata_round_trips() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async { connection_reject_with_nonempty_metadata_inner().await });
    }

    async fn connection_reject_with_nonempty_metadata_inner() {
        let (a, b) = memory_link_pair(64);
        let a_conduit = BareConduit::<MessageFamily, _>::new(a);
        let b_conduit = BareConduit::<MessageFamily, _>::new(b);
        let (a_tx, _a_rx) = a_conduit.split();
        let (_b_tx, mut b_rx) = b_conduit.split();

        // Send a ConnectionReject with non-empty metadata
        let permit = a_tx.reserve().await.unwrap();
        let msg = Message {
            connection_id: ConnectionId(1),
            payload: MessagePayload::ConnectionReject(ConnectionReject {
                metadata: vec![MetadataEntry::str(
                    "error",
                    "missing required vox-service metadata",
                )],
            }),
        };
        permit.send(msg).unwrap();

        // Receive and verify
        let received = b_rx.recv().await.unwrap().unwrap();
        let msg = received.get();
        if let MessagePayload::ConnectionReject(reject) = &msg.payload {
            assert_eq!(reject.metadata.len(), 1, "expected 1 metadata entry");
            assert_eq!(
                reject.metadata[0].key.as_ref(),
                "error",
                "key mismatch: got {:?}",
                reject.metadata[0].key
            );
            match &reject.metadata[0].value {
                MetadataValue::String(s) => {
                    assert_eq!(s.as_ref(), "missing required vox-service metadata");
                }
                other => panic!("expected String, got {:?}", other),
            }
        } else {
            panic!("expected ConnectionReject, got {:?}", msg.payload);
        }
    }
}
