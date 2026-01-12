# Phase 003: Spawn Tickets

## Goal

Implement the spawn ticket API that allows the host to reserve a peer slot,
create a doorbell, and pass the necessary information to a spawned child process.

## Current State

No spawn ticket API exists. Guests currently find their own peer slot via CAS,
which doesn't work for pre-assigned slots with doorbells.

## Target API

```rust
// Host side: reserve a peer slot and get spawn ticket
let (transport, ticket) = host.add_peer(AddPeerOptions {
    peer_name: Some("cell-image".into()),
    on_death: Some(Arc::new(|peer_id| { /* cleanup */ })),
})?;

// ticket contains:
// - peer_id: PeerId
// - guest doorbell (kept alive by the ticket until spawn)
//   - fd is inheritable (CLOEXEC cleared)

// Spawn child with ticket info
Command::new(&cell_path)
    .args(ticket.to_args())  // --hub-path=... --peer-id=... --doorbell-fd=...
    .spawn()?;

// Guest side: parse args and attach
let args = SpawnArgs::from_env()?;  // or from_args()
let guest = ShmGuest::attach_with_ticket(&args)?;
let doorbell = unsafe { Doorbell::from_raw_fd(args.doorbell_fd) };
```

## Spec Rules

| Rule | Description |
|------|-------------|
| `shm.spawn.ticket` | Spawn ticket contents (hub_path, peer_id, doorbell_fd) |
| `shm.spawn.reserved-state` | Reserved state for pre-allocated peer slots |
| `shm.spawn.args` | Command-line arguments format |
| `shm.spawn.fd-inheritance` | FD inheritance (clear CLOEXEC) |
| `shm.spawn.guest-init` | Guest initialization from ticket |

## Implementation Plan

### 1. Add Reserved State

```rust
// peer.rs

/// shm[impl shm.spawn.reserved-state]
#[repr(u32)]
pub enum PeerState {
    Empty = 0,
    Attached = 1,
    Goodbye = 2,
    Reserved = 3,  // Host has allocated, guest not yet attached
}

impl PeerEntry {
    /// Reserve this slot for a spawned guest.
    /// Returns Ok(epoch) if successful, Err if slot not empty.
    pub fn try_reserve(&self) -> Result<u32, ()> {
        let result = self.state.compare_exchange(
            PeerState::Empty as u32,
            PeerState::Reserved as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        
        match result {
            Ok(_) => {
                let epoch = self.epoch.fetch_add(1, Ordering::AcqRel);
                Ok(epoch + 1)
            }
            Err(_) => Err(()),
        }
    }
    
    /// Transition from Reserved to Attached (guest side).
    pub fn try_claim_reserved(&self) -> Result<(), ()> {
        self.state.compare_exchange(
            PeerState::Reserved as u32,
            PeerState::Attached as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ).map(|_| ()).map_err(|_| ())
    }
    
    /// Release a reserved slot back to empty (if spawn fails).
    pub fn release_reserved(&self) {
        self.state.store(PeerState::Empty as u32, Ordering::Release);
    }
}
```

### 2. Add Spawn Ticket Types

