# Phase 002: Doorbells

## Goal

Implement socketpair-based doorbells for instant cross-process wakeup and
death detection. Doorbells complement futex-based ring synchronization by
providing:

1. **Instant wakeup**: Signal peer to check for work without busy-waiting
2. **Death detection**: Detect peer crash via POLLHUP/POLLERR
3. **Async integration**: Works with epoll/kqueue/tokio

## Current State

No doorbell implementation exists. The current code has no mechanism for:
- Waking a sleeping peer
- Detecting peer death instantly
- Integrating with async runtimes

## Target API

```rust
// Create a doorbell pair (host side)
let (host_bell, guest_bell) = Doorbell::pair()?;

// Signal the peer
host_bell.ring()?;

// Wait for signal (blocking)
guest_bell.wait()?;

// Wait with timeout
guest_bell.wait_timeout(Duration::from_millis(100))?;

// Poll for signal or death (non-blocking)
match guest_bell.poll()? {
    DoorbellEvent::Signal => { /* peer rang */ }
    DoorbellEvent::Death => { /* peer died */ }
    DoorbellEvent::Timeout => { /* nothing happened */ }
}

// Get raw fd for async integration
let fd = guest_bell.as_raw_fd();
```

## Spec Rules

| Rule | Description |
|------|-------------|
| `shm.doorbell.purpose` | Wakeup and death detection |
| `shm.doorbell.socketpair` | Implementation via Unix socketpair |
| `shm.doorbell.signal` | Write byte to signal peer |
| `shm.doorbell.wait` | Poll with timeout, drain on wakeup |
| `shm.doorbell.death` | POLLHUP/POLLERR indicates peer death |
| `shm.doorbell.ring-integration` | Signal after enqueueing to ring |
| `shm.doorbell.optional` | Doorbells are optional (fallback to polling) |

## Implementation Plan

### 1. Add Doorbell to shm-primitives

```rust
// shm-primitives/src/doorbell.rs

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::time::Duration;

/// One end of a doorbell pair for cross-process notification.
///
/// shm[impl shm.doorbell.purpose]
pub struct Doorbell {
    fd: OwnedFd,
}

/// Result of polling a doorbell.
pub enum DoorbellEvent {
    /// Peer signaled (rang the doorbell)
    Signal,
    /// Peer died (socket closed)
    Death,
    /// Timeout expired with no event
    Timeout,
}

impl Doorbell {
    /// Create a connected doorbell pair.
    ///
    /// Returns (host_end, guest_end). The guest_end should be passed to
    /// the spawned process.
    ///
    /// shm[impl shm.doorbell.socketpair]
    pub fn pair() -> io::Result<(Doorbell, Doorbell)> {
        let mut fds = [0i32; 2];
        
        let ret = unsafe {
            libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
                0,
                fds.as_mut_ptr(),
            )
        };
        
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        
        Ok((
            Doorbell { fd: unsafe { OwnedFd::from_raw_fd(fds[0]) } },
            Doorbell { fd: unsafe { OwnedFd::from_raw_fd(fds[1]) } },
        ))
    }
    
    /// Create a doorbell from a raw file descriptor.
    ///
    /// Used by spawned processes to reconstruct the doorbell from an
    /// inherited fd.
    ///
    /// # Safety
    ///
    /// The fd must be a valid, open socket from a doorbell pair.
    pub unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Doorbell { fd: OwnedFd::from_raw_fd(fd) }
    }
    
    /// Signal the peer by ringing the doorbell.
    ///
    /// shm[impl shm.doorbell.signal]
    pub fn ring(&self) -> io::Result<()> {
        let byte: u8 = 1;
        let ret = unsafe {
            libc::send(
                self.fd.as_raw_fd(),
                &byte as *const u8 as *const _,
                1,
                libc::MSG_DONTWAIT,
            )
        };

        if ret >= 0 {
            return Ok(());
        }

        // Doorbell is level-triggered: if the socket buffer is full, the peer is
        // already "woken"; treat EAGAIN/EWOULDBLOCK as success.
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) | Some(libc::EWOULDBLOCK) => Ok(()),
            _ => Err(err),
        }
    }
    
    /// Wait for the peer to ring, blocking indefinitely.
    pub fn wait(&self) -> io::Result<DoorbellEvent> {
        self.wait_timeout(None)
    }
    
    /// Wait for the peer to ring with optional timeout.
    ///
    /// shm[impl shm.doorbell.wait]
    pub fn wait_timeout(&self, timeout: Option<Duration>) -> io::Result<DoorbellEvent> {
        let timeout_ms = timeout
            .map(|d| d.as_millis() as i32)
            .unwrap_or(-1);
        
        let mut pfd = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        
        let ret = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        
        if ret == 0 {
            return Ok(DoorbellEvent::Timeout);
        }
        
        // shm[impl shm.doorbell.death]
        if pfd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            return Ok(DoorbellEvent::Death);
        }
        
        if pfd.revents & libc::POLLIN != 0 {
            // Drain the socket
            self.drain()?;
            return Ok(DoorbellEvent::Signal);
        }
        
        Ok(DoorbellEvent::Timeout)
    }
    
    /// Non-blocking poll for events.
    pub fn poll(&self) -> io::Result<DoorbellEvent> {
        self.wait_timeout(Some(Duration::ZERO))
    }
    
    /// Drain any pending data from the socket.
    pub fn drain(&self) -> io::Result<()> {
        let mut buf = [0u8; 64];
        loop {
            let ret = unsafe {
                libc::recv(
                    self.fd.as_raw_fd(),
                    buf.as_mut_ptr() as *mut _,
                    buf.len(),
                    libc::MSG_DONTWAIT,
                )
            };
            
            if ret <= 0 {
                break;
            }
        }
        Ok(())
    }
    
    /// Clear the CLOEXEC flag so this fd is inherited by children.
    ///
    /// Call this on the guest's doorbell before spawning.
    pub fn clear_cloexec(&self) -> io::Result<()> {
        let flags = unsafe { libc::fcntl(self.fd.as_raw_fd(), libc::F_GETFD) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        
        let ret = unsafe {
            libc::fcntl(
                self.fd.as_raw_fd(),
                libc::F_SETFD,
                flags & !libc::FD_CLOEXEC,
            )
        };
        
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl AsRawFd for Doorbell {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
```

