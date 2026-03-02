//! Thin SHM host/guest orchestration helpers.
//!
//! This module intentionally stays small: SHM remains "just a transport".
//! It only packages repetitive setup for segment peers, doorbells, and mmap
//! control channels.

use std::io;
use std::sync::Arc;

use shm_primitives::PeerId;
#[cfg(windows)]
use shm_primitives_async::create_mmap_control_receiver_server;
use shm_primitives_async::{
    Doorbell, DoorbellHandle, MmapControlHandle, MmapControlReceiver, MmapControlSender,
    create_mmap_control_pair,
};

use crate::ShmLink;
use crate::mmap_registry::{MmapChannelRx, MmapChannelTx};
use crate::segment::Segment;

fn io_other(msg: impl Into<String>) -> io::Error {
    io::Error::other(msg.into())
}

#[cfg(unix)]
use std::os::unix::io::RawFd;

#[cfg(unix)]
fn dup_fd(fd: RawFd) -> io::Result<RawFd> {
    // SAFETY: libc::dup does not take ownership of `fd`.
    let duplicated = unsafe { libc::dup(fd) };
    if duplicated < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(duplicated)
    }
}

/// Host-side SHM orchestrator over an existing segment.
///
/// Owns no runtime tasks; it only creates per-peer setup bundles.
pub struct HostHub {
    segment: Arc<Segment>,
}

impl HostHub {
    /// Create a new host hub over an already-created segment.
    pub fn new(segment: Arc<Segment>) -> Self {
        Self { segment }
    }

    /// Access the shared segment.
    pub fn segment(&self) -> &Arc<Segment> {
        &self.segment
    }

    /// Reserve a peer slot and prepare both host+guest IPC endpoints.
    ///
    /// The returned [`PreparedPeer`] contains:
    /// - host-side runtime parts (`HostPeer`)
    /// - guest-side spawn ticket (`GuestSpawnTicket`)
    pub fn prepare_peer(&self) -> io::Result<PreparedPeer> {
        let peer_id = self
            .segment
            .reserve_peer()
            .ok_or_else(|| io_other("no free SHM peer slots"))?;

        let (host_doorbell, guest_doorbell) = Doorbell::create_pair()?;

        // On Unix, clear CLOEXEC so the guest process inherits the doorbell fd.
        #[cfg(unix)]
        shm_primitives_async::clear_cloexec(guest_doorbell.as_raw_fd())?;

        // Create the mmap control channel pair.
        let (host_mmap_tx, guest_mmap_rx) = create_mmap_control_pair()?;

        #[cfg(unix)]
        let (host, guest) = {
            // On Unix the socketpair is bidirectional.  Duplicate each side's
            // endpoint so host and guest each get independent tx + rx handles.
            shm_primitives_async::clear_cloexec(guest_mmap_rx.as_raw_fd())?;
            let host_mmap_rx_fd = dup_fd(host_mmap_tx.as_raw_fd())?;
            let guest_mmap_tx_fd = dup_fd(guest_mmap_rx.as_raw_fd())?;
            shm_primitives_async::clear_cloexec(guest_mmap_tx_fd)?;

            let host = HostPeer {
                segment: Arc::clone(&self.segment),
                peer_id,
                doorbell: host_doorbell,
                mmap_tx: host_mmap_tx,
                // SAFETY: `dup_fd` returns a fresh owning descriptor.
                mmap_rx: unsafe { MmapControlHandle::from_raw_fd(host_mmap_rx_fd) },
            };
            let guest = GuestSpawnTicket {
                peer_id,
                doorbell: guest_doorbell,
                mmap_rx: guest_mmap_rx,
                mmap_tx_fd: guest_mmap_tx_fd,
            };
            (host, guest)
        };

        #[cfg(windows)]
        let (host, guest) = {
            // On Windows, mmap control uses named pipes.  Two separate
            // unidirectional channels, one per direction.
            //
            // host→guest: host keeps the server/sender, guest connects as
            //   client/receiver (via pipe name in bootstrap response).
            // guest→host: host keeps the server/receiver, guest connects as
            //   client/sender (via SHM_MMAP_TX_PIPE env var).
            let (host_mmap_rx, mmap_tx_pipe) = create_mmap_control_receiver_server()?;

            let host = HostPeer {
                segment: Arc::clone(&self.segment),
                peer_id,
                doorbell: host_doorbell,
                mmap_tx: host_mmap_tx,
                mmap_rx: host_mmap_rx,
            };
            let guest = GuestSpawnTicket {
                peer_id,
                doorbell: guest_doorbell,
                mmap_rx: guest_mmap_rx,
                mmap_tx_pipe,
            };
            (host, guest)
        };

        Ok(PreparedPeer { host, guest })
    }
}