```rust
// roam-shm/src/spawn.rs

use std::os::unix::io::{AsRawFd, RawFd};
use std::path::PathBuf;
use crate::peer::PeerId;
use shm_primitives::Doorbell;

/// Information needed by a spawned guest to attach.
///
/// shm[impl shm.spawn.ticket]
#[derive(Debug)]
pub struct SpawnTicket {
    /// Path to the SHM segment file
    pub hub_path: PathBuf,
    /// Assigned peer ID
    pub peer_id: PeerId,
    /// Guest's doorbell end (fd is inheritable; ticket drop closes our copy)
    pub guest_doorbell: Doorbell,
}

impl SpawnTicket {
    /// The raw file descriptor to pass to the child process.
    pub fn doorbell_fd(&self) -> RawFd {
        self.guest_doorbell.as_raw_fd()
    }

    /// Convert to command-line arguments.
    ///
    /// shm[impl shm.spawn.args]
    pub fn to_args(&self) -> Vec<String> {
        vec![
            format!("--hub-path={}", self.hub_path.display()),
            format!("--peer-id={}", self.peer_id.get()),
            format!("--doorbell-fd={}", self.doorbell_fd()),
        ]
    }
}

/// Parsed spawn arguments for guest initialization.
///
/// shm[impl shm.spawn.guest-init]
#[derive(Debug, Clone)]
pub struct SpawnArgs {
    pub hub_path: PathBuf,
    pub peer_id: PeerId,
    pub doorbell_fd: RawFd,
}

impl SpawnArgs {
    /// Parse from command-line arguments.
    pub fn from_args<I, S>(args: I) -> Result<Self, SpawnArgsError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut hub_path = None;
        let mut peer_id = None;
        let mut doorbell_fd = None;
        
        for arg in args {
            let arg = arg.as_ref();
            if let Some(value) = arg.strip_prefix("--hub-path=") {
                hub_path = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--peer-id=") {
                let id: u8 = value.parse().map_err(|_| SpawnArgsError::InvalidPeerId)?;
                peer_id = Some(PeerId::new(id).ok_or(SpawnArgsError::InvalidPeerId)?);
            } else if let Some(value) = arg.strip_prefix("--doorbell-fd=") {
                doorbell_fd = Some(value.parse().map_err(|_| SpawnArgsError::InvalidFd)?);
            }
        }
        
        Ok(Self {
            hub_path: hub_path.ok_or(SpawnArgsError::MissingHubPath)?,
            peer_id: peer_id.ok_or(SpawnArgsError::MissingPeerId)?,
            doorbell_fd: doorbell_fd.ok_or(SpawnArgsError::MissingDoorbellFd)?,
        })
    }
    
    /// Parse from std::env::args().
    pub fn from_env() -> Result<Self, SpawnArgsError> {
        Self::from_args(std::env::args())
    }
}

#[derive(Debug)]
pub enum SpawnArgsError {
    MissingHubPath,
    MissingPeerId,
    MissingDoorbellFd,
    InvalidPeerId,
    InvalidFd,
}
```

### 3. Add AddPeerOptions and Host API

```rust
// roam-shm/src/host.rs

use std::sync::Arc;

/// Callback invoked when a peer dies.
pub type DeathCallback = Arc<dyn Fn(PeerId) + Send + Sync>;

/// Options for adding a peer.
#[derive(Default)]
pub struct AddPeerOptions {
    /// Human-readable name for debugging
    pub peer_name: Option<String>,
    /// Callback when peer dies (doorbell death or heartbeat timeout)
    pub on_death: Option<DeathCallback>,
}

/// Host-side handle for a single peer.
pub struct PeerHandle {
    pub peer_id: PeerId,
    /// Host's doorbell end
    doorbell: Doorbell,
    /// Death callback
    on_death: Option<DeathCallback>,
}

impl ShmHost {
    /// Add a new peer, returning the spawn ticket.
    ///
    /// This reserves a peer slot and creates a doorbell pair.
    /// The returned ticket should be passed to the spawned process.
    ///
    /// shm[impl shm.spawn.ticket]
pub fn add_peer(
        &mut self,
        options: AddPeerOptions,
    ) -> io::Result<(PeerHandle, SpawnTicket)> {
        // Find and reserve an empty slot
        let peer_id = self.reserve_peer_slot()?;
        
        // Create doorbell pair
        let (host_bell, guest_bell) = Doorbell::pair()?;
        
        // Clear CLOEXEC on guest's doorbell
        // shm[impl shm.spawn.fd-inheritance]
        guest_bell.clear_cloexec()?;
        
        let ticket = SpawnTicket {
            hub_path: self.path.clone().ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "no path for heap-backed segment")
            })?,
            peer_id,
            guest_doorbell: guest_bell,
        };
        
        let handle = PeerHandle {
            peer_id,
            doorbell: host_bell,
            on_death: options.on_death,
        };
        
        self.peer_handles.insert(peer_id, handle.doorbell);
        
        Ok((handle, ticket))
    }
    
    /// Reserve a peer slot, returning its ID.
    fn reserve_peer_slot(&self) -> io::Result<PeerId> {
        for i in 1..=self.layout.config.max_guests as u8 {
            let peer_id = PeerId::from_index(i - 1).unwrap();
            let entry = self.peer_entry(peer_id);
            
            if entry.try_reserve().is_ok() {
                return Ok(peer_id);
            }
        }
        
        Err(io::Error::new(io::ErrorKind::Other, "no available peer slots"))
    }
    
    /// Release a reserved peer slot (if spawn fails).
    pub fn release_peer(&mut self, peer_id: PeerId) {
        let entry = self.peer_entry(peer_id);
        entry.release_reserved();
        self.peer_handles.remove(&peer_id);
    }
}
```

