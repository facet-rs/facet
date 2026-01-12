# SHM Transport for Dodeca Migration

## Goal

Complete roam-shm implementation to enable dodeca to migrate from rapace's SHM
transport to roam-shm. This enables the host/guest plugin architecture with
high-performance shared memory IPC.

## Current State

| Component | Status | Spec Coverage | Notes |
|-----------|--------|---------------|-------|
| Segment layout/header | ✅ Done | Covered | `SegmentHeader`, `SegmentConfig`, `SegmentLayout` |
| Peer table | ✅ Done | Covered | `PeerEntry`, `PeerState`, `PeerId` |
| Ring buffers | ✅ Done | Partial | Descriptors, head/tail, send/recv |
| Slot pools (fixed) | ✅ Done | Partial | Bitmap allocation via `SlotPool` |
| Channel metadata | ✅ Done | Partial | `ChannelEntry`, `ChannelState` |
| Transport trait | ✅ Done | Covered | `ShmHostGuestTransport`, `ShmGuestTransport` |
| File-backed mmap | ✅ Done | Covered | `MmapRegion`, `ShmHost::create(path, ...)`, `ShmGuest::attach_path` |
| Doorbells | ✅ Done | Covered | `Doorbell` socketpair wakeup + death detection |
| Spawn tickets | ✅ Done | Covered | `SpawnTicket`, `SpawnArgs`, Reserved peer state |
| Death callbacks | ✅ Done | Covered | doorbell-based death detection + recovery |
| Futex wakeups | ✅ Done | Covered | ring/credit/slot waiting + fallback |
| Variable-size slots | ✅ Done | Covered | shared size classes + ownership tracking |
| Slot extents | ✅ Done | Covered | extent-based growth + cross-process remap |
| **Roam SHM driver** | ❌ Missing | n/a | Needed for `ConnectionHandle`/`#[roam::service]` over SHM |
| **Tracing** | ❌ Missing | n/a | Replace `rapace_tracing` plumbing |
| **Tunnel streams** | ❌ Missing | n/a | Replace `rapace::TunnelStream` |

**Spec coverage:** see tracey (SHM / rust). The remaining work is mostly runtime integration (driver/tracing/tunnel).

## Target API (matching rapace)

Dodeca currently uses rapace's API:

```rust
use rapace::transport::shm::{AddPeerOptions, HubConfig, HubHost};

// Host creates the hub
let hub = HubHost::create(&hub_path, HubConfig::default())?;

// Add a peer (before spawning)
let (transport, ticket) = hub.add_peer_transport_with_options(AddPeerOptions {
    peer_name: Some("cell-image".into()),
    on_death: Some(Arc::new(|peer_id| { /* cleanup */ })),
})?;

// Spawn child process with ticket info
Command::new(&cell_path)
    .arg(format!("--hub-path={}", hub_path))
    .arg(format!("--peer-id={}", ticket.peer_id))
    .arg(format!("--doorbell-fd={}", ticket.doorbell_fd))
    .spawn()?;
```

Guest side:
```rust
use rapace::transport::shm::{Doorbell, HubPeer};

let peer = HubPeer::attach(&hub_path, peer_id)?;
let doorbell = Doorbell::from_raw_fd(doorbell_fd);
let transport = peer.into_transport(doorbell);
```

## Phases

| Phase | File | Status | Description |
|-------|------|--------|-------------|
| 001 | [001-mmap-backed-regions.md](./001-mmap-backed-regions.md) | ✅ Done | File-backed mmap for cross-process SHM |
| 002 | [002-doorbells.md](./002-doorbells.md) | ✅ Done | Socketpair-based wakeup and death detection |
| 003 | [003-spawn-tickets.md](./003-spawn-tickets.md) | ✅ Done | `AddPeerOptions` and spawn ticket API |
| 004 | [004-death-callbacks.md](./004-death-callbacks.md) | ✅ Done | Crash detection and cleanup callbacks |
| 005 | [005-futex-wakeups.md](./005-futex-wakeups.md) | ✅ Done | Efficient blocking on ring/credit/slots |
| 006 | [006-variable-slots.md](./006-variable-slots.md) | ✅ Done | Variable-size slot pools |
| 007 | [007-slot-extents.md](./007-slot-extents.md) | ✅ Done | Extent-based growth + remap |
| 008 | [008-roam-shm-driver.md](./008-roam-shm-driver.md) | TODO | Roam driver over SHM (no Hello/Credit frames) |
| 009 | [009-roam-tracing.md](./009-roam-tracing.md) | TODO | Tracing across cells (roam-native) |
| 010 | [010-tunnel-streams.md](./010-tunnel-streams.md) | TODO | Tunnel streams (just `Tx/Rx<Vec<u8>>`) |
| 011 | [011-dodeca-flag-day-cutover.md](./011-dodeca-flag-day-cutover.md) | TODO | Dodeca flag-day cutover plan |
| 012 | [012-flow-control-backend.md](./012-flow-control-backend.md) | TODO | Real credit flow control (move off infinite credit) |

## Phase Dependencies

```
001 Mmap Regions (foundation)
      │
      ├──────────────────┐
      ▼                  ▼
002 Doorbells      005 Futex Wakeups
      │
      ▼
003 Spawn Tickets
      │
      ▼
004 Death Callbacks
      │
      ▼
006 Variable Slots
      │
      ▼
007 Slot Extents (optional)
      │
      ▼
008 Roam SHM Driver (required for dodeca)
      │             ┌────────────────┬────────────────┐
      │             ▼                ▼                ▼
      │         009 Tracing      010 Tunnel Streams  012 Flow Control (optional)
      │             └───────┬────────┘
      ▼                     ▼
011 Dodeca Cutover (flag day)
```

