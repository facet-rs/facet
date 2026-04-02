use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use facet::Facet;
use facet_core::PtrConst;
use facet_reflect::Peek;
use vox_types::{
    Conduit, ConduitRx, ConduitTx, ConduitTxPermit, Link, LinkRx, LinkTx, LinkTxPermit, MsgFamily,
    Payload, SelfRef, WriteSlot,
};

use crate::MessagePlan;
use zerocopy::little_endian::U32 as LeU32;

mod replay_buffer;
use replay_buffer::{PacketAck, PacketSeq, ReplayBuffer};

// ---------------------------------------------------------------------------
// Handshake wire types
// ---------------------------------------------------------------------------

/// 16-byte CSPRNG-generated session resumption key.
#[derive(
    Clone,
    Copy,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(C)]
struct ResumeKey([u8; 16]);

const CLIENT_HELLO_MAGIC: u32 = u32::from_le_bytes(*b"VOCH");
const SERVER_HELLO_MAGIC: u32 = u32::from_le_bytes(*b"VOSH");

// ClientHello flags
const CH_HAS_RESUME_KEY: u8 = 0b0000_0001;
const CH_HAS_LAST_RECEIVED: u8 = 0b0000_0010;

// ServerHello flags
const SH_REJECTED: u8 = 0b0000_0001;
const SH_HAS_LAST_RECEIVED: u8 = 0b0000_0010;

/// Client's opening handshake — fixed-size, cast directly from wire bytes.
// r[impl stable.handshake.client-hello]
#[derive(
    Clone,
    Copy,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(C)]
pub struct ClientHello {
    magic: LeU32,
    flags: u8,
    resume_key: ResumeKey,
    last_received: LeU32,
}

/// Server's handshake response — fixed-size, cast directly from wire bytes.
// r[impl stable.handshake.server-hello]
// r[impl stable.reconnect.failure]
#[derive(
    Clone,
    Copy,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(C)]
struct ServerHello {
    magic: LeU32,
    flags: u8,
    resume_key: ResumeKey,
    last_received: LeU32,
}

/// Sequenced data frame. All post-handshake traffic is a `Frame`.
///
/// The `item` field is an opaque [`Payload`] — the message bytes are
/// serialized/deserialized independently from the frame envelope.
/// This decouples the frame format from the message schema.
// r[impl stable.framing]
// r[impl stable.framing.encoding]
#[derive(Facet, Debug)]
struct Frame<'a> {
    seq: PacketSeq,
    // r[impl stable.ack]
    ack: Option<PacketAck>,
    item: Payload<'a>,
}

vox_types::impl_reborrow!(Frame);

// ---------------------------------------------------------------------------
// Attachment / LinkSource
// ---------------------------------------------------------------------------

/// One transport attachment consumed by [`LinkSource::next_link`].
///
/// Use [`Attachment::initiator`] for the initiator side, or
/// [`prepare_acceptor_attachment`] on inbound links for the acceptor side.
pub struct Attachment<L> {
    link: L,
    client_hello: Option<ClientHello>,
}

impl<L> Attachment<L> {
    /// Build an initiator-side attachment.
    pub fn initiator(link: L) -> Self {
        Self {
            link,
            client_hello: None,
        }
    }

    pub(crate) fn into_link(self) -> L {
        self.link
    }
}

/// Link wrapper that re-combines pre-split Tx/Rx halves into a [`Link`].
///
/// This is used by [`prepare_acceptor_attachment`] after consuming the inbound
/// `ClientHello` during stable-conduit setup.
pub struct SplitLink<Tx, Rx> {
    tx: Tx,
    rx: Rx,
}

impl<Tx, Rx> Link for SplitLink<Tx, Rx>
where
    Tx: LinkTx,
    Rx: LinkRx,
{
    type Tx = Tx;
    type Rx = Rx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        (self.tx, self.rx)
    }
}

