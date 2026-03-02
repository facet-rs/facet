//! Shared-memory transport for roam.
//!
//! Implements [`Link`] over lock-free ring buffers in shared memory.
//! Inline bipbuf payloads are copied into boxed backing; slot-ref payloads are exposed
//! as shared zero-copy backing and freed when the backing is dropped.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use roam_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, SharedBacking, WriteSlot};
use shm_primitives::{BipBuf, PeerId};
use shm_primitives_async::{Doorbell, SignalResult};
use tracing::{debug, trace, warn};

use crate::framing::{DEFAULT_INLINE_THRESHOLD, MmapRef, OwnedFrame};
use crate::mmap_registry::{
    MmapAllocation, MmapAttachments, MmapChannelRx, MmapChannelTx, MmapRegistry,
};

pub mod bootstrap;
pub mod framing;
pub mod host;
pub mod mmap_registry;
pub mod peer_table;
pub mod segment;
pub mod varslot;

pub use segment::{AttachError, Segment, SegmentConfig, SegmentLayout};
pub use varslot::{SizeClassConfig, SlotRef, VarSlotPool};

pub use host::create_test_link_pair;
pub use host::{
    AddPeerOptions, GuestSpawnTicket, HostHub, HostPeer, MultiPeerHostDriver, PreparedPeer, ShmHost,
};
#[cfg(windows)]
pub use host::{guest_link_from_names, guest_link_from_ticket_windows};
#[cfg(unix)]
pub use host::{guest_link_from_raw, guest_link_from_ticket};

pub mod driver {
    pub use crate::host::{AddPeerOptions, MultiPeerHostDriver, ShmHost};
}

const SLOT_LEN_PREFIX_SIZE: usize = 4;

#[derive(Clone)]
struct Backend(Arc<Segment>);

impl Backend {
    fn allocate_slot(&self, size: u32, owner_peer: u8) -> Option<SlotRef> {
        self.0.var_pool().allocate(size, owner_peer)
    }

    fn free_slot(&self, slot_ref: SlotRef) {
        let _ = self.0.var_pool().free(slot_ref);
    }

    unsafe fn slot_data<'a>(&self, slot_ref: &SlotRef) -> &'a [u8] {
        unsafe { self.0.var_pool().slot_data(slot_ref) }
    }

    unsafe fn slot_data_mut<'a>(&self, slot_ref: &SlotRef) -> &'a mut [u8] {
        unsafe { self.0.var_pool().slot_data_mut(slot_ref) }
    }

    fn max_slot_size(&self) -> Option<u32> {
        let pool = self.0.var_pool();
        let class_count = pool.class_count();
        if class_count == 0 {
            return None;
        }
        let mut max_size = 0u32;
        for class_idx in 0..class_count {
            max_size = max_size.max(pool.slot_size(class_idx));
        }
        Some(max_size)
    }
}

struct TxShared {
    tx_bipbuf: Arc<BipBuf>,
    backend: Backend,
    owner_peer: u8,
    max_payload_size: u32,
    inline_threshold: u32,
    max_varslot_payload_size: Option<u32>,
    reserve_ring_bytes: u32,
    tx_lock: Mutex<()>,
    doorbell: Arc<Doorbell>,
    doorbell_dead: AtomicBool,
    stats: Arc<ShmTransportStats>,
    mmap_registry: Mutex<MmapRegistry>,
}

/// A [`Link`] over shared memory ring buffers.
// r[impl transport.shm]
// r[impl zerocopy.framing.link.shm]
pub struct ShmLink {
    tx_shared: Arc<TxShared>,
    rx_bipbuf: Arc<BipBuf>,
    rx_backend: Backend,
    tx_closed: Arc<AtomicBool>,
    peer_closed: Arc<AtomicBool>,
    mmap_attachments: MmapAttachments,
}

