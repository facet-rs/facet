# Phase 004: Death Callbacks

## Goal

Implement death notification callbacks so the host can react to guest crashes
immediately. This enables cleanup, logging, and optional restart of crashed guests.

## Current State

Phase 003 adds `AddPeerOptions::on_death` but doesn't actually invoke it.
There's no monitoring loop to detect death and trigger callbacks.

## Target API

```rust
// Register death callback when adding peer
let (handle, ticket) = host.add_peer(AddPeerOptions {
    peer_name: Some("cell-image".into()),
    on_death: Some(Arc::new(|peer_id| {
        tracing::warn!("Peer {} died", peer_id);
        // Cleanup, restart, etc.
    })),
})?;

// Host runs a monitor that watches doorbells
// (typically in a background task)
host.run_death_monitor().await;  // async version

// Or integrate with your own event loop
loop {
    match host.poll_deaths(Duration::from_millis(100))? {
        Some(peer_id) => { /* handle death */ }
        None => { /* timeout, do other work */ }
    }
}
```

## Spec Rules

| Rule | Description |
|------|-------------|
| `shm.death.callback` | Death callback registration |
| `shm.death.callback-context` | Callback execution context |
| `shm.death.detection-methods` | Multiple detection methods |
| `shm.death.process-handle` | pidfd/process handle detection |
| `shm.death.recovery` | Recovery actions on death |
| `shm.crash.recovery` | Cleanup crashed guest resources |

## Implementation Plan

### 1. Track Peer State in Host

```rust
// host.rs

use std::collections::HashMap;
use std::sync::Arc;

pub type DeathCallback = Arc<dyn Fn(PeerId) + Send + Sync>;

/// State tracked for each active peer.
struct PeerTracking {
    /// Human-readable name
    name: Option<String>,
    /// Host's doorbell end
    doorbell: Doorbell,
    /// Death callback
    on_death: Option<DeathCallback>,
    /// Whether we've already notified death
    death_notified: bool,
}

pub struct ShmHost {
    // ... existing fields
    
    /// Tracked peers (by peer_id)
    peers: HashMap<PeerId, PeerTracking>,
}
```

### 2. Implement Death Polling

```rust
// host.rs

impl ShmHost {
    /// Poll all peer doorbells for death events.
    ///
    /// Returns the first dead peer found, or None if timeout expires.
    ///
    /// shm[impl shm.death.detection-methods]
    pub fn poll_deaths(&mut self, timeout: Duration) -> io::Result<Option<PeerId>> {
        // Build poll array
        let peers: Vec<_> = self.peers.iter()
            .filter(|(_, p)| !p.death_notified)
            .collect();
        
        if peers.is_empty() {
            std::thread::sleep(timeout);
            return Ok(None);
        }
        
        let mut pollfds: Vec<libc::pollfd> = peers.iter()
            .map(|(_, p)| libc::pollfd {
                fd: p.doorbell.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        
        let timeout_ms = timeout.as_millis() as i32;
        let ret = unsafe {
            libc::poll(pollfds.as_mut_ptr(), pollfds.len() as _, timeout_ms)
        };
        
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        
        // Check for deaths
        for (i, pfd) in pollfds.iter().enumerate() {
            if pfd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                let peer_id = *peers[i].0;
                self.handle_death(peer_id);
                return Ok(Some(peer_id));
            }
            
            // Also drain any signals (not death, just wakeup)
            if pfd.revents & libc::POLLIN != 0 {
                let _ = peers[i].1.doorbell.drain();
            }
        }
        
        Ok(None)
    }
    
    /// Handle a peer death.
    ///
    /// shm[impl shm.death.callback]
    /// shm[impl shm.death.callback-context]
    fn handle_death(&mut self, peer_id: PeerId) {
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            if peer.death_notified {
                return;
            }
            peer.death_notified = true;
            
            // Invoke callback
            if let Some(ref callback) = peer.on_death {
                callback(peer_id);
            }
        }
        
        // Perform recovery
        self.recover_peer(peer_id);
    }
    
    /// Clean up a crashed peer's resources.
    ///
    /// shm[impl shm.crash.recovery]
    /// shm[impl shm.death.recovery]
    fn recover_peer(&mut self, peer_id: PeerId) {
        let entry = self.peer_entry(peer_id);
        
        // 1. Set state to Goodbye
        entry.state.store(PeerState::Goodbye as u32, Ordering::Release);
        
        // 2. Reset rings to empty
        entry.guest_to_host_head.store(0, Ordering::Release);
        entry.guest_to_host_tail.store(0, Ordering::Release);
        entry.host_to_guest_head.store(0, Ordering::Release);
        entry.host_to_guest_tail.store(0, Ordering::Release);
        
        // 3. Return all slots to free
        self.reset_guest_slot_pool(peer_id);
        
        // 4. Reset channel table
        self.reset_channel_table(peer_id);
        
        // 5. Set state to Empty
        entry.state.store(PeerState::Empty as u32, Ordering::Release);
    }
    
    fn reset_guest_slot_pool(&self, peer_id: PeerId) {
        let pool_offset = self.layout.guest_slot_pool_offset(peer_id.get());
        // Reset bitmap to all-free
        // ...
    }
    
    fn reset_channel_table(&self, peer_id: PeerId) {
        let table_offset = self.layout.guest_channel_table_offset(peer_id.get());
        let num_channels = self.layout.config.max_channels;
        
        for i in 0..num_channels {
            let entry_offset = table_offset + (i as u64 * CHANNEL_ENTRY_SIZE as u64);
            let entry = unsafe {
                &*(self.region.offset(entry_offset as usize) as *const ChannelEntry)
            };
            entry.state.store(ChannelState::Free as u32, Ordering::Release);
        }
    }
}
```