### 4. Update Guest Attach

```rust
// guest.rs

impl ShmGuest {
    /// Attach using spawn ticket information.
    ///
    /// shm[impl shm.spawn.guest-init]
    pub fn attach_with_ticket(args: &SpawnArgs) -> Result<Self, AttachError> {
        let backing = MmapRegion::attach(&args.hub_path)
            .map_err(AttachError::Io)?;
        let region = backing.region();
        
        // Validate header (same as attach)
        let header = unsafe { &*(region.as_ptr() as *const SegmentHeader) };
        if header.magic != MAGIC {
            return Err(AttachError::InvalidMagic);
        }
        // ... other validation
        
        // Claim our reserved slot
        let layout = /* reconstruct from header */;
        let entry_offset = layout.peer_entry_offset(args.peer_id.get());
        let entry = unsafe { &*(region.offset(entry_offset as usize) as *const PeerEntry) };
        
        entry.try_claim_reserved()
            .map_err(|_| AttachError::SlotNotReserved)?;
        
        let slots = SlotPool::new(
            region,
            layout.guest_slot_pool_offset(args.peer_id.get()),
            &/* config */,
        );
        
        Ok(Self {
            backing: Some(ShmBacking::Mmap(backing)),
            region,
            peer_id: args.peer_id,
            layout,
            slots,
            g2h_local_head: 0,
            h2g_local_tail: 0,
            fatal_error: false,
        })
    }
}

/// New error variant
pub enum AttachError {
    // ... existing
    SlotNotReserved,
}
```

## Tasks

- [ ] Add `PeerState::Reserved` variant
- [ ] Implement `try_reserve()` and `try_claim_reserved()` on `PeerEntry`
- [ ] Add `SpawnTicket` and `SpawnArgs` types
- [ ] Add `AddPeerOptions` and `DeathCallback` type alias
- [ ] Implement `ShmHost::add_peer()`
- [ ] Implement `ShmGuest::attach_with_ticket()`
- [ ] Add tracey annotations
- [ ] Write integration tests

## Testing Strategy

```rust
#[test]
fn test_spawn_ticket_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.shm");
    
    // Host creates segment and adds peer
    let mut host = ShmHost::create(&path, SegmentConfig::default()).unwrap();
    let (handle, ticket) = host.add_peer(AddPeerOptions::default()).unwrap();
    
    // Simulate parsing args in child
    let args_vec = ticket.to_args();
    let args = SpawnArgs::from_args(&args_vec).unwrap();
    
    assert_eq!(args.hub_path, path);
    assert_eq!(args.peer_id, handle.peer_id);
}

#[test]
fn test_spawn_and_attach() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.shm");
    
    let mut host = ShmHost::create(&path, SegmentConfig::default()).unwrap();
    let (handle, ticket) = host.add_peer(AddPeerOptions::default()).unwrap();
    
    // Fork and have child attach
    match unsafe { libc::fork() } {
        0 => {
            // Child
            let SpawnTicket { hub_path, peer_id, guest_doorbell } = ticket;
            let doorbell_fd = guest_doorbell.into_raw_fd();
            let args = SpawnArgs { hub_path, peer_id, doorbell_fd };
            let guest = ShmGuest::attach_with_ticket(&args).unwrap();
            let doorbell = unsafe { Doorbell::from_raw_fd(args.doorbell_fd) };
            
            // Signal readiness
            doorbell.ring().unwrap();
            std::process::exit(0);
        }
        pid => {
            // Parent: wait for child to signal
            match handle.doorbell.wait_timeout(Some(Duration::from_secs(1))).unwrap() {
                DoorbellEvent::Signal => {}
                other => panic!("expected Signal, got {:?}", other),
            }
            unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
        }
    }
}
```

## Dependencies

- Phase 001 (mmap regions) - for file-backed segments
- Phase 002 (doorbells) - for doorbell pairs

## Notes

- The guest doorbell is kept alive by the returned `SpawnTicket` until `Command::spawn()`
- After successful spawn, drop the `SpawnTicket` to close the parent's copy (child inherited it)
- If spawn fails, call `host.release_peer(peer_id)` to free the reserved slot
