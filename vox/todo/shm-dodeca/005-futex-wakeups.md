# Phase 005: Futex Wakeups

## Goal

Implement efficient blocking using futex (Linux) or equivalent mechanisms for
waiting on ring buffers, credit availability, and slot availability. This
eliminates busy-waiting and reduces CPU usage.

## Current State

No blocking primitives exist. Code would need to busy-wait or sleep:

```rust
// Bad: busy-wait
while ring_is_full() {
    std::hint::spin_loop();
}

// Bad: sleep (adds latency)
while ring_is_full() {
    std::thread::sleep(Duration::from_micros(100));
}
```

## Target API

```rust
// Wait for ring space (producer)
ring.wait_for_space()?;  // blocks until ring has room

// Wait for ring data (consumer)  
ring.wait_for_data()?;  // blocks until ring has data

// Wait for credit
channel.wait_for_credit(needed_bytes)?;

// Wait for slot
slot_pool.wait_for_slot()?;

// Wake waiters
ring.wake_consumers();  // after producing
ring.wake_producers();  // after consuming
channel.wake_senders(); // after granting credit
slot_pool.wake_waiters(); // after freeing slot
```

## Spec Rules

| Rule | Description |
|------|-------------|
| `shm.wakeup.consumer-wait` | Consumer waits on ring head |
| `shm.wakeup.producer-wait` | Producer waits on ring tail |
| `shm.wakeup.credit-wait` | Sender waits on granted_total |
| `shm.wakeup.slot-wait` | Sender waits on bitmap word |
| `shm.wakeup.fallback` | Non-Linux fallback (polling) |

## Implementation Plan

### 1. Add Futex Wrapper

```rust
// shm-primitives/src/futex.rs

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// Result of a futex wait operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitResult {
    /// Woken by futex_wake
    Woken,
    /// Value changed before we could sleep
    Changed,
    /// Timeout expired
    Timeout,
}

/// Futex operations for Linux.
#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;
    
    /// Wait until the atomic value is no longer equal to `expected`.
    ///
    /// shm[impl shm.wakeup.consumer-wait]
    /// shm[impl shm.wakeup.producer-wait]
    pub fn wait(
        atomic: &AtomicU32,
        expected: u32,
        timeout: Option<Duration>,
    ) -> WaitResult {
        let timespec = timeout.map(|d| libc::timespec {
            tv_sec: d.as_secs() as i64,
            tv_nsec: d.subsec_nanos() as i64,
        });
        
        let timeout_ptr = timespec
            .as_ref()
            .map(|t| t as *const _)
            .unwrap_or(std::ptr::null());
        
        let ret = unsafe {
            libc::syscall(
                libc::SYS_futex,
                atomic as *const AtomicU32 as *const u32,
                libc::FUTEX_WAIT,
                expected,
                timeout_ptr,
                std::ptr::null::<u32>(),
                0u32,
            )
        };
        
        if ret == 0 {
            return WaitResult::Woken;
        }
        
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EAGAIN) => WaitResult::Changed,
            Some(libc::ETIMEDOUT) => WaitResult::Timeout,
            Some(libc::EINTR) => WaitResult::Woken, // treat interrupt as wakeup
            _ => WaitResult::Woken, // unexpected, but don't panic
        }
    }
    
    /// Wake up to `count` waiters on this atomic.
    pub fn wake(atomic: &AtomicU32, count: u32) -> u32 {
        let ret = unsafe {
            libc::syscall(
                libc::SYS_futex,
                atomic as *const AtomicU32 as *const u32,
                libc::FUTEX_WAKE,
                count,
                std::ptr::null::<libc::timespec>(),
                std::ptr::null::<u32>(),
                0u32,
            )
        };
        
        if ret < 0 { 0 } else { ret as u32 }
    }
    
    /// Wake one waiter.
    pub fn wake_one(atomic: &AtomicU32) -> bool {
        wake(atomic, 1) > 0
    }
    
    /// Wake all waiters.
    pub fn wake_all(atomic: &AtomicU32) -> u32 {
        wake(atomic, u32::MAX)
    }
}

/// Fallback for non-Linux platforms (polling with backoff).
///
/// shm[impl shm.wakeup.fallback]
#[cfg(not(target_os = "linux"))]
pub mod fallback {
    use super::*;
    
    pub fn wait(
        atomic: &AtomicU32,
        expected: u32,
        timeout: Option<Duration>,
    ) -> WaitResult {
        use std::time::Instant;
        
        let deadline = timeout.map(|t| Instant::now() + t);
        let mut backoff = 1u64;
        const MAX_BACKOFF: u64 = 1000; // 1ms max
        
        loop {
            let current = atomic.load(Ordering::Acquire);
            if current != expected {
                return WaitResult::Changed;
            }
            
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return WaitResult::Timeout;
                }
            }
            
            // Exponential backoff
            std::thread::sleep(Duration::from_micros(backoff));
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }
    
    pub fn wake(_atomic: &AtomicU32, _count: u32) -> u32 {
        // No-op on non-Linux; waiters poll anyway
        0
    }
    
    pub fn wake_one(_atomic: &AtomicU32) -> bool {
        false
    }
    
    pub fn wake_all(_atomic: &AtomicU32) -> u32 {
        0
    }
}

// Re-export the appropriate implementation
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(not(target_os = "linux"))]
pub use fallback::*;
```