#[derive(Default)]
struct ShmTransportStats {
    inline_sends: AtomicU64,
    slot_ref_sends: AtomicU64,
    mmap_ref_sends: AtomicU64,
    inline_recvs: AtomicU64,
    slot_ref_recvs: AtomicU64,
    mmap_ref_recvs: AtomicU64,
    varslot_exhausted: AtomicU64,
    ring_exhausted: AtomicU64,
    reserve_waits: AtomicU64,
    commit_retries: AtomicU64,
    doorbell_peer_dead: AtomicU64,
    doorbell_wait_errors: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShmTransportStatsSnapshot {
    pub inline_sends: u64,
    pub slot_ref_sends: u64,
    pub mmap_ref_sends: u64,
    pub inline_recvs: u64,
    pub slot_ref_recvs: u64,
    pub mmap_ref_recvs: u64,
    pub varslot_exhausted: u64,
    pub ring_exhausted: u64,
    pub reserve_waits: u64,
    pub commit_retries: u64,
    pub doorbell_peer_dead: u64,
    pub doorbell_wait_errors: u64,
}

impl ShmTransportStats {
    fn snapshot(&self) -> ShmTransportStatsSnapshot {
        ShmTransportStatsSnapshot {
            inline_sends: self.inline_sends.load(AtomicOrdering::Relaxed),
            slot_ref_sends: self.slot_ref_sends.load(AtomicOrdering::Relaxed),
            mmap_ref_sends: self.mmap_ref_sends.load(AtomicOrdering::Relaxed),
            inline_recvs: self.inline_recvs.load(AtomicOrdering::Relaxed),
            slot_ref_recvs: self.slot_ref_recvs.load(AtomicOrdering::Relaxed),
            mmap_ref_recvs: self.mmap_ref_recvs.load(AtomicOrdering::Relaxed),
            varslot_exhausted: self.varslot_exhausted.load(AtomicOrdering::Relaxed),
            ring_exhausted: self.ring_exhausted.load(AtomicOrdering::Relaxed),
            reserve_waits: self.reserve_waits.load(AtomicOrdering::Relaxed),
            commit_retries: self.commit_retries.load(AtomicOrdering::Relaxed),
            doorbell_peer_dead: self.doorbell_peer_dead.load(AtomicOrdering::Relaxed),
            doorbell_wait_errors: self.doorbell_wait_errors.load(AtomicOrdering::Relaxed),
        }
    }
}

impl ShmLink {
    fn normalize_threshold(threshold: u32) -> u32 {
        if threshold == 0 {
            DEFAULT_INLINE_THRESHOLD
        } else {
            threshold
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        tx_bipbuf: Arc<BipBuf>,
        rx_bipbuf: Arc<BipBuf>,
        backend: Backend,
        doorbell: Arc<Doorbell>,
        owner_peer: u8,
        max_payload_size: u32,
        inline_threshold: u32,
        tx_closed: Arc<AtomicBool>,
        peer_closed: Arc<AtomicBool>,
        mmap_channel_tx: MmapChannelTx,
        mmap_channel_rx: MmapChannelRx,
    ) -> Self {
        let ring_capacity = tx_bipbuf.capacity();
        let ring_contiguous_ceiling = ring_capacity.saturating_sub(1);
        let inline_ceiling = max_payload_size
            .min(Self::normalize_threshold(inline_threshold))
            .min(ring_contiguous_ceiling.saturating_sub(framing::FRAME_HEADER_SIZE as u32));
        let reserve_inline_bytes = ((framing::FRAME_HEADER_SIZE as u32 + inline_ceiling) + 3) & !3;
        let reserve_ring_bytes = reserve_inline_bytes
            .max(framing::SLOT_REF_ENTRY_SIZE)
            .max(framing::MMAP_REF_ENTRY_SIZE)
            .min(ring_contiguous_ceiling);
        let max_varslot_payload_size = backend
            .max_slot_size()
            .and_then(|slot_size| slot_size.checked_sub(SLOT_LEN_PREFIX_SIZE as u32));

        let default_mmap_region_size = 1024 * 1024; // 1 MiB default
        let mmap_registry = MmapRegistry::new(mmap_channel_tx, default_mmap_region_size);

        let stats = Arc::new(ShmTransportStats::default());
        let tx_shared = Arc::new(TxShared {
            tx_bipbuf,
            backend: backend.clone(),
            owner_peer,
            max_payload_size,
            inline_threshold: Self::normalize_threshold(inline_threshold),
            max_varslot_payload_size,
            reserve_ring_bytes,
            tx_lock: Mutex::new(()),
            doorbell,
            doorbell_dead: AtomicBool::new(false),
            stats: stats.clone(),
            mmap_registry: Mutex::new(mmap_registry),
        });

        let mmap_attachments = MmapAttachments::new(mmap_channel_rx);

        Self {
            tx_shared,
            rx_bipbuf,
            rx_backend: backend,
            tx_closed,
            peer_closed,
            mmap_attachments,
        }
    }

    /// Build a guest-side SHM link from a shared segment.
    pub fn for_guest(
        segment: Arc<Segment>,
        peer_id: PeerId,
        doorbell: Doorbell,
        mmap_channel_tx: MmapChannelTx,
        mmap_channel_rx: MmapChannelRx,
    ) -> Self {
        let tx_bipbuf = Arc::new(segment.g2h_bipbuf(peer_id));
        let rx_bipbuf = Arc::new(segment.h2g_bipbuf(peer_id));
        let backend = Backend(segment.clone());

        Self::from_parts(
            tx_bipbuf,
            rx_bipbuf,
            backend,
            Arc::new(doorbell),
            peer_id.get(),
            segment.header().max_payload_size,
            segment.header().inline_threshold,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            mmap_channel_tx,
            mmap_channel_rx,
        )
    }

    /// Build a host-side SHM link for one peer from a shared segment.
    pub fn for_host(
        segment: Arc<Segment>,
        peer_id: PeerId,
        doorbell: Doorbell,
        mmap_channel_tx: MmapChannelTx,
        mmap_channel_rx: MmapChannelRx,
    ) -> Self {
        let tx_bipbuf = Arc::new(segment.h2g_bipbuf(peer_id));
        let rx_bipbuf = Arc::new(segment.g2h_bipbuf(peer_id));
        let backend = Backend(segment.clone());

        Self::from_parts(
            tx_bipbuf,
            rx_bipbuf,
            backend,
            Arc::new(doorbell),
            0,
            segment.header().max_payload_size,
            segment.header().inline_threshold,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            mmap_channel_tx,
            mmap_channel_rx,
        )
    }

    /// Accept the doorbell connection (Windows only).
    ///
    /// On Windows, the named pipe server must call `ConnectNamedPipe` before
    /// data can flow.  Call this on the **host** link after the guest has
    /// connected (i.e., after `Doorbell::from_handle` on the guest side).
    ///
    /// No-op on Unix.
    #[cfg(windows)]
    pub async fn accept_doorbell(&self) -> std::io::Result<()> {
        self.tx_shared.doorbell.accept().await
    }
}

/// Sending half of a [`ShmLink`].
pub struct ShmLinkTx {
    shared: Arc<TxShared>,
    tx_closed: Arc<AtomicBool>,
}

/// Receiving half of a [`ShmLink`].
pub struct ShmLinkRx {
    rx_bipbuf: Arc<BipBuf>,
    backend: Backend,
    peer_closed: Arc<AtomicBool>,
    doorbell: Arc<Doorbell>,
    stats: Arc<ShmTransportStats>,
    mmap_attachments: MmapAttachments,
}

pub struct ShmTxPermit {
    shared: Arc<TxShared>,
    tx_closed: Arc<AtomicBool>,
}

enum ShmWriteSlotInner {
    Inline {
        bytes: Vec<u8>,
    },
    VarSlot {
        slot_ref: Option<SlotRef>,
        payload_len: usize,
    },
    MmapRef {
        alloc: Option<MmapAllocation>,
        payload_len: usize,
    },
}

pub struct ShmWriteSlot {
    shared: Arc<TxShared>,
    inner: ShmWriteSlotInner,
}

impl Drop for ShmWriteSlot {
    fn drop(&mut self) {
        match &mut self.inner {
            ShmWriteSlotInner::VarSlot { slot_ref, .. } => {
                if let Some(slot_ref) = slot_ref.take() {
                    self.shared.backend.free_slot(slot_ref);
                    if matches!(self.shared.doorbell.signal_now(), SignalResult::PeerDead) {
                        self.shared.doorbell_dead.store(true, Ordering::Release);
                        self.shared
                            .stats
                            .doorbell_peer_dead
                            .fetch_add(1, AtomicOrdering::Relaxed);
                    }
                }
            }
            ShmWriteSlotInner::MmapRef { alloc, .. } => {
                // r[impl shm.mmap.release]
                if let Some(alloc) = alloc.take() {
                    alloc.lease_counter.fetch_sub(1, Ordering::Release);
                }
            }
            ShmWriteSlotInner::Inline { .. } => {}
        }
    }
}

impl Link for ShmLink {
    type Tx = ShmLinkTx;
    type Rx = ShmLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let tx_shared = self.tx_shared;
        let doorbell = tx_shared.doorbell.clone();
        let stats = tx_shared.stats.clone();
        (
            ShmLinkTx {
                shared: tx_shared,
                tx_closed: self.tx_closed,
            },
            ShmLinkRx {
                rx_bipbuf: self.rx_bipbuf,
                backend: self.rx_backend,
                peer_closed: self.peer_closed,
                doorbell,
                stats,
                mmap_attachments: self.mmap_attachments,
            },
        )
    }
}

impl LinkTx for ShmLinkTx {
    type Permit = ShmTxPermit;