**Critical path**: 001 → 002 → 003 → 004 → 006 → 008 (+ 010 if tunneling is required)

**Parallel work**: 005 can be done alongside 002-004, 009/010 can proceed once 008 exists, and 012 is optional hardening once 008 exists.

## Spec Rules by Phase

### Phase 001: Mmap Regions
- `shm.file.path` - Segment file location
- `shm.file.create` - Creating segment file
- `shm.file.attach` - Attaching to segment
- `shm.file.permissions` - File permissions
- `shm.file.cleanup` - Cleanup on shutdown
- `shm.file.mmap-posix` - POSIX mmap
- `shm.file.mmap-windows` - Windows mapping (optional)

### Phase 002: Doorbells
- `shm.doorbell.purpose` - Why doorbells exist
- `shm.doorbell.socketpair` - Implementation via socketpair
- `shm.doorbell.signal` - Signaling the peer
- `shm.doorbell.wait` - Waiting for signals
- `shm.doorbell.death` - Death detection via POLLHUP
- `shm.doorbell.ring-integration` - Integration with rings
- `shm.doorbell.optional` - Doorbells are optional

### Phase 003: Spawn Tickets
- `shm.spawn.ticket` - Spawn ticket contents
- `shm.spawn.reserved-state` - Reserved peer state
- `shm.spawn.args` - Command-line arguments
- `shm.spawn.fd-inheritance` - FD inheritance for doorbells
- `shm.spawn.guest-init` - Guest initialization

### Phase 004: Death Callbacks
- `shm.death.callback` - Death callback registration
- `shm.death.callback-context` - Callback execution context
- `shm.death.detection-methods` - Detection methods table
- `shm.death.process-handle` - Process handle detection
- `shm.death.recovery` - Recovery actions
- `shm.crash.recovery` - Crash cleanup

### Phase 005: Futex Wakeups
- `shm.wakeup.consumer-wait` - Consumer waiting for messages
- `shm.wakeup.producer-wait` - Producer waiting for space
- `shm.wakeup.credit-wait` - Sender waiting for credit
- `shm.wakeup.slot-wait` - Sender waiting for slots
- `shm.wakeup.fallback` - Non-Linux platforms

### Phase 006: Variable Slots
- `shm.varslot.classes` - Size classes
- `shm.varslot.selection` - Slot selection algorithm
- `shm.varslot.shared` - Shared pool architecture
- `shm.varslot.ownership` - Slot ownership tracking
- `shm.varslot.extents` - Extent-based growth
- `shm.varslot.extent-layout` - Extent layout
- `shm.varslot.freelist` - Free list management
- `shm.varslot.allocation` - Allocation algorithm
- `shm.varslot.freeing` - Freeing algorithm

### Phase 007: Slot Extents
- `shm.varslot.extents` - Extent-based growth
- `shm.varslot.extent-layout` - Extent layout

## Success Criteria

1. ✅ `ShmHost::create(path, config)` creates a file-backed segment
2. ✅ `ShmGuest::attach_path(path)` attaches to existing segment (non-spawned)
3. ✅ `ShmGuest::attach_with_ticket(&SpawnArgs)` works for spawned guests
4. ✅ Doorbells provide instant wakeup and death detection
5. ✅ `AddPeerOptions` supports death callbacks
6. ✅ Spawn tickets work with `Command::spawn()`
7. ✅ All spec tests pass
8. ✅ Roam driver can run over SHM (`ConnectionHandle` + driver task)
9. ✅ Tracing works across cells (or is explicitly deferred)
10. ✅ Tunneling works for cell-http (or is explicitly deferred)
11. ✅ Dodeca can be migrated from rapace to roam + roam-shm

## Files

### roam-shm
- `rust/roam-shm/src/lib.rs` - Crate root, re-exports
- `rust/roam-shm/src/layout.rs` - Segment layout calculations
- `rust/roam-shm/src/host.rs` - Host implementation
- `rust/roam-shm/src/guest.rs` - Guest implementation
- `rust/roam-shm/src/peer.rs` - Peer table entry
- `rust/roam-shm/src/channel.rs` - Channel metadata
- `rust/roam-shm/src/slot_pool.rs` - Slot pool allocation
- `rust/roam-shm/src/transport.rs` - Transport trait impl
- `rust/roam-shm/src/msg.rs` - Message type constants

### shm-primitives
- `rust/shm-primitives/src/lib.rs` - Low-level memory regions

### Spec
- `docs/content/shm-spec/_index.md` - SHM specification

## Commands

```bash
# Run roam-shm tests
cargo nextest run -p roam-shm

# Check tracey coverage
tracey web  # then look at shm/rust

# Run stress tests
cargo nextest run -p roam-shm --test stress

# Build dodeca (after migration)
cd ~/bearcove/dodeca && cargo build
```

## Estimated Effort

| Phase | Complexity | Est. Time |
|-------|------------|-----------|
| 008 Roam SHM Driver | High | 1-3 days |
| 009 Tracing | Medium | 0.5-2 days |
| 010 Tunnel Streams | Medium-High | 1-3 days |
| 011 Dodeca Cutover | High | 1-3 days |

## Notes

- Windows support (`shm.file.mmap-windows`) is optional for initial migration
- The existing `HeapRegion` code is useful for unit tests even after mmap is added