/// Prepare an acceptor-side attachment from an inbound link.
///
/// This consumes the leading stable `ClientHello` from the inbound link and
/// returns an attachment suitable for acceptor-side [`StableConduit::new`].
pub async fn prepare_acceptor_attachment<L: Link>(
    link: L,
) -> Result<Attachment<SplitLink<L::Tx, L::Rx>>, StableConduitError> {
    let (tx, mut rx) = link.split();
    let client_hello = recv_handshake::<_, ClientHello>(&mut rx).await?;
    Ok(Attachment {
        link: SplitLink { tx, rx },
        client_hello: Some(client_hello),
    })
}

// r[impl stable.link-source]
pub trait LinkSource: Send + 'static {
    type Link: Link + Send;

    fn next_link(
        &mut self,
    ) -> impl Future<Output = std::io::Result<Attachment<Self::Link>>> + Send + '_;
}

/// A one-shot [`LinkSource`] backed by a single attachment.
pub struct SingleAttachmentSource<L> {
    attachment: Option<Attachment<L>>,
}

/// Build a one-shot [`LinkSource`] from a prepared attachment.
pub fn single_attachment_source<L: Link + Send + 'static>(
    attachment: Attachment<L>,
) -> SingleAttachmentSource<L> {
    SingleAttachmentSource {
        attachment: Some(attachment),
    }
}

/// Build a one-shot initiator-side [`LinkSource`] from a raw link.
pub fn single_link_source<L: Link + Send + 'static>(link: L) -> SingleAttachmentSource<L> {
    single_attachment_source(Attachment::initiator(link))
}

/// Build an already-exhausted [`LinkSource`]. Any call to `next_link` will
/// fail immediately. Used when the first link is passed directly to
/// [`StableConduit::with_first_link`] and no reconnection source is available.
pub fn exhausted_source<L: Link + Send + 'static>() -> SingleAttachmentSource<L> {
    SingleAttachmentSource { attachment: None }
}

impl<L: Link + Send + 'static> LinkSource for SingleAttachmentSource<L> {
    type Link = L;

    async fn next_link(&mut self) -> std::io::Result<Attachment<Self::Link>> {
        self.attachment.take().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "single-use LinkSource exhausted",
            )
        })
    }
}

// ---------------------------------------------------------------------------
// StableConduit
// ---------------------------------------------------------------------------

// r[impl stable]
// r[impl zerocopy.framing.conduit.stable]
pub struct StableConduit<F: MsgFamily, LS: LinkSource> {
    shared: Arc<Shared<LS>>,
    message_plan: Option<MessagePlan>,
    _phantom: PhantomData<fn(F) -> F>,
}

struct Shared<LS: LinkSource> {
    inner: Mutex<Inner<LS>>,
    reconnecting: AtomicBool,
    reconnected: moire::sync::Notify,
    tx_ready: moire::sync::Notify,
}

struct Inner<LS: LinkSource> {
    source: Option<LS>,
    /// Incremented every time the link is replaced. Used to detect whether
    /// another task has already reconnected while we were waiting.
    link_generation: u64,
    tx: Option<<LS::Link as Link>::Tx>,
    rx: Option<<LS::Link as Link>::Rx>,
    tx_checked_out: bool,
    resume_key: Option<ResumeKey>,
    // r[impl stable.seq]
    next_send_seq: PacketSeq,
    last_received: Option<PacketSeq>,
    // r[impl stable.replay-buffer]
    /// Encoded item bytes buffered for replay on reconnect.
    replay: ReplayBuffer,
}

impl<F: MsgFamily, LS: LinkSource> StableConduit<F, LS> {
    pub async fn new(mut source: LS) -> Result<Self, StableConduitError> {
        let attachment = source.next_link().await.map_err(StableConduitError::Io)?;
        let (link_tx, link_rx) = attachment.link.split();
        Self::with_first_link(link_tx, link_rx, attachment.client_hello, source).await
    }