### 3. Async Death Monitor

```rust
// host.rs

#[cfg(feature = "tokio")]
impl ShmHost {
    /// Run death monitor as an async task.
    ///
    /// This watches all peer doorbells and invokes callbacks on death.
    pub async fn run_death_monitor(mut self: Arc<Mutex<Self>>) {
        use tokio::time::{interval, Duration};
        
        let mut ticker = interval(Duration::from_millis(100));
        
        loop {
            ticker.tick().await;
            
            let mut host = self.lock().await;
            while let Some(peer_id) = host.poll_deaths(Duration::ZERO).ok().flatten() {
                // Death already handled in poll_deaths
                tracing::debug!("Peer {} death processed", peer_id);
            }
        }
    }
}
```

### 4. Alternative: epoll-based Monitor

```rust
// For more efficient monitoring with many peers

#[cfg(target_os = "linux")]
pub struct DeathMonitor {
    epoll_fd: RawFd,
    peers: HashMap<RawFd, PeerId>,
}

#[cfg(target_os = "linux")]
impl DeathMonitor {
    pub fn new() -> io::Result<Self> {
        let epoll_fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if epoll_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        
        Ok(Self {
            epoll_fd,
            peers: HashMap::new(),
        })
    }
    
    pub fn add_peer(&mut self, peer_id: PeerId, doorbell: &Doorbell) -> io::Result<()> {
        let fd = doorbell.as_raw_fd();
        
        let mut event = libc::epoll_event {
            events: (libc::EPOLLIN | libc::EPOLLHUP | libc::EPOLLERR) as u32,
            u64: fd as u64,
        };
        
        let ret = unsafe {
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut event)
        };
        
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        
        self.peers.insert(fd, peer_id);
        Ok(())
    }
    
    pub fn wait(&self, timeout: Duration) -> io::Result<Vec<(PeerId, bool)>> {
        let mut events = [libc::epoll_event { events: 0, u64: 0 }; 32];
        let timeout_ms = timeout.as_millis() as i32;
        
        let n = unsafe {
            libc::epoll_wait(
                self.epoll_fd,
                events.as_mut_ptr(),
                events.len() as i32,
                timeout_ms,
            )
        };
        
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        
        let mut results = Vec::new();
        for event in &events[..n as usize] {
            let fd = event.u64 as RawFd;
            if let Some(&peer_id) = self.peers.get(&fd) {
                let is_death = (event.events & (libc::EPOLLHUP | libc::EPOLLERR) as u32) != 0;
                results.push((peer_id, is_death));
            }
        }
        
        Ok(results)
    }
}
```

## Tasks

- [ ] Add `PeerTracking` struct with death state
- [ ] Implement `poll_deaths()` for synchronous polling
- [ ] Implement `handle_death()` with callback invocation
- [ ] Implement `recover_peer()` for resource cleanup
- [ ] Add async `run_death_monitor()` (tokio feature)
- [ ] Optional: Add epoll-based `DeathMonitor` for Linux
- [ ] Add tracey annotations
- [ ] Write tests for death detection and recovery

## Testing Strategy

```rust
#[test]
fn test_death_callback() {
    use std::sync::atomic::{AtomicBool, Ordering};
    
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.shm");
    
    let death_seen = Arc::new(AtomicBool::new(false));
    let death_seen_clone = death_seen.clone();
    
    let mut host = ShmHost::create(&path, SegmentConfig::default()).unwrap();
    let (handle, ticket) = host.add_peer(AddPeerOptions {
        peer_name: Some("test".into()),
        on_death: Some(Arc::new(move |_peer_id| {
            death_seen_clone.store(true, Ordering::SeqCst);
        })),
    }).unwrap();
    
    // Fork child that attaches and immediately exits
    match unsafe { libc::fork() } {
        0 => {
            // Child: attach and exit
            let args = SpawnArgs {
                hub_path: ticket.hub_path,
                peer_id: ticket.peer_id,
                doorbell_fd: ticket.doorbell_fd,
            };
            let _guest = ShmGuest::attach_with_ticket(&args).unwrap();
            std::process::exit(0);
        }
        pid => {
            // Parent: wait for death
            unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
            
            // Poll for death
            let dead = host.poll_deaths(Duration::from_secs(1)).unwrap();
            assert_eq!(dead, Some(handle.peer_id));
            assert!(death_seen.load(Ordering::SeqCst));
        }
    }
}

#[test]
fn test_peer_recovery() {
    // Test that peer slot becomes available after death recovery
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.shm");
    
    let config = SegmentConfig { max_guests: 1, ..Default::default() };
    let mut host = ShmHost::create(&path, config).unwrap();
    
    // Add peer (uses the only slot)
    let (handle, ticket) = host.add_peer(AddPeerOptions::default()).unwrap();
    
    // Simulate death
    host.handle_death(handle.peer_id);
    
    // Should be able to add another peer now
    let result = host.add_peer(AddPeerOptions::default());
    assert!(result.is_ok());
}
```

## Dependencies

- Phase 002 (doorbells) - for death detection via POLLHUP
- Phase 003 (spawn tickets) - for `AddPeerOptions` and peer tracking

## Notes

- Death callbacks are invoked synchronously in the polling thread
- Callbacks should be quick; schedule heavy work asynchronously
- Recovery resets all peer resources so the slot can be reused
- Consider adding a "restart" helper that re-spawns crashed guests