### 2. Ring Buffer Waiting

```rust
// roam-shm/src/ring.rs (new file or inline in host/guest)

use shm_primitives::futex;

/// Wait until the ring has space for a new descriptor.
///
/// shm[impl shm.wakeup.producer-wait]
pub fn wait_for_ring_space(
    head: &AtomicU32,
    tail: &AtomicU32,
    ring_size: u32,
    timeout: Option<Duration>,
) -> Result<(), WaitError> {
    loop {
        let h = head.load(Ordering::Acquire);
        let t = tail.load(Ordering::Acquire);
        
        // Ring is full when (head + 1) % size == tail
        if (h + 1) % ring_size != t {
            return Ok(());  // Has space
        }
        
        // Wait for tail to change (consumer increments tail)
        match futex::wait(tail, t, timeout) {
            WaitResult::Woken | WaitResult::Changed => continue,
            WaitResult::Timeout => return Err(WaitError::Timeout),
        }
    }
}

/// Wait until the ring has data to consume.
///
/// shm[impl shm.wakeup.consumer-wait]
pub fn wait_for_ring_data(
    head: &AtomicU32,
    tail: &AtomicU32,
    timeout: Option<Duration>,
) -> Result<(), WaitError> {
    loop {
        let h = head.load(Ordering::Acquire);
        let t = tail.load(Ordering::Acquire);
        
        // Ring is empty when head == tail
        if h != t {
            return Ok(());  // Has data
        }
        
        // Wait for head to change (producer increments head)
        match futex::wait(head, h, timeout) {
            WaitResult::Woken | WaitResult::Changed => continue,
            WaitResult::Timeout => return Err(WaitError::Timeout),
        }
    }
}

/// Wake consumers waiting for data.
pub fn wake_ring_consumers(head: &AtomicU32) {
    futex::wake_all(head);
}

/// Wake producers waiting for space.
pub fn wake_ring_producers(tail: &AtomicU32) {
    futex::wake_all(tail);
}

#[derive(Debug)]
pub enum WaitError {
    Timeout,
}
```

### 3. Credit Waiting

```rust
// roam-shm/src/channel.rs

impl ChannelEntry {
    /// Wait until enough credit is available.
    ///
    /// shm[impl shm.wakeup.credit-wait]
    pub fn wait_for_credit(
        &self,
        sent_total: u32,
        needed: u32,
        timeout: Option<Duration>,
    ) -> Result<(), WaitError> {
        loop {
            let granted = self.granted_total.load(Ordering::Acquire);
            let remaining = granted.wrapping_sub(sent_total);
            
            if remaining >= needed {
                return Ok(());
            }
            
            match futex::wait(&self.granted_total, granted, timeout) {
                WaitResult::Woken | WaitResult::Changed => continue,
                WaitResult::Timeout => return Err(WaitError::Timeout),
            }
        }
    }
    
    /// Wake senders waiting for credit.
    pub fn wake_senders(&self) {
        futex::wake_all(&self.granted_total);
    }
}
```

### 4. Slot Pool Waiting

```rust
// roam-shm/src/slot_pool.rs

impl SlotPool {
    /// Wait until a slot is available.
    ///
    /// shm[impl shm.wakeup.slot-wait]
    pub fn wait_for_slot(&self, timeout: Option<Duration>) -> Result<SlotHandle, WaitError> {
        loop {
            // Try to allocate
            if let Some(handle) = self.try_alloc() {
                return Ok(handle);
            }
            
            // Wait on first bitmap word (arbitrary choice)
            let word = &self.bitmap[0];
            let current = word.load(Ordering::Acquire);
            
            if current != 0 {
                // There's a free slot, try again
                continue;
            }
            
            match futex::wait(word, current, timeout) {
                WaitResult::Woken | WaitResult::Changed => continue,
                WaitResult::Timeout => return Err(WaitError::Timeout),
            }
        }
    }
    
    /// Free a slot and wake any waiters.
    pub fn free_and_wake(&self, handle: SlotHandle) {
        self.free(handle);
        
        // Pick a single, canonical futex word for slot-waiters and always wake it.
        // This avoids "wait-on-word-0, wake-word-N" bugs.
        futex::wake_one(&self.bitmap[0]);
    }
}
```