/// Host-side resources for one reserved peer.
pub struct HostPeer {
    segment: Arc<Segment>,
    peer_id: PeerId,
    doorbell: Doorbell,
    mmap_tx: MmapControlSender,
    /// On Unix: pipe name handle (client connects later in `into_link`).
    /// On Windows: already-constructed receiver (host keeps the server handle).
    #[cfg(unix)]
    mmap_rx: MmapControlHandle,
    #[cfg(windows)]
    mmap_rx: MmapControlReceiver,
}

impl HostPeer {
    /// Peer id reserved in the segment.
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Build a host-side [`ShmLink`] for this peer.
    pub fn into_link(self) -> io::Result<ShmLink> {
        #[cfg(unix)]
        let mmap_rx = MmapControlReceiver::from_handle(self.mmap_rx)?;
        #[cfg(windows)]
        let mmap_rx = self.mmap_rx;
        Ok(ShmLink::for_host(
            self.segment,
            self.peer_id,
            self.doorbell,
            MmapChannelTx::Real(self.mmap_tx),
            MmapChannelRx::Real(mmap_rx),
        ))
    }

    /// Release the reserved peer slot if startup/spawn failed.
    pub fn release_reservation(self) {
        self.segment.release_reserved_peer(self.peer_id);
    }
}

/// Guest-side spawn payload for one peer.
///
/// This is meant to be turned into command-line args or env vars during spawn.
pub struct GuestSpawnTicket {
    pub peer_id: PeerId,
    pub doorbell: DoorbellHandle,
    pub mmap_rx: MmapControlHandle,
    /// Raw fd for the guest→host mmap control sender endpoint (Unix).
    #[cfg(unix)]
    pub mmap_tx_fd: RawFd,
    /// Pipe name for the guest→host mmap control channel (Windows).
    /// The guest calls `MmapControlSender::connect()` on this name;
    /// the host keeps the server handle in `HostPeer.mmap_rx`.
    #[cfg(windows)]
    pub mmap_tx_pipe: String,
}

impl GuestSpawnTicket {
    pub fn doorbell_arg(&self) -> String {
        self.doorbell.to_arg()
    }

    pub fn mmap_rx_arg(&self) -> String {
        self.mmap_rx.to_arg()
    }

    pub fn mmap_tx_arg(&self) -> String {
        #[cfg(unix)]
        {
            self.mmap_tx_fd.to_string()
        }
        #[cfg(windows)]
        {
            self.mmap_tx_pipe.clone()
        }
    }
}

/// Prepared peer bundle with host+guest sides.
pub struct PreparedPeer {
    host: HostPeer,
    guest: GuestSpawnTicket,
}

impl PreparedPeer {
    pub fn into_parts(self) -> (HostPeer, GuestSpawnTicket) {
        (self.host, self.guest)
    }
}

/// Backward-compat host options placeholder.
///
/// v7 currently has no peer-add tunables in this helper layer.
#[derive(Debug, Clone, Copy, Default)]
pub struct AddPeerOptions;

/// Backward-compat alias for users migrating from older SHM host APIs.
pub type MultiPeerHostDriver = ShmHost;

/// Thin compatibility wrapper around [`HostHub`].
///
/// This keeps old call sites compiling while using the v7 primitive orchestration.
pub struct ShmHost {
    hub: HostHub,
}

impl ShmHost {
    pub fn new(segment: Arc<Segment>) -> Self {
        Self {
            hub: HostHub::new(segment),
        }
    }

    pub fn segment(&self) -> &Arc<Segment> {
        self.hub.segment()
    }

    pub fn add_peer(&self, _options: AddPeerOptions) -> io::Result<PreparedPeer> {
        self.hub.prepare_peer()
    }
}

// ---------------------------------------------------------------------------
// Unix: guest link from inherited raw fds
// ---------------------------------------------------------------------------