    async fn reserve(&self) -> io::Result<Self::Permit> {
        loop {
            if self.tx_closed.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "shm tx is closed",
                ));
            }
            if self.shared.doorbell_dead.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "shm doorbell peer is closed",
                ));
            }
            if self
                .shared
                .tx_bipbuf
                .inner()
                .can_grant(self.shared.reserve_ring_bytes)
            {
                return Ok(ShmTxPermit {
                    shared: self.shared.clone(),
                    tx_closed: self.tx_closed.clone(),
                });
            }

            self.shared
                .stats
                .reserve_waits
                .fetch_add(1, AtomicOrdering::Relaxed);
            self.shared
                .stats
                .ring_exhausted
                .fetch_add(1, AtomicOrdering::Relaxed);
            if let Err(err) = self.shared.doorbell.wait().await {
                self.shared
                    .stats
                    .doorbell_wait_errors
                    .fetch_add(1, AtomicOrdering::Relaxed);
                warn!(
                    error = %err,
                    raw_os_error = ?err.raw_os_error(),
                    "shm tx doorbell wait failed"
                );
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    format!("shm doorbell wait failed: {err}"),
                ));
            }
        }
    }

    async fn close(self) -> io::Result<()> {
        self.tx_closed.store(true, Ordering::Release);
        match self.shared.doorbell.signal_now() {
            SignalResult::PeerDead => {
                self.shared.doorbell_dead.store(true, Ordering::Release);
                self.shared
                    .stats
                    .doorbell_peer_dead
                    .fetch_add(1, AtomicOrdering::Relaxed);
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "shm doorbell peer is closed",
                ))
            }
            _ => Ok(()),
        }
    }
}

// r[impl zerocopy.send.shm]
impl LinkTxPermit for ShmTxPermit {
    type Slot = ShmWriteSlot;