### 5. Integrate with Send/Recv

```rust
// host.rs

impl ShmHost {
    /// Send with blocking wait for ring space.
    pub fn send_blocking(
        &mut self,
        peer_id: PeerId,
        frame: Frame,
        timeout: Option<Duration>,
    ) -> Result<(), SendError> {
        let entry = self.peer_entry(peer_id);
        
        // Wait for ring space
        wait_for_ring_space(
            &entry.host_to_guest_head,
            &entry.host_to_guest_tail,
            self.layout.config.ring_size,
            timeout,
        ).map_err(|_| SendError::Timeout)?;
        
        // Actually send
        self.send(peer_id, frame)?;
        
        // Wake consumer
        wake_ring_consumers(&entry.host_to_guest_head);
        
        Ok(())
    }
}

// guest.rs

impl ShmGuest {
    /// Receive with blocking wait for data.
    pub fn recv_blocking(&mut self, timeout: Option<Duration>) -> Result<Frame, RecvError> {
        let entry = self.peer_entry();
        
        // Wait for ring data
        wait_for_ring_data(
            &entry.host_to_guest_head,
            &entry.host_to_guest_tail,
            timeout,
        ).map_err(|_| RecvError::Timeout)?;
        
        // Actually receive
        let frame = self.recv()?;
        
        // Wake producer
        wake_ring_producers(&entry.host_to_guest_tail);
        
        Ok(frame)
    }
}
```

## Tasks

- [ ] Add `futex` module to `shm-primitives`
- [ ] Implement Linux futex wrapper
- [ ] Implement fallback polling for non-Linux
- [ ] Add ring waiting functions
- [ ] Add credit waiting to `ChannelEntry`
- [ ] Add slot waiting to `SlotPool`
- [ ] Integrate blocking send/recv in host and guest
- [ ] Add tracey annotations
- [ ] Write tests

## Testing Strategy

```rust
#[test]
fn test_futex_basic() {
    use std::sync::Arc;
    use std::thread;
    
    let atomic = Arc::new(AtomicU32::new(0));
    let atomic2 = atomic.clone();
    
    // Spawn waiter
    let handle = thread::spawn(move || {
        futex::wait(&atomic2, 0, Some(Duration::from_secs(5)));
        atomic2.load(Ordering::Acquire)
    });
    
    // Give waiter time to sleep
    thread::sleep(Duration::from_millis(50));
    
    // Update and wake
    atomic.store(42, Ordering::Release);
    futex::wake_one(&atomic);
    
    assert_eq!(handle.join().unwrap(), 42);
}

#[test]
fn test_ring_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.shm");
    
    // Small ring to test fullness
    let config = SegmentConfig { ring_size: 4, ..Default::default() };
    let mut host = ShmHost::create(&path, config).unwrap();
    let (_handle, ticket) = host.add_peer(AddPeerOptions::default()).unwrap();
    
    // Fork
    match unsafe { libc::fork() } {
        0 => {
            // Child: fill ring, then block on send
            let args = SpawnArgs { /* from ticket */ };
            let mut guest = ShmGuest::attach_with_ticket(&args).unwrap();
            
            // Fill ring (3 messages for size-4 ring)
            for i in 0..3 {
                guest.send(make_frame(i)).unwrap();
            }
            
            // This should block
            guest.send_blocking(make_frame(99), Some(Duration::from_secs(5))).unwrap();
            std::process::exit(0);
        }
        pid => {
            // Parent: wait a bit, then drain
            thread::sleep(Duration::from_millis(100));
            
            // Drain some messages
            host.recv(ticket.peer_id).unwrap();
            
            // Child should unblock and exit
            let mut status = 0;
            unsafe { libc::waitpid(pid, &mut status, 0) };
            assert!(libc::WIFEXITED(status));
        }
    }
}
```

## Dependencies

- `libc` crate for futex syscall on Linux
- None for fallback (pure Rust polling)

## Notes

- Futex operates on `AtomicU32`, but our ring indices are `AtomicU32` anyway
- SHM is cross-process, so use the shared futex ops (do **not** use `FUTEX_PRIVATE_FLAG`)
- Fallback uses exponential backoff to avoid burning CPU
- Consider adding `FUTEX_WAIT_BITSET` for more selective wakeup on slot pools
- macOS has no futex; fallback uses polling (consider `os_sync_wait_on_address` on newer macOS)