    /// Create a stable conduit with a pre-split first link.
    ///
    /// Use this when the first link has already been obtained and processed
    /// (e.g. after a CBOR session handshake) before the stable conduit's own
    /// resume handshake runs.
    pub async fn with_first_link(
        link_tx: <LS::Link as Link>::Tx,
        mut link_rx: <LS::Link as Link>::Rx,
        client_hello: Option<ClientHello>,
        source: LS,
    ) -> Result<Self, StableConduitError> {
        let (resume_key, _peer_last_received) =
            handshake::<LS::Link>(&link_tx, &mut link_rx, client_hello, None, None).await?;

        let inner = Inner {
            source: Some(source),
            link_generation: 0,
            tx: Some(link_tx),
            rx: Some(link_rx),
            tx_checked_out: false,
            resume_key: Some(resume_key),
            next_send_seq: PacketSeq(0),
            last_received: None,
            replay: ReplayBuffer::new(),
        };

        Ok(Self {
            shared: Arc::new(Shared {
                inner: Mutex::new(inner),
                reconnecting: AtomicBool::new(false),
                reconnected: moire::sync::Notify::new("stable_conduit.reconnected"),
                tx_ready: moire::sync::Notify::new("stable_conduit.tx_ready"),
            }),
            message_plan: None,
            _phantom: PhantomData,
        })
    }

    /// Set the message plan for schema-aware deserialization of the payload
    /// inside each frame.
    pub fn with_message_plan(mut self, plan: MessagePlan) -> Self {
        self.message_plan = Some(plan);
        self
    }
}

// ---------------------------------------------------------------------------
// Reconnect
// ---------------------------------------------------------------------------

impl<LS: LinkSource> Shared<LS> {
    fn lock_inner(&self) -> Result<MutexGuard<'_, Inner<LS>>, StableConduitError> {
        self.inner
            .lock()
            .map_err(|_| StableConduitError::Setup("stable conduit mutex poisoned".into()))
    }

    async fn ensure_reconnected(&self, generation: u64) -> Result<(), StableConduitError> {
        loop {
            {
                let inner = self.lock_inner()?;
                if inner.link_generation != generation {
                    return Ok(());
                }
            }

            if self
                .reconnecting
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let result = self.reconnect_once(generation).await;
                self.reconnecting.store(false, Ordering::Release);
                self.reconnected.notify_waiters();
                return result;
            }

            self.reconnected.notified().await;
        }
    }

    /// Obtain a new link from the source, re-handshake, and replay any
    /// buffered items the peer missed.
    // r[impl stable.reconnect]
    // r[impl stable.reconnect.client-replay]
    // r[impl stable.reconnect.server-replay]
    // r[impl stable.replay-buffer.order]
    async fn reconnect_once(&self, generation: u64) -> Result<(), StableConduitError> {
        let (mut source, resume_key, last_received, replay_frames) = {
            let mut inner = self.lock_inner()?;
            if inner.link_generation != generation {
                return Ok(());
            }
            let source = inner
                .source
                .take()
                .ok_or_else(|| StableConduitError::Setup("link source unavailable".into()))?;
            let replay_frames = inner
                .replay
                .iter()
                .map(|(seq, bytes)| (*seq, bytes.clone()))
                .collect::<Vec<_>>();
            (source, inner.resume_key, inner.last_received, replay_frames)
        };

        let reconnect_result = async {
            let attachment = source.next_link().await.map_err(StableConduitError::Io)?;
            let (new_tx, mut new_rx) = attachment.link.split();

            let (new_resume_key, peer_last_received) = handshake::<LS::Link>(
                &new_tx,
                &mut new_rx,
                attachment.client_hello,
                resume_key,
                last_received,
            )
            .await?;

            // Replay frames the peer hasn't received yet, in original order.
            // Frame bytes include the original seq/ack — stale acks are
            // harmless since the peer ignores acks older than what it has seen.
            for (seq, frame_bytes) in replay_frames {
                if peer_last_received.is_some_and(|last| seq <= last) {
                    continue;
                }
                let permit = new_tx.reserve().await.map_err(StableConduitError::Io)?;
                let mut slot = permit
                    .alloc(frame_bytes.len())
                    .map_err(StableConduitError::Io)?;
                slot.as_mut_slice().copy_from_slice(&frame_bytes);
                slot.commit();
            }

            Ok::<_, StableConduitError>((new_tx, new_rx, new_resume_key))
        }
        .await;

        let mut inner = self.lock_inner()?;
        inner.source = Some(source);

        if inner.link_generation != generation {
            return Ok(());
        }

        let (new_tx, new_rx, new_resume_key) = reconnect_result?;

        inner.link_generation = inner.link_generation.wrapping_add(1);
        inner.tx = Some(new_tx);
        inner.rx = Some(new_rx);
        inner.tx_checked_out = false;
        inner.resume_key = Some(new_resume_key);
        self.tx_ready.notify_waiters();

        Ok(())
    }
}