/// Build a guest-side link from inherited raw fds.
///
/// `claim_reserved` should be true for spawned guests that were pre-reserved by
/// the host (`reserve_peer`). For walk-in attach flows, pass false and call
/// `Segment::attach_peer` separately.
///
/// # Safety
///
/// The passed fds must be valid inherited descriptors, uniquely owned by the
/// caller, and must match the same peer/segment context.
#[cfg(unix)]
pub unsafe fn guest_link_from_raw(
    segment: Arc<Segment>,
    peer_id: PeerId,
    doorbell_fd: RawFd,
    mmap_rx_fd: RawFd,
    mmap_tx_fd: RawFd,
    claim_reserved: bool,
) -> io::Result<ShmLink> {
    if claim_reserved {
        segment.claim_peer(peer_id).map_err(|state| {
            io_other(format!(
                "failed to claim reserved peer {} (state: {state:?})",
                peer_id.get()
            ))
        })?;
    }

    // SAFETY: caller guarantees fd ownership/validity.
    let doorbell = unsafe { Doorbell::from_handle(DoorbellHandle::from_raw_fd(doorbell_fd)) }?;
    // SAFETY: caller guarantees fd ownership/validity.
    let mmap_rx =
        MmapControlReceiver::from_handle(unsafe { MmapControlHandle::from_raw_fd(mmap_rx_fd) })?;
    // SAFETY: caller guarantees fd ownership/validity.
    let mmap_tx = unsafe { MmapControlSender::from_raw_fd(mmap_tx_fd) };

    Ok(ShmLink::for_guest(
        segment,
        peer_id,
        doorbell,
        MmapChannelTx::Real(mmap_tx),
        MmapChannelRx::Real(mmap_rx),
    ))
}

/// Build a guest-side link from a host-generated spawn ticket.
///
/// # Safety
///
/// Same requirements as [`guest_link_from_raw`].
#[cfg(unix)]
pub unsafe fn guest_link_from_ticket(
    segment: Arc<Segment>,
    ticket: GuestSpawnTicket,
    claim_reserved: bool,
) -> io::Result<ShmLink> {
    let peer_id = ticket.peer_id;
    let doorbell_fd = ticket.doorbell.into_raw_fd();
    let mmap_rx_fd = ticket.mmap_rx.into_raw_fd();
    let mmap_tx_fd = ticket.mmap_tx_fd;

    // SAFETY: ownership transferred out of `ticket` via into_raw_fd above.
    unsafe {
        guest_link_from_raw(
            segment,
            peer_id,
            doorbell_fd,
            mmap_rx_fd,
            mmap_tx_fd,
            claim_reserved,
        )
    }
}

// ---------------------------------------------------------------------------
// Windows: guest link from named-pipe names
// ---------------------------------------------------------------------------

/// Build a guest-side link from named pipe names received via bootstrap.
///
/// On Windows there are no inherited fds; instead the bootstrap response
/// carries pipe names that the guest connects to.
#[cfg(windows)]
pub fn guest_link_from_names(
    segment: Arc<Segment>,
    peer_id: PeerId,
    doorbell_pipe: &str,
    mmap_rx_pipe: &str,
    mmap_tx_pipe: &str,
    claim_reserved: bool,
) -> io::Result<ShmLink> {
    if claim_reserved {
        segment.claim_peer(peer_id).map_err(|state| {
            io_other(format!(
                "failed to claim reserved peer {} (state: {state:?})",
                peer_id.get()
            ))
        })?;
    }

    let doorbell =
        Doorbell::from_handle(DoorbellHandle::from_pipe_name(doorbell_pipe.to_string()))?;

    let mmap_rx_handle = unsafe { MmapControlHandle::from_arg(mmap_rx_pipe) }
        .map_err(|e| io_other(format!("mmap_rx pipe name: {e}")))?;
    let mmap_rx = MmapControlReceiver::from_handle(mmap_rx_handle)?;

    // Guest→host direction: connect as client/sender to the host's pipe server.
    let mmap_tx_sender = MmapControlSender::connect(mmap_tx_pipe)?;

    Ok(ShmLink::for_guest(
        segment,
        peer_id,
        doorbell,
        MmapChannelTx::Real(mmap_tx_sender),
        MmapChannelRx::Real(mmap_rx),
    ))
}

/// Build a guest-side link from a host-generated ticket (Windows).
///
/// The guest connects as pipe client/sender to the host's receiver server
/// pipe named in `ticket.mmap_tx_pipe`.
#[cfg(windows)]
pub fn guest_link_from_ticket_windows(
    segment: Arc<Segment>,
    ticket: GuestSpawnTicket,
    claim_reserved: bool,
) -> io::Result<ShmLink> {
    if claim_reserved {
        segment.claim_peer(ticket.peer_id).map_err(|state| {
            io_other(format!(
                "failed to claim reserved peer {} (state: {state:?})",
                ticket.peer_id.get()
            ))
        })?;
    }

    let doorbell = Doorbell::from_handle(ticket.doorbell)?;
    let mmap_rx = MmapControlReceiver::from_handle(ticket.mmap_rx)?;
    let mmap_tx = MmapControlSender::connect(&ticket.mmap_tx_pipe)?;

    Ok(ShmLink::for_guest(
        segment,
        ticket.peer_id,
        doorbell,
        MmapChannelTx::Real(mmap_tx),
        MmapChannelRx::Real(mmap_rx),
    ))
}