    fn alloc(self, len: usize) -> io::Result<Self::Slot> {
        if self.tx_closed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "shm tx is closed",
            ));
        }
        if len > self.shared.max_payload_size as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "payload exceeds max_payload_size",
            ));
        }

        if len as u32 <= self.shared.inline_threshold {
            return Ok(ShmWriteSlot {
                shared: self.shared.clone(),
                inner: ShmWriteSlotInner::Inline {
                    bytes: vec![0; len],
                },
            });
        }
        let needed = len.checked_add(SLOT_LEN_PREFIX_SIZE).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "payload length overflow while allocating slot-ref",
            )
        })?;
        let needed_u32 = u32::try_from(needed).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "payload length exceeds varslot addressing range",
            )
        })?;
        let max_varslot_payload = self.shared.max_varslot_payload_size.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "payload exceeds inline threshold but no varslot class is configured",
            )
        })?;
        if len as u32 > max_varslot_payload {
            // r[impl shm.mmap.ordering]
            // Payload exceeds varslot — use mmap-ref path.
            // Step 1: alloc delivers fd to peer (ordering: registry visible)
            let mut registry = self
                .shared
                .mmap_registry
                .lock()
                .expect("mmap registry poisoned");
            let alloc = registry
                .alloc(len)
                .map_err(|e| io::Error::other(format!("mmap alloc failed: {e}")))?;
            return Ok(ShmWriteSlot {
                shared: self.shared.clone(),
                inner: ShmWriteSlotInner::MmapRef {
                    alloc: Some(alloc),
                    payload_len: len,
                },
            });
        }
        let slot_ref = self
            .shared
            .backend
            .allocate_slot(needed_u32, self.shared.owner_peer)
            .ok_or_else(|| {
                self.shared
                    .stats
                    .varslot_exhausted
                    .fetch_add(1, AtomicOrdering::Relaxed);
                if matches!(self.shared.doorbell.signal_now(), SignalResult::PeerDead) {
                    self.shared.doorbell_dead.store(true, Ordering::Release);
                    self.shared
                        .stats
                        .doorbell_peer_dead
                        .fetch_add(1, AtomicOrdering::Relaxed);
                }
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "varslot exhausted; retry on next reserve/send cycle",
                )
            })?;

        Ok(ShmWriteSlot {
            shared: self.shared.clone(),
            inner: ShmWriteSlotInner::VarSlot {
                slot_ref: Some(slot_ref),
                payload_len: len,
            },
        })
    }
}

