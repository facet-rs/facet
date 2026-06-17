use std::marker::PhantomData;

use vox_types::{Conduit, ConduitRx, ConduitTx, Link, LinkTx, MaybeSend, MsgFamily, SelfRef};

use crate::MessagePlan;

/// Wraps a [`Link`] with phon serialization. No reconnect, no reliability.
///
/// If the link dies, the conduit is dead. For localhost or any transport
/// where reconnect isn't needed.
///
/// `F` is a [`MsgFamily`] — it maps lifetimes to concrete message types.
/// The send path accepts `F::Msg<'a>` (borrowed data serialized in place).
/// The recv path yields `SelfRef<F::Msg<'static>>`: the decoded value borrows
/// the received backing.
///
/// The `Message` envelope is an evolvable wire type like any other: the Rx half
/// builds a compatibility decode program from the peer's envelope schema (received
/// in the handshake, carried in [`MessagePlan`]) to its own `Message` descriptor
/// and reuses it for every frame. There is no same-version envelope shortcut.
// r[impl conduit.bare]
// r[impl conduit.typeplan]
// r[impl connection.handshake.protocol-schema.connection-scoped]
pub struct BareConduit<F: MsgFamily, L: Link> {
    link: L,
    /// The peer's `Message` envelope schema (phon bytes) from the handshake, or
    /// `None` when built without a plan (tests / degenerate path) — in which case
    /// the Rx half derives our own schema as the writer (the schema-identical degenerate
    /// of the one compat path, not a shortcut).
    writer_schema: Option<Vec<u8>>,
    _phantom: PhantomData<fn(F) -> F>,
}

impl<F: MsgFamily, L: Link> BareConduit<F, L> {
    /// Create a new BareConduit without a pre-exchanged envelope schema. The Rx
    /// half will build a program from our own `Message` schema to itself — the
    /// schema-identical degenerate of the compat path. Used by tests and the rare
    /// no-handshake case.
    pub fn new(link: L) -> Self {
        Self {
            link,
            writer_schema: None,
            _phantom: PhantomData,
        }
    }

    /// Create a BareConduit carrying the peer's envelope schema from the
    /// handshake. The Rx half builds the compat decode program against its own
    /// `Message` descriptor.
    // r[impl connection.handshake.protocol-schema.connection-scoped]
    pub fn with_message_plan(link: L, message_plan: MessagePlan) -> Self {
        Self {
            link,
            writer_schema: Some(message_plan.writer_schema),
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
                _phantom: PhantomData,
            },
            BareConduitRx {
                link_rx: rx,
                pending_fds: vox_types::FrameFds::default(),
                writer_schema: self.writer_schema,
                program: None,
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

    fn prepare_send(&self, item: F::Msg<'_>) -> Result<Self::Prepared, Self::Error> {
        // Collect any `Fd`s the encoder funnels into the thread-local
        // collector — same install-around-encode shape as the channel
        // binder. Off-Unix this is a pass-through and `fds` is `()`.
        let (encoded, fds) =
            vox_types::collect_fds(|| vox_phon::to_vec(&item).map_err(BareConduitError::Encode));
        Ok(PreparedFrame {
            bytes: encoded?,
            fds,
        })
    }

    async fn send_prepared(&self, prepared: Self::Prepared) -> Result<(), Self::Error> {
        let PreparedFrame { bytes, fds } = prepared;
        // r[impl transport.fd.capability]
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
    /// The peer's `Message` envelope schema (phon bytes) from the handshake, or
    /// `None` to build a program from our own schema to itself (degenerate path).
    writer_schema: Option<Vec<u8>>,
    /// The compat decode program, built lazily on the first `recv` (writer schema
    /// against `F::Msg`) and reused.
    program: Option<vox_phon::DecodeProgram>,
    _phantom: PhantomData<fn() -> F>,
}

impl<F: MsgFamily, LRx> BareConduitRx<F, LRx> {
    /// Build (once) and return the envelope compat decode program. Uses the
    /// peer's `Message` schema — or our own, when none was exchanged (the
    /// schema-identical degenerate of the one compat path) — against `F::Msg`'s
    /// descriptor via phon's `lower_decode`.
    // r[impl conduit.typeplan]
    // r[impl connection.handshake.protocol-schema.connection-scoped]
    fn ensure_program(&mut self) -> Result<&vox_phon::DecodeProgram, BareConduitError> {
        if self.program.is_none() {
            let writer_bytes = match &self.writer_schema {
                Some(b) => std::borrow::Cow::Borrowed(b.as_slice()),
                None => std::borrow::Cow::Owned(
                    vox_phon::schema_bytes::<F::Msg<'static>>()
                        .map_err(BareConduitError::Decode)?,
                ),
            };
            let writer =
                vox_phon::parse_schema_bytes(&writer_bytes).map_err(BareConduitError::Decode)?;
            let program = vox_phon::build_decode_program::<F::Msg<'static>>(&writer)
                .map_err(BareConduitError::Decode)?;
            self.program = Some(program);
        }
        Ok(self.program.as_ref().expect("program built above"))
    }
}

impl<F: MsgFamily, LRx> ConduitRx for BareConduitRx<F, LRx>
where
    LRx: vox_types::LinkRx + MaybeSend + 'static,
{
    type Msg = F;
    type Error = BareConduitError;

    #[vox_rt::instrument]
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
        // `take_frame_fds` and installed at that decode site, not here.
        self.pending_fds = self.link_rx.take_frame_fds();

        // Lazily build the envelope compat program from the peer's
        // `Message` schema (or our own, in the degenerate no-exchange case)
        // against our `Message` descriptor. Built once, reused for every frame.
        let program = self.ensure_program()?;

        // Decode the envelope through the compat program: the decoded
        // `Message` borrows the backing (payload span, metadata strings).
        SelfRef::try_new(backing, |bytes| {
            vox_phon::decode_with_program::<F::Msg<'static>>(program, bytes)
                .map_err(BareConduitError::Decode)
        })
        .map(Some)
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
    Encode(vox_phon::Error),
    Decode(vox_phon::Error),
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

        // Send a LaneReject with non-empty metadata
        let msg = Message {
            lane_id: LaneId(1),
            payload: MessagePayload::LaneReject(LaneReject {
                metadata: metadata()
                    .str("error", "missing required vox-service metadata")
                    .build(),
            }),
        };
        let prepared = a_tx.prepare_send(msg).unwrap();
        a_tx.send_prepared(prepared).await.unwrap();

        // Receive and verify
        let received = b_rx.recv().await.unwrap().unwrap();
        let msg = received.get();
        if let MessagePayload::LaneReject(reject) = &msg.payload {
            assert_eq!(reject.metadata.meta_len(), 1, "expected 1 metadata entry");
            assert_eq!(
                reject.metadata.meta_str("error"),
                Some("missing required vox-service metadata"),
            );
        } else {
            panic!("expected LaneReject, got {:?}", msg.payload);
        }
    }
}