### 2. Async Doorbell Wrapper

```rust
// shm-primitives/src/doorbell.rs (continued)

#[cfg(feature = "tokio")]
pub mod tokio {
    use super::*;
    use ::tokio::io::unix::AsyncFd;
    use ::tokio::io::Interest;
    
    /// Async doorbell for use with tokio.
    pub struct AsyncDoorbell {
        inner: AsyncFd<Doorbell>,
    }
    
    impl AsyncDoorbell {
        pub fn new(doorbell: Doorbell) -> io::Result<Self> {
            Ok(Self {
                inner: AsyncFd::new(doorbell)?,
            })
        }
        
        /// Wait for the peer to ring, asynchronously.
        pub async fn wait(&self) -> io::Result<DoorbellEvent> {
            loop {
                let mut guard = self.inner.ready(Interest::READABLE).await?;
                
                match guard.try_io(|inner| inner.get_ref().poll()) {
                    Ok(Ok(event)) => return Ok(event),
                    Ok(Err(e)) => return Err(e),
                    Err(_would_block) => continue,
                }
            }
        }
        
        /// Ring the doorbell (sync, doesn't block).
        pub fn ring(&self) -> io::Result<()> {
            self.inner.get_ref().ring()
        }
    }
}
```

### 3. Integrate with Host/Guest

```rust
// host.rs

impl ShmHost {
    /// Send a frame and ring the guest's doorbell.
    ///
    /// shm[impl shm.doorbell.ring-integration]
    pub fn send_and_ring(
        &mut self,
        peer_id: PeerId,
        frame: Frame,
        doorbell: &Doorbell,
    ) -> Result<(), SendError> {
        self.send(peer_id, frame)?;
        doorbell.ring().map_err(SendError::Io)?;
        Ok(())
    }

    /// Convert this doorbell into a raw fd without closing it.
    ///
    /// This is useful in fork-based tests; real `Command::spawn()` + `exec()`
    /// won't have a `Doorbell` value to drop.
    pub fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

// guest.rs

impl ShmGuest {
    /// Send a frame and ring the host's doorbell.
    pub fn send_and_ring(
        &mut self,
        frame: Frame,
        doorbell: &Doorbell,
    ) -> Result<(), SendError> {
        self.send(frame)?;
        doorbell.ring().map_err(SendError::Io)?;
        Ok(())
    }
}
```

## Tasks

- [ ] Add `Doorbell` struct to `shm-primitives`
- [ ] Implement `pair()`, `ring()`, `wait_timeout()`
- [ ] Implement death detection via `POLLHUP`
- [ ] Add `clear_cloexec()` for fd inheritance
- [ ] Add async wrapper (tokio feature)
- [ ] Add tracey annotations
- [ ] Write unit tests
- [ ] Test death detection (kill child, observe POLLHUP)

## Testing Strategy

```rust
#[test]
fn test_doorbell_signal() {
    let (a, b) = Doorbell::pair().unwrap();
    
    // Signal from a to b
    a.ring().unwrap();
    
    // b should see it
    match b.poll().unwrap() {
        DoorbellEvent::Signal => {}
        other => panic!("expected Signal, got {:?}", other),
    }
}

#[test]
fn test_doorbell_death() {
    let (a, b) = Doorbell::pair().unwrap();
    
    // Drop a (close the socket)
    drop(a);
    
    // b should see death
    match b.wait_timeout(Some(Duration::from_millis(100))).unwrap() {
        DoorbellEvent::Death => {}
        other => panic!("expected Death, got {:?}", other),
    }
}

#[test]
fn test_doorbell_cross_process() {
    let (host_bell, guest_bell) = Doorbell::pair().unwrap();
    guest_bell.clear_cloexec().unwrap();
    
    let fd = guest_bell.as_raw_fd();
    
    // Fork and test
    match unsafe { libc::fork() } {
        0 => {
            // Child: reconstruct doorbell, ring it, exit
            let bell = unsafe { Doorbell::from_raw_fd(fd) };
            bell.ring().unwrap();
            std::process::exit(0);
        }
        pid => {
            // Parent: wait for ring
            drop(guest_bell); // Close our copy
            match host_bell.wait_timeout(Some(Duration::from_secs(1))).unwrap() {
                DoorbellEvent::Signal => {}
                other => panic!("expected Signal, got {:?}", other),
            }
            unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
        }
    }
}
```

## Dependencies

- `libc` for socketpair, poll, fcntl
- `tokio` (optional) for async wrapper

## Notes

- Doorbells are created by the host and one end is passed to the guest
- The guest's doorbell fd must have CLOEXEC cleared before spawn
- After spawn, the host closes its copy of the guest's fd
- Death detection is immediate when the peer process exits