impl WriteSlot for ShmWriteSlot {
    fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.inner {
            ShmWriteSlotInner::Inline { bytes } => bytes.as_mut_slice(),
            ShmWriteSlotInner::VarSlot {
                slot_ref,
                payload_len,
            } => {
                let slot_ref = slot_ref
                    .as_ref()
                    .expect("slot must be present while write slot is alive");
                let end = SLOT_LEN_PREFIX_SIZE + *payload_len;
                let data = unsafe { self.shared.backend.slot_data_mut(slot_ref) };
                &mut data[SLOT_LEN_PREFIX_SIZE..end]
            }
            ShmWriteSlotInner::MmapRef { alloc, payload_len } => {
                let alloc = alloc
                    .as_mut()
                    .expect("mmap alloc must be present while write slot is alive");
                // SAFETY: We just allocated this range and no one else is reading it.
                unsafe { alloc.payload_mut(*payload_len) }
            }
        }
    }

    fn commit(mut self) {
        fn ring_doorbell(shared: &TxShared) {
            if matches!(shared.doorbell.signal_now(), SignalResult::PeerDead) {
                shared.doorbell_dead.store(true, Ordering::Release);
                shared
                    .stats
                    .doorbell_peer_dead
                    .fetch_add(1, AtomicOrdering::Relaxed);
            }
        }

        match &mut self.inner {
            ShmWriteSlotInner::Inline { bytes } => loop {
                let lock = self.shared.tx_lock.lock().expect("tx lock poisoned");
                let (mut producer, _) = self.shared.tx_bipbuf.split();
                let result = framing::write_inline(&mut producer, bytes);
                drop(lock);
                match result {
                    Ok(()) => {
                        self.shared
                            .stats
                            .inline_sends
                            .fetch_add(1, AtomicOrdering::Relaxed);
                        ring_doorbell(&self.shared);
                        return;
                    }
                    Err(_) => {
                        self.shared
                            .stats
                            .commit_retries
                            .fetch_add(1, AtomicOrdering::Relaxed);
                        std::thread::yield_now();
                    }
                }
            },
            ShmWriteSlotInner::VarSlot {
                slot_ref,
                payload_len,
            } => {
                let Some(slot_ref_value) = *slot_ref else {
                    return;
                };

                {
                    let payload_len_bytes = (*payload_len as u32).to_le_bytes();
                    let data = unsafe { self.shared.backend.slot_data_mut(&slot_ref_value) };
                    data[..SLOT_LEN_PREFIX_SIZE].copy_from_slice(&payload_len_bytes);
                }

                loop {
                    let lock = self.shared.tx_lock.lock().expect("tx lock poisoned");
                    let (mut producer, _) = self.shared.tx_bipbuf.split();
                    let result = framing::write_slot_ref(&mut producer, &slot_ref_value);
                    drop(lock);
                    match result {
                        Ok(()) => {
                            *slot_ref = None;
                            self.shared
                                .stats
                                .slot_ref_sends
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            ring_doorbell(&self.shared);
                            return;
                        }
                        Err(_) => {
                            self.shared
                                .stats
                                .commit_retries
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            std::thread::yield_now();
                        }
                    }
                }
            }
            // r[impl shm.mmap.ordering]
            ShmWriteSlotInner::MmapRef { alloc, payload_len } => {
                let Some(alloc_value) = alloc.take() else {
                    return;
                };

                // Step 2→3: bytes are initialized (caller wrote them), issue release fence
                std::sync::atomic::fence(Ordering::Release);

                let mmap_ref = MmapRef {
                    map_id: alloc_value.map_id,
                    map_generation: alloc_value.map_generation,
                    map_offset: alloc_value.map_offset,
                    payload_len: *payload_len as u32,
                };

                loop {
                    let lock = self.shared.tx_lock.lock().expect("tx lock poisoned");
                    let (mut producer, _) = self.shared.tx_bipbuf.split();
                    let result = framing::write_mmap_ref(&mut producer, &mmap_ref);
                    drop(lock);
                    match result {
                        Ok(()) => {
                            // The lease counter stays at 1 — the receiver will hold
                            // it via ShmMmapBacking and decrement on drop.
                            // alloc_value (no Drop impl) goes out of scope naturally.
                            self.shared
                                .stats
                                .mmap_ref_sends
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            ring_doorbell(&self.shared);
                            return;
                        }
                        Err(_) => {
                            self.shared
                                .stats
                                .commit_retries
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            std::thread::yield_now();
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum ShmLinkRxError {
    MmapResolve(crate::mmap_registry::MmapResolveError),
    DoorbellWait(io::Error),
    MalformedSlotRefLength {
        slot_bytes: usize,
        payload_len: usize,
    },
}

impl std::fmt::Display for ShmLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShmLinkRxError::MmapResolve(err) => write!(f, "mmap resolve failed: {err}"),
            ShmLinkRxError::DoorbellWait(err) => {
                write!(
                    f,
                    "doorbell wait failed: {} (raw_os_error={:?})",
                    err,
                    err.raw_os_error()
                )
            }
            ShmLinkRxError::MalformedSlotRefLength {
                slot_bytes,
                payload_len,
            } => write!(
                f,
                "malformed slot-ref payload length: payload_len={payload_len}, slot_bytes={slot_bytes}"
            ),
        }
    }
}

impl std::error::Error for ShmLinkRxError {}

impl LinkRx for ShmLinkRx {
    type Error = ShmLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        loop {
            let (_, mut consumer) = self.rx_bipbuf.split();
            if let Some(frame) = framing::read_frame(&mut consumer) {
                return match frame {
                    // r[impl zerocopy.recv.shm.inline]
                    // r[impl zerocopy.backing.bipbuf]
                    OwnedFrame::Inline(bytes) => {
                        trace!(len = bytes.len(), "shm rx received inline frame");
                        self.stats
                            .inline_recvs
                            .fetch_add(1, AtomicOrdering::Relaxed);
                        if matches!(self.doorbell.signal_now(), SignalResult::PeerDead) {
                            let was_closed = self.peer_closed.swap(true, Ordering::AcqRel);
                            self.stats
                                .doorbell_peer_dead
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            if !was_closed {
                                debug!("shm rx observed peer dead while draining inline frame");
                            }
                        }
                        Ok(Some(Backing::Boxed(bytes.into_boxed_slice())))
                    }
                    // r[impl zerocopy.recv.shm.slotref]
                    OwnedFrame::SlotRef(slot_ref) => {
                        trace!(slot_ref = ?slot_ref, "shm rx received slot-ref frame");
                        self.stats
                            .slot_ref_recvs
                            .fetch_add(1, AtomicOrdering::Relaxed);
                        if matches!(self.doorbell.signal_now(), SignalResult::PeerDead) {
                            let was_closed = self.peer_closed.swap(true, Ordering::AcqRel);
                            self.stats
                                .doorbell_peer_dead
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            if !was_closed {
                                debug!("shm rx observed peer dead while draining slot-ref frame");
                            }
                        }
                        let slot = unsafe { self.backend.slot_data(&slot_ref) };
                        if slot.len() < SLOT_LEN_PREFIX_SIZE {
                            self.backend.free_slot(slot_ref);
                            warn!(
                                slot_ref = ?slot_ref,
                                slot_bytes = slot.len(),
                                "shm rx malformed slot-ref: missing payload length prefix"
                            );
                            return Err(ShmLinkRxError::MalformedSlotRefLength {
                                slot_bytes: slot.len(),
                                payload_len: 0,
                            });
                        }

                        let payload_len =
                            u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]) as usize;
                        if payload_len > slot.len().saturating_sub(SLOT_LEN_PREFIX_SIZE) {
                            self.backend.free_slot(slot_ref);
                            warn!(
                                slot_ref = ?slot_ref,
                                slot_bytes = slot.len(),
                                payload_len,
                                "shm rx malformed slot-ref: payload length exceeds slot size"
                            );
                            return Err(ShmLinkRxError::MalformedSlotRefLength {
                                slot_bytes: slot.len(),
                                payload_len,
                            });
                        }
                        trace!(
                            slot_ref = ?slot_ref,
                            slot_bytes = slot.len(),
                            payload_len,
                            "shm rx slot-ref frame decoded"
                        );

                        Ok(Some(Backing::shared(Arc::new(ShmVarSlotBacking {
                            backend: self.backend.clone(),
                            slot_ref,
                            payload_len,
                            doorbell: self.doorbell.clone(),
                            peer_closed: self.peer_closed.clone(),
                            stats: self.stats.clone(),
                        }))))
                    }
                    // r[impl zerocopy.recv.shm.mmap]
                    OwnedFrame::MmapRef(mmap_ref) => {
                        trace!(
                            map_id = mmap_ref.map_id,
                            map_generation = mmap_ref.map_generation,
                            map_offset = mmap_ref.map_offset,
                            payload_len = mmap_ref.payload_len,
                            "shm rx received mmap-ref frame"
                        );
                        self.stats
                            .mmap_ref_recvs
                            .fetch_add(1, AtomicOrdering::Relaxed);
                        if matches!(self.doorbell.signal_now(), SignalResult::PeerDead) {
                            let was_closed = self.peer_closed.swap(true, Ordering::AcqRel);
                            self.stats
                                .doorbell_peer_dead
                                .fetch_add(1, AtomicOrdering::Relaxed);
                            if !was_closed {
                                debug!("shm rx observed peer dead while draining mmap-ref frame");
                            }
                        }

                        let mapping = match self.mmap_attachments.resolve_with_grace(
                            mmap_ref.map_id,
                            mmap_ref.map_generation,
                            mmap_ref.map_offset,
                            mmap_ref.payload_len,
                        ) {
                            Ok(mapping) => mapping,
                            Err(error) => {
                                warn!(
                                    map_id = mmap_ref.map_id,
                                    map_generation = mmap_ref.map_generation,
                                    map_offset = mmap_ref.map_offset,
                                    payload_len = mmap_ref.payload_len,
                                    error = %error,
                                    "shm rx mmap-ref resolve failed"
                                );
                                // r[impl shm.mmap.attach.protocol-error]
                                return Err(ShmLinkRxError::MmapResolve(error));
                            }
                        };

                        // r[impl zerocopy.backing.mmap]
                        trace!(
                            map_id = mmap_ref.map_id,
                            map_generation = mmap_ref.map_generation,
                            map_offset = mmap_ref.map_offset,
                            payload_len = mmap_ref.payload_len,
                            "shm rx mmap-ref resolved"
                        );
                        Ok(Some(Backing::shared(Arc::new(ShmMmapBacking {
                            mapping,
                            offset: mmap_ref.map_offset as usize,
                            len: mmap_ref.payload_len as usize,
                        }))))
                    }
                };
            }

            if self.peer_closed.load(Ordering::Acquire) && self.rx_bipbuf.inner().is_empty() {
                debug!("shm rx returning EOF: peer closed and rx bipbuf is empty");
                return Ok(None);
            }

            trace!("shm rx waiting on doorbell");
            if let Err(err) = self.doorbell.wait().await {
                self.stats
                    .doorbell_wait_errors
                    .fetch_add(1, AtomicOrdering::Relaxed);
                warn!(
                    error = %err,
                    raw_os_error = ?err.raw_os_error(),
                    "shm rx doorbell wait failed"
                );
                return Err(ShmLinkRxError::DoorbellWait(err));
            }
            trace!("shm rx woke from doorbell wait");
        }
    }
}