/// Create a real SHM link pair for testing.
///
/// Uses a file-backed segment with real doorbells and real mmap control
/// channels — no in-process fakes.
///
/// **Ordering note (Windows):** `host_peer.into_link()` is called before
/// `guest_link_from_ticket_windows` so that the host connects as pipe client
/// first, satisfying named-pipe `ConnectNamedPipe` ordering.
pub async fn create_test_link_pair(segment: Arc<Segment>) -> io::Result<(ShmLink, ShmLink)> {
    let hub = HostHub::new(Arc::clone(&segment));
    let prepared = hub.prepare_peer()?;
    let (host_peer, ticket) = prepared.into_parts();
    // Host must build its link first (connects as pipe client on Windows).
    let host_link = host_peer.into_link()?;
    #[cfg(unix)]
    let guest_link = unsafe { guest_link_from_ticket(segment, ticket, true) }?;
    #[cfg(windows)]
    let guest_link = guest_link_from_ticket_windows(segment, ticket, true)?;
    // On Windows, the host doorbell (named pipe server) must call
    // ConnectNamedPipe after the guest client connects.
    #[cfg(windows)]
    host_link.accept_doorbell().await?;
    Ok((host_link, guest_link))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::segment::SegmentConfig;
    use crate::varslot::SizeClassConfig;
    use roam_types::{Link, LinkRx, LinkTx, LinkTxPermit, WriteSlot};
    use shm_primitives::FileCleanup;

    #[tokio::test]
    async fn host_hub_builds_host_and_guest_links() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("host-hub.shm");
        let classes = [SizeClassConfig {
            slot_size: 4096,
            slot_count: 8,
        }];
        let segment = Arc::new(
            Segment::create(
                &path,
                SegmentConfig {
                    max_guests: 1,
                    bipbuf_capacity: 64 * 1024,
                    max_payload_size: 4096,
                    inline_threshold: 256,
                    heartbeat_interval: 0,
                    size_classes: &classes,
                },
                FileCleanup::Manual,
            )
            .expect("create segment"),
        );

        let hub = HostHub::new(Arc::clone(&segment));
        let prepared = hub.prepare_peer().expect("prepare peer");
        let (host_peer, ticket) = prepared.into_parts();

        let host_link = host_peer.into_link().expect("build host link");

        #[cfg(unix)]
        let guest_link = {
            // SAFETY: fds come from freshly-created ticket and are consumed exactly once.
            unsafe { guest_link_from_ticket(Arc::clone(&segment), ticket, true) }
                .expect("build guest link")
        };
        #[cfg(windows)]
        let guest_link = {
            guest_link_from_ticket_windows(Arc::clone(&segment), ticket, true)
                .expect("build guest link")
        };

        // On Windows, accept the doorbell connection after guest has connected.
        #[cfg(windows)]
        host_link.accept_doorbell().await.expect("accept doorbell");

        let (host_tx, mut host_rx) = host_link.split();
        let (guest_tx, mut guest_rx) = guest_link.split();

        // host -> guest
        let permit = host_tx.reserve().await.expect("reserve host tx");
        let mut slot = permit.alloc(4).expect("alloc host slot");
        slot.as_mut_slice().copy_from_slice(b"ping");
        slot.commit();
        let msg = guest_rx
            .recv()
            .await
            .expect("recv guest")
            .expect("guest payload");
        assert_eq!(msg.as_bytes(), b"ping");

        // guest -> host
        let permit = guest_tx.reserve().await.expect("reserve guest tx");
        let mut slot = permit.alloc(4).expect("alloc guest slot");
        slot.as_mut_slice().copy_from_slice(b"pong");
        slot.commit();
        let msg = host_rx
            .recv()
            .await
            .expect("recv host")
            .expect("host payload");
        assert_eq!(msg.as_bytes(), b"pong");
    }

    #[tokio::test]
    async fn releasing_reservation_allows_reuse() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("host-hub-reuse.shm");
        let classes = [SizeClassConfig {
            slot_size: 4096,
            slot_count: 4,
        }];
        let segment = Arc::new(
            Segment::create(
                &path,
                SegmentConfig {
                    max_guests: 1,
                    bipbuf_capacity: 16 * 1024,
                    max_payload_size: 1024,
                    inline_threshold: 256,
                    heartbeat_interval: 0,
                    size_classes: &classes,
                },
                FileCleanup::Manual,
            )
            .expect("create segment"),
        );

        let hub = HostHub::new(Arc::clone(&segment));
        let prepared = hub.prepare_peer().expect("first prepare");
        let (host_peer, _ticket) = prepared.into_parts();
        host_peer.release_reservation();

        // slot should be available again
        let _prepared2 = hub.prepare_peer().expect("second prepare");
    }
}
