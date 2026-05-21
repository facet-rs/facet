use std::marker::PhantomData;

#[cfg(not(target_arch = "wasm32"))]
use facet_core::PtrConst;
#[cfg(not(target_arch = "wasm32"))]
use vox_jit::cache::{CompiledDecoder, CompiledEncoder};
#[cfg(not(target_arch = "wasm32"))]
use vox_jit::cal::BorrowMode;

use vox_types::{Conduit, ConduitRx, ConduitTx, Link, LinkTx, MaybeSend, MsgFamily, SelfRef};

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
    #[cfg(not(target_arch = "wasm32"))]
    encoder: &'static CompiledEncoder,
    #[cfg(not(target_arch = "wasm32"))]
    decoder: Option<&'static CompiledDecoder>,
    message_plan: MessagePlan,
    _phantom: PhantomData<fn(F) -> F>,
}

impl<F: MsgFamily, L: Link> BareConduit<F, L> {
    /// Create a new BareConduit (identity plan — no schema translation).
    pub fn new(link: L) -> Self {
        let identity_plan = vox_postcard::build_identity_plan(F::shape());
        Self::resolve(
            link,
            MessagePlan {
                remote_schema_id: 0,
                plan: identity_plan,
                registry: vox_types::SchemaRegistry::new(),
            },
        )
    }

    /// Create a new BareConduit with a pre-built message translation plan.
    pub fn with_message_plan(link: L, message_plan: MessagePlan) -> Self {
        Self::resolve(link, message_plan)
    }

    fn resolve(link: L, message_plan: MessagePlan) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let runtime = vox_jit::global_runtime();
        #[cfg(not(target_arch = "wasm32"))]
        let encoder = runtime
            .prepare_encoder(F::shape())
            .expect("JIT encode unavailable for message shape");
        #[cfg(not(target_arch = "wasm32"))]
        let decoder = runtime.prepare_decoder(
            message_plan.remote_schema_id,
            F::shape(),
            &message_plan.plan,
            &message_plan.registry,
            BorrowMode::Owned,
        );
        #[cfg(not(target_arch = "wasm32"))]
        if decoder.is_none() {
            tracing::warn!("vox bare conduit message decoder unavailable; falling back");
        }
        Self {
            link,
            #[cfg(not(target_arch = "wasm32"))]
            encoder,
            #[cfg(not(target_arch = "wasm32"))]
            decoder,
            message_plan,
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
                #[cfg(not(target_arch = "wasm32"))]
                encoder: self.encoder,
                _phantom: PhantomData,
            },
            BareConduitRx {
                link_rx: rx,
                pending_fds: vox_types::FrameFds::default(),
                #[cfg(not(target_arch = "wasm32"))]
                decoder: self.decoder,
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
    #[cfg(not(target_arch = "wasm32"))]
    encoder: &'static CompiledEncoder,
    _phantom: PhantomData<fn(F)>,
}

/// A serialized message plus the file descriptors collected while encoding
/// it. The descriptors travel out-of-band via `SCM_RIGHTS`; off-Unix
/// [`FrameFds`](vox_types::FrameFds) is `()`.
pub struct PreparedFrame {
    pub bytes: Vec<u8>,
    pub fds: vox_types::FrameFds,
}

impl<F: MsgFamily, LTx: LinkTx + MaybeSend + 'static> ConduitTx for BareConduitTx<F, LTx> {
    type Msg = F;
    type Prepared = PreparedFrame;
    type Error = BareConduitError;

    // r[impl zerocopy.framing.single-pass]
    // r[impl zerocopy.framing.no-double-serialize]
    // r[impl zerocopy.scatter]
    // r[impl zerocopy.scatter.plan]
    // r[impl zerocopy.scatter.plan.size]
    // r[impl zerocopy.scatter.write]
    // r[impl zerocopy.scatter.lifetime]
    fn prepare_send(&self, item: F::Msg<'_>) -> Result<Self::Prepared, Self::Error> {
        let encode = || -> Result<Vec<u8>, BareConduitError> {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let ptr = PtrConst::new((&raw const item).cast::<u8>());
                vox_jit::encode_with(self.encoder, ptr).map_err(BareConduitError::Encode)
            }
            #[cfg(target_arch = "wasm32")]
            {
                vox_postcard::to_vec(&item).map_err(BareConduitError::Encode)
            }
        };
        // Collect any `Fd`s the encoder funnels into the thread-local
        // collector — same install-around-encode shape as the channel
        // binder (`with_channel_binder`). Off-Unix this is a pass-through
        // and `fds` is `()`.
        let (encoded, fds) = vox_types::collect_fds(encode);
        Ok(PreparedFrame {
            bytes: encoded?,
            fds,
        })
    }

    async fn send_prepared(&self, prepared: Self::Prepared) -> Result<(), Self::Error> {
        let PreparedFrame { bytes, fds } = prepared;
        if vox_types::frame_fds_len(&fds) > 0 && !self.link_tx.supports_fd_passing() {
            return Err(BareConduitError::Io(std::io::Error::other(
                "message carries file descriptors but the transport \
                 cannot pass them",
            )));
        }
        self.link_tx
            .send_with_fds(bytes, fds)
            .await
            .map_err(BareConduitError::Io)
    }

    async fn close(self) -> std::io::Result<()> {
        self.link_tx.close().await
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

pub struct BareConduitRx<F: MsgFamily, LRx> {
    link_rx: LRx,
    /// Descriptors that arrived with the most recently `recv`'d frame,
    /// awaiting [`take_frame_fds`](vox_types::ConduitRx::take_frame_fds).
    pending_fds: vox_types::FrameFds,
    #[cfg(not(target_arch = "wasm32"))]
    decoder: Option<&'static CompiledDecoder>,
    message_plan: MessagePlan,
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

        // Capture this frame's descriptors. `Payload` only *borrows* its
        // bytes during Message decode — the typed `Fd` is decoded later by
        // the generated stub — so the fds are threaded out via
        // `take_frame_fds` (the same rail as the schema tracker) and
        // installed at that decode site, not here.
        self.pending_fds = self.link_rx.take_frame_fds();

        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::deserialize_postcard_with_decoder::<F::Msg<'static>>(
                backing,
                self.decoder,
                &self.message_plan.plan,
                &self.message_plan.registry,
            )
            .map_err(BareConduitError::Decode)
            .map(Some)
        }
        #[cfg(target_arch = "wasm32")]
        {
            crate::deserialize_postcard_with_plan::<F::Msg<'static>>(
                backing,
                &self.message_plan.plan,
                &self.message_plan.registry,
            )
            .map_err(BareConduitError::Decode)
            .map(Some)
        }
    }

    fn take_frame_fds(&mut self) -> vox_types::FrameFds {
        std::mem::take(&mut self.pending_fds)
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
        let msg = Message {
            connection_id: ConnectionId(1),
            payload: MessagePayload::ConnectionReject(ConnectionReject {
                metadata: vec![MetadataEntry::str(
                    "error",
                    "missing required vox-service metadata",
                )],
            }),
        };
        let prepared = a_tx.prepare_send(msg).unwrap();
        a_tx.send_prepared(prepared).await.unwrap();

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