impl ShmLinkTx {
    pub fn stats(&self) -> ShmTransportStatsSnapshot {
        self.shared.stats.snapshot()
    }
}

impl ShmLinkRx {
    pub fn stats(&self) -> ShmTransportStatsSnapshot {
        self.stats.snapshot()
    }
}

// r[impl zerocopy.backing.varslot]
struct ShmVarSlotBacking {
    backend: Backend,
    slot_ref: SlotRef,
    payload_len: usize,
    doorbell: Arc<Doorbell>,
    peer_closed: Arc<AtomicBool>,
    stats: Arc<ShmTransportStats>,
}

impl SharedBacking for ShmVarSlotBacking {
    fn as_bytes(&self) -> &[u8] {
        let slot = unsafe { self.backend.slot_data(&self.slot_ref) };
        let end = SLOT_LEN_PREFIX_SIZE + self.payload_len;
        &slot[SLOT_LEN_PREFIX_SIZE..end]
    }
}

impl Drop for ShmVarSlotBacking {
    fn drop(&mut self) {
        self.backend.free_slot(self.slot_ref);
        if matches!(self.doorbell.signal_now(), SignalResult::PeerDead) {
            self.peer_closed.store(true, Ordering::Release);
            self.stats
                .doorbell_peer_dead
                .fetch_add(1, AtomicOrdering::Relaxed);
        }
    }
}

// r[impl zerocopy.backing.mmap]
struct ShmMmapBacking {
    mapping: Arc<mmap_registry::AttachedMapping>,
    offset: usize,
    len: usize,
}

