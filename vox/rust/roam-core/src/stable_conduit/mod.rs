use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use facet::Facet;
use facet_core::PtrConst;
use facet_reflect::Peek;
use roam_types::{
    Conduit, ConduitRx, ConduitTx, ConduitTxPermit, Link, LinkRx, LinkTx, LinkTxPermit, MsgFamily,
    SelfRef, WriteSlot,
};
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

const CLIENT_HELLO_MAGIC: u32 = u32::from_le_bytes(*b"ROCH");
const SERVER_HELLO_MAGIC: u32 = u32::from_le_bytes(*b"ROSH");

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
struct ClientHello {
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

/// Sequenced data frame. All post-handshake traffic is `Frame<T>`.
/// Serialized in a single postcard pass — the seq/ack fields are just
/// the first fields of the serialized output.
// r[impl stable.framing]
// r[impl stable.framing.encoding]
#[derive(Facet, Debug, Clone)]
struct Frame<T> {
    seq: PacketSeq,
    // r[impl stable.ack]
    ack: Option<PacketAck>,
    item: T,
}

// ---------------------------------------------------------------------------
// Attachment / LinkSource
// ---------------------------------------------------------------------------

struct Attachment<L> {
    link: L,
    client_hello: Option<ClientHello>,
}

// r[impl stable.link-source]
pub trait LinkSource: Send + 'static {
    type Link: Link + Send;

    fn next_link(
        &mut self,
    ) -> impl Future<Output = std::io::Result<Attachment<Self::Link>>> + Send + '_;
}

// ---------------------------------------------------------------------------
// StableConduit
// ---------------------------------------------------------------------------

// r[impl stable]
// r[impl zerocopy.framing.conduit.stable]
pub struct StableConduit<F: MsgFamily, LS: LinkSource> {
    shared: Arc<Shared<LS>>,
    _phantom: PhantomData<fn(F) -> F>,
}

struct Shared<LS: LinkSource> {
    inner: Mutex<Inner<LS>>,
    reconnecting: AtomicBool,
    reconnected: moire::sync::Notify,
}

struct Inner<LS: LinkSource> {
    source: Option<LS>,
    /// Incremented every time the link is replaced. Used to detect whether
    /// another task has already reconnected while we were waiting.
    link_generation: u64,
    tx: Option<<LS::Link as Link>::Tx>,
    rx: Option<<LS::Link as Link>::Rx>,
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
        let (link_tx, mut link_rx) = attachment.link.split();

        let (resume_key, _peer_last_received) =
            handshake::<LS::Link>(&link_tx, &mut link_rx, attachment.client_hello, None, None)
                .await?;

        let inner = Inner {
            source: Some(source),
            link_generation: 0,
            tx: Some(link_tx),
            rx: Some(link_rx),
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
            }),
            _phantom: PhantomData,
        })
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
        inner.resume_key = Some(new_resume_key);

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
    <LS::Link as Link>::Tx: Clone + Send + 'static,
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
    <LS::Link as Link>::Tx: Clone + Send + 'static,
    <LS::Link as Link>::Rx: Send + 'static,
    LS: Send + 'static,
{
    type Msg = F;
    type Permit<'a>
        = StableConduitPermit<F, LS>
    where
        Self: 'a;

    async fn reserve(&self) -> std::io::Result<Self::Permit<'_>> {
        loop {
            let (tx, generation) = {
                let inner = self
                    .shared
                    .lock_inner()
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                (inner.tx.clone(), inner.link_generation)
            };

            let tx = match tx {
                Some(tx) => tx,
                None => {
                    self.shared
                        .ensure_reconnected(generation)
                        .await
                        .map_err(|e| std::io::Error::other(e.to_string()))?;
                    continue;
                }
            };

            match tx.reserve().await {
                Ok(link_permit) => {
                    return Ok(StableConduitPermit {
                        shared: Arc::clone(&self.shared),
                        link_permit,
                        generation,
                        _phantom: PhantomData,
                    });
                }
                Err(_) => {
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

        let frame = Frame { seq, ack, item };

        // SAFETY: The shape matches `frame`'s concrete type for all lifetimes.
        // `Frame<F::Msg<'a>>` has a lifetime-independent shape by `MsgFamily` contract.
        #[allow(unsafe_code)]
        let peek = unsafe {
            Peek::unchecked_new(
                PtrConst::new((&raw const frame).cast::<u8>()),
                Frame::<F::Msg<'static>>::SHAPE,
            )
        };
        let plan =
            facet_postcard::peek_to_scatter_plan(peek).map_err(StableConduitError::Encode)?;

        let mut slot = link_permit
            .alloc(plan.total_size())
            .map_err(StableConduitError::Io)?;
        let slot_bytes = slot.as_mut_slice();
        plan.write_into(slot_bytes)
            .map_err(StableConduitError::Encode)?;

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

            // Phase 2: deserialize the frame.
            let frame: SelfRef<Frame<F::Msg<'static>>> =
                crate::deserialize_postcard(backing).map_err(StableConduitError::Decode)?;

            // Phase 3: update shared state; skip duplicates.
            // r[impl stable.seq.monotonic]
            // r[impl stable.ack.trim]
            let is_dup = {
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

            return Ok(Some(frame.map(|f| f.item)));
        }
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum StableConduitError {
    Encode(facet_postcard::SerializeError),
    Decode(facet_format::DeserializeError),
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