/// Perform the handshake on a fresh link.
///
/// Returns `(our_resume_key, peer_last_received)`:
///   - `our_resume_key`: the key to use for the next reconnect attempt
///   - `peer_last_received`: the highest seq the peer has already seen,
///     used to decide which replay-buffer entries to re-send
// r[impl stable.handshake]
async fn handshake<L: Link>(
    tx: &L::Tx,
    rx: &mut L::Rx,
    client_hello: Option<ClientHello>,
    resume_key: Option<ResumeKey>,
    last_received: Option<PacketSeq>,
) -> Result<(ResumeKey, Option<PacketSeq>), StableConduitError> {
    match client_hello {
        None => {
            // r[impl stable.reconnect]
            let mut flags = 0u8;
            if resume_key.is_some() {
                flags |= CH_HAS_RESUME_KEY;
            }
            if last_received.is_some() {
                flags |= CH_HAS_LAST_RECEIVED;
            }
            let hello = ClientHello {
                magic: LeU32::new(CLIENT_HELLO_MAGIC),
                flags,
                resume_key: resume_key.unwrap_or(ResumeKey([0u8; 16])),
                last_received: LeU32::new(last_received.map_or(0, |s| s.0)),
            };
            send_handshake(tx, &hello).await?;

            let sh = recv_handshake::<_, ServerHello>(rx).await?;
            if sh.magic.get() != SERVER_HELLO_MAGIC {
                return Err(StableConduitError::Setup(
                    "ServerHello magic mismatch".into(),
                ));
            }
            // r[impl stable.reconnect.failure]
            if sh.flags & SH_REJECTED != 0 {
                return Err(StableConduitError::SessionLost);
            }
            let peer_last_received =
                (sh.flags & SH_HAS_LAST_RECEIVED != 0).then(|| PacketSeq(sh.last_received.get()));
            Ok((sh.resume_key, peer_last_received))
        }
        Some(ch) => {
            // r[impl stable.resume-key]
            let key = fresh_key()?;
            let mut flags = 0u8;
            if last_received.is_some() {
                flags |= SH_HAS_LAST_RECEIVED;
            }
            let hello = ServerHello {
                magic: LeU32::new(SERVER_HELLO_MAGIC),
                flags,
                resume_key: key,
                last_received: LeU32::new(last_received.map_or(0, |s| s.0)),
            };
            send_handshake(tx, &hello).await?;

            let peer_last_received =
                (ch.flags & CH_HAS_LAST_RECEIVED != 0).then(|| PacketSeq(ch.last_received.get()));
            Ok((key, peer_last_received))
        }
    }
}

async fn send_handshake<LTx: LinkTx, M: zerocopy::IntoBytes + zerocopy::Immutable>(
    tx: &LTx,
    msg: &M,
) -> Result<(), StableConduitError> {
    let bytes = msg.as_bytes();
    let permit = tx.reserve().await.map_err(StableConduitError::Io)?;
    let mut slot = permit.alloc(bytes.len()).map_err(StableConduitError::Io)?;
    slot.as_mut_slice().copy_from_slice(bytes);
    slot.commit();
    Ok(())
}

async fn recv_handshake<
    LRx: LinkRx,
    M: zerocopy::FromBytes + zerocopy::KnownLayout + zerocopy::Immutable,