impl SharedBacking for ShmMmapBacking {
    fn as_bytes(&self) -> &[u8] {
        let region = self.mapping.region.region();
        let ptr = region.as_ptr();
        // SAFETY: offset+len was bounds-checked during resolve
        unsafe { std::slice::from_raw_parts(ptr.add(self.offset), self.len) }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use roam_types::{LinkRx as _, LinkTx as _, LinkTxPermit as _};
    use shm_primitives::FileCleanup;
    use tokio::time::timeout;

    use super::*;
    use crate::host::create_test_link_pair;
    use crate::segment::SegmentConfig;

    /// Create a real segment-backed test link pair.
    async fn make_test_pair(
        bipbuf_capacity: u32,
        max_payload_size: u32,
        inline_threshold: u32,
        size_classes: &[SizeClassConfig],
    ) -> (ShmLink, ShmLink, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.shm");
        let segment = Arc::new(
            Segment::create(
                &path,
                SegmentConfig {
                    max_guests: 1,
                    bipbuf_capacity,
                    max_payload_size,
                    inline_threshold,
                    heartbeat_interval: 0,
                    size_classes,
                },
                FileCleanup::Manual,
            )
            .expect("create segment"),
        );
        let (a, b) = create_test_link_pair(segment)
            .await
            .expect("create_test_link_pair");
        (a, b, dir)
    }

    const CLASSES: &[SizeClassConfig] = &[SizeClassConfig {
        slot_size: 256,
        slot_count: 1,
    }];

    #[tokio::test]
    async fn inline_payload_roundtrip_is_boxed() {
        let (a, b, _dir) = make_test_pair(4096, 1024, 128, CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        let payload = b"inline hello";
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(payload);
        slot.commit();

        let backing = b_rx.recv().await.unwrap().unwrap();
        match backing {
            Backing::Boxed(bytes) => assert_eq!(&*bytes, payload),
            Backing::Shared(_) => panic!("inline path must be boxed"),
        }
    }

    #[tokio::test]
    async fn slot_ref_payload_is_zero_copy_shared_backing() {
        let (a, b, _dir) = make_test_pair(4096, 1024, 64, CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        let payload = vec![7_u8; 200];
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(&payload);
        slot.commit();

        let backing = b_rx.recv().await.unwrap().unwrap();
        match backing {
            Backing::Shared(shared) => assert_eq!(shared.as_bytes(), payload.as_slice()),
            Backing::Boxed(_) => panic!("slot-ref path must be shared"),
        }
    }

    #[tokio::test]
    async fn shared_backing_drop_releases_slot() {
        let (a, b, _dir) = make_test_pair(4096, 1024, 64, CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        let payload = vec![1_u8; 200];
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(&payload);
        slot.commit();

        let backing = b_rx.recv().await.unwrap().unwrap();

        // single-slot pool: reserve stays size-agnostic and alloc fails immediately.
        let permit2 = a_tx.reserve().await.unwrap();
        match permit2.alloc(payload.len()) {
            Ok(_) => panic!("alloc should fail while slot is still held by shared backing"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::WouldBlock),
        }

        drop(backing);

        let permit3 = a_tx.reserve().await.unwrap();
        let _slot3 = permit3
            .alloc(payload.len())
            .expect("slot must be released after drop");
    }

    #[tokio::test]
    async fn mixed_payload_stress_roundtrip() {
        let classes = [SizeClassConfig {
            slot_size: 4096,
            slot_count: 32,
        }];
        let (a, b, _dir) = make_test_pair(1 << 16, 1 << 20, 256, &classes).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        for i in 0..400 {
            let len = if i % 3 == 0 { 48 } else { 1500 };
            let payload = vec![(i % 239) as u8; len];
            let permit = a_tx.reserve().await.unwrap();
            let mut slot = permit.alloc(payload.len()).unwrap();
            slot.as_mut_slice().copy_from_slice(&payload);
            slot.commit();

            let backing = b_rx.recv().await.unwrap().unwrap();
            assert_eq!(backing.as_bytes(), payload.as_slice());
        }
    }

    #[tokio::test]
    async fn reserve_waits_until_rx_frees_ring_space() {
        let (a, b, _dir) = make_test_pair(32, 1024, 256, CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        let payload = vec![9_u8; 24]; // align4(8 + 24) = 32 (fills ring)
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(&payload);
        slot.commit();

        let reserve_fut = a_tx.reserve();
        tokio::pin!(reserve_fut);
        assert!(
            timeout(Duration::from_millis(20), &mut reserve_fut)
                .await
                .is_err(),
            "reserve should wait while ring is full"
        );

        let _ = b_rx.recv().await.unwrap().unwrap();
        let permit2 = timeout(Duration::from_secs(1), &mut reserve_fut)
            .await
            .expect("reserve should wake after recv")
            .expect("reserve should succeed");
        drop(permit2);
    }

    #[tokio::test]
    async fn transport_stats_track_send_recv_and_exhaustion() {
        let (a, b, _dir) = make_test_pair(4096, 1024, 64, CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        let inline = b"hello";
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(inline.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(inline);
        slot.commit();

        let large = vec![1_u8; 200];
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(large.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(&large);
        slot.commit();

        let backing1 = b_rx.recv().await.unwrap().unwrap();
        assert!(backing1.as_bytes().starts_with(inline));
        let backing2 = b_rx.recv().await.unwrap().unwrap();
        assert_eq!(backing2.as_bytes(), large.as_slice());

        let permit = a_tx.reserve().await.unwrap();
        match permit.alloc(large.len()) {
            Ok(_) => panic!("alloc should fail while varslot is exhausted"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::WouldBlock),
        }

        drop(backing2); // free slot for future sends
        let permit = a_tx.reserve().await.unwrap();
        let _slot = permit.alloc(large.len()).expect("slot should be available");

        let tx_stats = a_tx.stats();
        let rx_stats = b_rx.stats();

        assert_eq!(tx_stats.inline_sends, 1);
        assert_eq!(tx_stats.slot_ref_sends, 1);
        assert!(tx_stats.varslot_exhausted >= 1);
        assert_eq!(rx_stats.inline_recvs, 1);
        assert_eq!(rx_stats.slot_ref_recvs, 1);
    }

    // Small varslot class to force mmap path for payloads > 60 bytes
    const SMALL_CLASSES: &[SizeClassConfig] = &[SizeClassConfig {
        slot_size: 64,
        slot_count: 2,
    }];

    #[tokio::test]
    async fn mmap_large_payload_roundtrip() {
        let (a, b, _dir) = make_test_pair(4096, 1 << 20, 32, SMALL_CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        // 200 bytes exceeds max varslot payload (64 - 4 = 60 bytes) → mmap path
        let payload = vec![0xAB_u8; 200];
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(&payload);
        slot.commit();

        let backing = b_rx.recv().await.unwrap().unwrap();
        match &backing {
            Backing::Shared(shared) => assert_eq!(shared.as_bytes(), payload.as_slice()),
            Backing::Boxed(_) => panic!("mmap path must be shared"),
        }

        let tx_stats = a_tx.stats();
        let rx_stats = b_rx.stats();
        assert_eq!(tx_stats.mmap_ref_sends, 1);
        assert_eq!(rx_stats.mmap_ref_recvs, 1);
    }

    #[tokio::test]
    async fn mmap_multiple_payloads_share_region() {
        let (a, b, _dir) = make_test_pair(4096, 1 << 20, 32, SMALL_CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        // Send two mmap payloads — they should share the same region
        for i in 0u8..3 {
            let payload = vec![i; 200];
            let permit = a_tx.reserve().await.unwrap();
            let mut slot = permit.alloc(payload.len()).unwrap();
            slot.as_mut_slice().copy_from_slice(&payload);
            slot.commit();
        }

        for i in 0u8..3 {
            let backing = b_rx.recv().await.unwrap().unwrap();
            assert_eq!(backing.as_bytes(), &vec![i; 200]);
        }

        assert_eq!(a_tx.stats().mmap_ref_sends, 3);
        assert_eq!(b_rx.stats().mmap_ref_recvs, 3);
    }

    #[tokio::test]
    async fn mmap_mixed_with_inline_and_varslot() {
        let (a, b, _dir) = make_test_pair(4096, 1 << 20, 32, SMALL_CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        // Inline (≤32 bytes)
        let inline_payload = b"hello";
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(inline_payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(inline_payload);
        slot.commit();

        // Varslot (33..=60 bytes)
        let varslot_payload = vec![0x42_u8; 50];
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(varslot_payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(&varslot_payload);
        slot.commit();

        // Mmap (>60 bytes)
        let mmap_payload = vec![0xFF_u8; 500];
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(mmap_payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(&mmap_payload);
        slot.commit();

        let b1 = b_rx.recv().await.unwrap().unwrap();
        assert!(b1.as_bytes().starts_with(inline_payload));

        let b2 = b_rx.recv().await.unwrap().unwrap();
        assert_eq!(b2.as_bytes(), varslot_payload.as_slice());

        let b3 = b_rx.recv().await.unwrap().unwrap();
        assert_eq!(b3.as_bytes(), mmap_payload.as_slice());

        let stats = a_tx.stats();
        assert_eq!(stats.inline_sends, 1);
        assert_eq!(stats.slot_ref_sends, 1);
        assert_eq!(stats.mmap_ref_sends, 1);
    }

    #[tokio::test]
    async fn mmap_backing_survives_rx_drop_and_peer_teardown() {
        let (a, b, _dir) = make_test_pair(4096, 1 << 20, 32, SMALL_CLASSES).await;
        let (a_tx, _a_rx) = a.split();
        let (_b_tx, mut b_rx) = b.split();

        let payload = vec![0x5A_u8; 500];
        let permit = a_tx.reserve().await.unwrap();
        let mut slot = permit.alloc(payload.len()).unwrap();
        slot.as_mut_slice().copy_from_slice(&payload);
        slot.commit();

        let backing = b_rx.recv().await.unwrap().unwrap();
        let shared = match backing {
            Backing::Shared(shared) => shared,
            Backing::Boxed(_) => panic!("expected mmap-backed shared payload"),
        };

        // Tear down receiver state (drops MmapAttachments) and peer tx.
        drop(b_rx);
        drop(a_tx);

        // Backing must remain valid independently of attachment table/peer lifetime.
        assert_eq!(shared.as_bytes(), payload.as_slice());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mmap_ref_can_arrive_before_attach_control_message() {
        let (_a, b, _dir) = make_test_pair(4096, 1 << 20, 32, SMALL_CLASSES).await;
        let (_b_tx, mut b_rx) = b.split();

        // Replace the default control channel so this test can
        // intentionally delay the attach message relative to the data frame.
        let (control_tx, control_rx) =
            shm_primitives_async::create_mmap_control_pair_connected().unwrap();
        b_rx.mmap_attachments = MmapAttachments::new(MmapChannelRx::Real(control_rx));

        let payload = b"late mmap attach payload".to_vec();
        let map_id = 72_u32;
        let map_generation = 1_u32;
        let mapping_length = 4096_u64;
        let mmap_ref = MmapRef {
            map_id,
            map_generation,
            map_offset: 0,
            payload_len: payload.len() as u32,
        };

        // Enqueue an mmap-ref frame first (before the attach control message).
        let (mut producer, _) = b_rx.rx_bipbuf.split();
        framing::write_mmap_ref(&mut producer, &mmap_ref).unwrap();

        // Build the mapping and schedule attach delivery shortly after recv starts.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("late-attach.shm");
        let region =
            shm_primitives::MmapRegion::create(&path, mapping_length as usize, FileCleanup::Manual)
                .unwrap();
        unsafe {
            std::ptr::copy_nonoverlapping(
                payload.as_ptr(),
                region.region().as_ptr(),
                payload.len(),
            );
        }
        let attach = shm_primitives_async::MmapAttachMessage {
            map_id,
            map_generation,
            mapping_length,
        };

        let control_tx = Arc::new(control_tx);
        let delayed_tx = Arc::clone(&control_tx);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            delayed_tx.send(region.as_raw_fd(), &attach).unwrap();
        });

        let backing = timeout(Duration::from_secs(1), b_rx.recv())
            .await
            .expect("recv timed out")
            .expect("recv should succeed despite delayed attach")
            .expect("expected payload");
        assert_eq!(backing.as_bytes(), payload.as_slice());
    }
}