>(
    rx: &mut LRx,
) -> Result<M, StableConduitError> {
    let backing = rx
        .recv()
        .await
        .map_err(|_| StableConduitError::LinkDead)?
        .ok_or(StableConduitError::LinkDead)?;
    M::read_from_bytes(backing.as_bytes())
        .map_err(|_| StableConduitError::Setup("handshake message size mismatch".into()))
}

/// Receive a stable conduit `ClientHello` from a link.
///
/// Used by the acceptor when the CBOR session handshake has already been
/// completed on the link — the next bytes are the stable conduit's
/// binary `ClientHello`.
pub async fn recv_client_hello<Rx: LinkRx>(rx: &mut Rx) -> Result<ClientHello, StableConduitError> {
    recv_handshake::<_, ClientHello>(rx).await
}

fn fresh_key() -> Result<ResumeKey, StableConduitError> {
    let mut key = ResumeKey([0u8; 16]);
    getrandom::fill(&mut key.0)
        .map_err(|e| StableConduitError::Setup(format!("failed to generate resume key: {e}")))?;
    Ok(key)
}

// ---------------------------------------------------------------------------
// Conduit impl
// ---------------------------------------------------------------------------

impl<F: MsgFamily, LS: LinkSource> Conduit for StableConduit<F, LS>
where
    <LS::Link as Link>::Tx: Send + 'static,
    <LS::Link as Link>::Rx: Send + 'static,
    LS: Send + 'static,
{
    type Msg = F;
    type Tx = StableConduitTx<F, LS>;
    type Rx = StableConduitRx<F, LS>;

    fn split(self) -> (Self::Tx, Self::Rx) {
        (
            StableConduitTx {
                shared: Arc::clone(&self.shared),
                _phantom: PhantomData,
            },
            StableConduitRx {
                shared: Arc::clone(&self.shared),
                message_plan: self.message_plan,
                _phantom: PhantomData,
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

pub struct StableConduitTx<F: MsgFamily, LS: LinkSource> {
    shared: Arc<Shared<LS>>,
    _phantom: PhantomData<fn(F)>,
}

impl<F: MsgFamily, LS: LinkSource> ConduitTx for StableConduitTx<F, LS>
where
    <LS::Link as Link>::Tx: Send + 'static,
    <LS::Link as Link>::Rx: Send + 'static,
    LS: Send + 'static,
{
    type Msg = F;
    type Permit<'a>
        = StableConduitPermit<F, LS>
    where
        Self: 'a;

    async fn reserve(&self) -> std::io::Result<Self::Permit<'_>> {
        enum TxReservation<Tx> {
            CheckedOut { tx: Tx, generation: u64 },
            Wait,
            Reconnect { generation: u64 },
        }

        loop {
            let reservation = {
                let mut inner = self
                    .shared
                    .lock_inner()
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                match inner.tx.take() {
                    Some(tx) => {
                        inner.tx_checked_out = true;
                        TxReservation::CheckedOut {
                            tx,
                            generation: inner.link_generation,
                        }
                    }
                    None if inner.tx_checked_out => TxReservation::Wait,
                    None => TxReservation::Reconnect {
                        generation: inner.link_generation,
                    },
                }
            };

            let (tx, generation) = match reservation {
                TxReservation::CheckedOut { tx, generation } => (tx, generation),
                TxReservation::Wait => {
                    self.shared.tx_ready.notified().await;
                    continue;
                }
                TxReservation::Reconnect { generation } => {
                    self.shared
                        .ensure_reconnected(generation)
                        .await
                        .map_err(|e| std::io::Error::other(e.to_string()))?;
                    continue;
                }
            };

            match tx.reserve().await {
                Ok(link_permit) => {
                    let restore_ok = {
                        let mut inner = self
                            .shared
                            .lock_inner()
                            .map_err(|e| std::io::Error::other(e.to_string()))?;
                        let restore_ok = inner.link_generation == generation && inner.tx.is_none();
                        if restore_ok {
                            inner.tx = Some(tx);
                        }
                        inner.tx_checked_out = false;
                        self.shared.tx_ready.notify_waiters();
                        restore_ok
                    };

                    if !restore_ok {
                        drop(link_permit);
                        continue;
                    }

                    return Ok(StableConduitPermit {
                        shared: Arc::clone(&self.shared),
                        link_permit,
                        generation,
                        _phantom: PhantomData,
                    });
                }
                Err(_) => {
                    {
                        let mut inner = self
                            .shared
                            .lock_inner()
                            .map_err(|e| std::io::Error::other(e.to_string()))?;
                        if inner.link_generation == generation {
                            inner.tx_checked_out = false;
                        }
                        self.shared.tx_ready.notify_waiters();
                    }
                    self.shared
                        .ensure_reconnected(generation)
                        .await
                        .map_err(|e| std::io::Error::other(e.to_string()))?;
                }
            }
        }
    }

    async fn close(self) -> std::io::Result<()> {
        let tx = {
            let mut inner = self
                .shared
                .lock_inner()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            inner.tx.take()
        };
        if let Some(tx) = tx {
            tx.close().await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Permit
// ---------------------------------------------------------------------------

pub struct StableConduitPermit<F: MsgFamily, LS: LinkSource> {
    shared: Arc<Shared<LS>>,
    link_permit: <<LS::Link as Link>::Tx as LinkTx>::Permit,
    generation: u64,
    _phantom: PhantomData<fn(F)>,
}

impl<F: MsgFamily, LS: LinkSource> ConduitTxPermit for StableConduitPermit<F, LS> {
    type Msg = F;
    type Error = StableConduitError;

    // r[impl zerocopy.framing.single-pass]
    // r[impl zerocopy.framing.no-double-serialize]
    // r[impl zerocopy.scatter]
    // r[impl zerocopy.scatter.plan]
    // r[impl zerocopy.scatter.plan.size]
    // r[impl zerocopy.scatter.write]
    // r[impl zerocopy.scatter.lifetime]
    // r[impl zerocopy.scatter.replay]
    fn send(self, item: F::Msg<'_>) -> Result<(), StableConduitError> {
        let StableConduitPermit {
            shared,
            link_permit,
            generation,
            _phantom: _,
        } = self;

        let (seq, ack) = {
            let mut inner = shared.lock_inner()?;
            if inner.link_generation != generation {
                return Err(StableConduitError::LinkDead);
            }
            let seq = inner.next_send_seq;
            inner.next_send_seq = PacketSeq(seq.0.wrapping_add(1));
            let ack = inner
                .last_received
                .map(|max_delivered| PacketAck { max_delivered });
            (seq, ack)
        };

        // Wrap the message as an outgoing Payload — the opaque adapter
        // serializes its bytes inline, giving us one scatter pass for the
        // whole frame (header + message bytes).
        let msg_shape = F::shape();
        // SAFETY: item is a valid F::Msg<'_> and msg_shape matches it.
        #[allow(unsafe_code)]
        let payload = unsafe {
            Payload::outgoing_unchecked(PtrConst::new((&raw const item).cast::<u8>()), msg_shape)
        };

        let frame = Frame {
            seq,
            ack,
            item: payload,
        };

        // SAFETY: Frame<'_> shape is lifetime-independent.
        #[allow(unsafe_code)]
        let peek = unsafe {
            Peek::unchecked_new(
                PtrConst::new((&raw const frame).cast::<u8>()),
                <Frame<'static> as Facet<'static>>::SHAPE,
            )
        };
        let plan = vox_postcard::peek_to_scatter_plan(peek).map_err(StableConduitError::Encode)?;

        let mut slot = link_permit
            .alloc(plan.total_size())
            .map_err(StableConduitError::Io)?;
        let slot_bytes = slot.as_mut_slice();
        plan.write_into(slot_bytes);

        // Keep an owned copy for replay after reconnect.
        shared.lock_inner()?.replay.push(seq, slot_bytes.to_vec());
        slot.commit();

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

pub struct StableConduitRx<F: MsgFamily, LS: LinkSource> {
    shared: Arc<Shared<LS>>,
    message_plan: Option<MessagePlan>,
    _phantom: PhantomData<fn() -> F>,
}

impl<F: MsgFamily, LS: LinkSource> ConduitRx for StableConduitRx<F, LS>
where
    <LS::Link as Link>::Tx: Send + 'static,
    <LS::Link as Link>::Rx: Send + 'static,
    LS: Send + 'static,
{
    type Msg = F;
    type Error = StableConduitError;

    #[moire::instrument]
    async fn recv(&mut self) -> Result<Option<SelfRef<F::Msg<'static>>>, Self::Error> {
        loop {
            // Phase 1: take current Rx out of shared state, then await without locks held.
            let (rx_opt, generation) = {
                let mut inner = self.shared.lock_inner()?;
                (inner.rx.take(), inner.link_generation)
            }; // lock released here — no guard held across any await below
            let mut rx = match rx_opt {
                Some(rx) => rx,
                None => {
                    self.shared.ensure_reconnected(generation).await?;
                    continue;
                }
            };

            // Any link termination — graceful EOF or error — triggers reconnect.
            // The session ends only when the LinkSource itself fails (no more
            // links available), which surfaces as Err.
            let recv_result = rx.recv().await;

            // Put Rx back only if we're still on the same generation and no newer
            // Rx has been installed by reconnect.
            {
                let mut inner = self.shared.lock_inner()?;
                if inner.link_generation == generation && inner.rx.is_none() {
                    inner.rx = Some(rx);
                }
            }

            let backing = match recv_result {
                Ok(Some(b)) => b,
                Ok(None) | Err(_) => {
                    // r[impl stable.reconnect]
                    self.shared.ensure_reconnected(generation).await?;
                    continue;
                }
            };

            // Phase 2: deserialize the frame envelope (seq/ack + opaque payload bytes).
            let frame: SelfRef<Frame<'static>> =
                crate::deserialize_postcard(backing).map_err(StableConduitError::Decode)?;

            // Phase 3: update shared state; skip duplicates.
            // r[impl stable.seq.monotonic]
            // r[impl stable.ack.trim]
            let is_dup = {
                let frame = frame.get();
                let mut inner = self.shared.lock_inner()?;

                if let Some(ack) = frame.ack {
                    inner.replay.trim(ack);
                }

                let dup = inner.last_received.is_some_and(|prev| frame.seq <= prev);
                if !dup {
                    inner.last_received = Some(frame.seq);
                }
                dup
            };

            if is_dup {
                continue;
            }

            // Phase 4: deserialize the message from the payload bytes
            // using the message plan for schema-aware translation.
            let frame = frame.get();
            let item_bytes = match &frame.item {
                Payload::PostcardBytes(bytes) => bytes,
                _ => unreachable!("deserialized Payload should always be Incoming"),
            };
            let item_backing = vox_types::Backing::Boxed(item_bytes.to_vec().into());
            let msg = match &self.message_plan {
                Some(plan) => crate::deserialize_postcard_with_plan::<F::Msg<'static>>(
                    item_backing,
                    &plan.plan,
                    &plan.registry,
                ),
                None => crate::deserialize_postcard::<F::Msg<'static>>(item_backing),
            }
            .map_err(StableConduitError::Decode)?;

            return Ok(Some(msg));
        }
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum StableConduitError {
    Encode(vox_postcard::SerializeError),
    Decode(vox_postcard::DeserializeError),
    Io(std::io::Error),
    LinkDead,
    Setup(String),
    /// The server rejected our resume_key; the session is permanently lost.
    // r[impl stable.reconnect.failure]
    SessionLost,
}

impl std::fmt::Display for StableConduitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encode(e) => write!(f, "encode error: {e}"),
            Self::Decode(e) => write!(f, "decode error: {e}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::LinkDead => write!(f, "link dead"),
            Self::Setup(s) => write!(f, "setup error: {s}"),
            Self::SessionLost => write!(f, "session lost: server rejected resume key"),
        }
    }
}

impl std::error::Error for StableConduitError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
