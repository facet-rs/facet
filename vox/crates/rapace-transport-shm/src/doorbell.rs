//! Socketpair doorbell for cross-process wakeup.
//!
//! This module provides a cross-platform (Linux epoll, macOS kqueue) mechanism
//! for one process to wake another without polling. Each peer gets one end of
//! a Unix domain socketpair.
//!
//! # Usage
//!
//! ```ignore
//! // Host side: create socketpair, keep host_fd, give peer_fd to plugin
//! let (host_doorbell, peer_fd) = Doorbell::create_pair()?;
//!
//! // Pass peer_fd to plugin via --doorbell-fd=N
//! // Plugin side: wrap inherited fd
//! let plugin_doorbell = Doorbell::from_raw_fd(peer_fd)?;
//!
//! // Signal the other side
//! doorbell.signal();
//!
//! // Wait for signal (async)
//! doorbell.wait().await;
//! ```

use std::io::{self, ErrorKind};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

/// A doorbell for cross-process wakeup.
///
/// Uses a Unix domain socketpair (SOCK_DGRAM) for bidirectional signaling.
/// Wrapped in `AsyncFd` for async readiness notification via epoll/kqueue.
pub struct Doorbell {
    /// The async-ready file descriptor.
    async_fd: AsyncFd<OwnedFd>,
}

fn drain_fd(fd: RawFd, would_block_is_error: bool) -> io::Result<bool> {
    let mut buf = [0u8; 64];
    let mut drained = false;

    loop {
        // SAFETY: fd is valid, buf is valid
        let ret = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0) };

        if ret > 0 {
            drained = true;
            continue;
        }

        if ret == 0 {
            // For SOCK_DGRAM this generally shouldn't happen (we always send 1 byte),
            // but treat it as "nothing to drain".
            return Ok(drained);
        }

        let err = io::Error::last_os_error();
        if err.kind() == ErrorKind::WouldBlock {
            if drained {
                return Ok(true);
            }
            return if would_block_is_error {
                Err(err)
            } else {
                Ok(false)
            };
        }

        Err(err)?;
    }
}

impl Doorbell {
    /// Create a socketpair and return (host_doorbell, peer_raw_fd).
    ///
    /// The peer_raw_fd should be passed to the plugin (e.g., via --doorbell-fd=N).
    /// The host keeps the Doorbell.
    ///
    /// # FD Inheritance
    ///
    /// The returned peer_raw_fd does NOT have CLOEXEC set, so it will be
    /// inherited by child processes. The host should close it after spawning
    /// the plugin.
    pub fn create_pair() -> io::Result<(Self, RawFd)> {
        // Create socketpair
        let (host_fd, peer_fd) = create_socketpair()?;

        // Set host_fd to non-blocking (peer_fd is already non-blocking from socketpair)
        set_nonblocking(host_fd.as_raw_fd())?;

        // Wrap host_fd in AsyncFd
        let async_fd = AsyncFd::new(host_fd)?;

        let peer_raw = peer_fd.into_raw_fd(); // Convert to raw, caller owns it

        Ok((Self { async_fd }, peer_raw))
    }

    /// Create a Doorbell from a raw file descriptor.
    ///
    /// This is used by the plugin side to wrap the inherited fd.
    ///
    /// # Safety
    ///
    /// The fd must be a valid, open file descriptor from a socketpair.
    pub fn from_raw_fd(fd: RawFd) -> io::Result<Self> {
        tracing::debug!(fd = fd, "Doorbell::from_raw_fd: wrapping inherited FD");

        // SAFETY: Caller guarantees fd is valid
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };

        // Ensure non-blocking
        set_nonblocking(fd)?;
        tracing::debug!(fd = fd, "Doorbell::from_raw_fd: set non-blocking");

        // Wrap in AsyncFd
        let async_fd = AsyncFd::new(owned)?;
        tracing::debug!(
            fd = fd,
            "Doorbell::from_raw_fd: registered with AsyncFd (tokio epoll)"
        );

        Ok(Self { async_fd })
    }

    /// Signal the other side.
    ///
    /// Sends a 1-byte datagram. If the socket buffer is full (EAGAIN),
    /// the signal is dropped (the other side is already signaled).
    pub fn signal(&self) {
        let fd = self.async_fd.get_ref().as_raw_fd();
        let buf = [1u8];

        // SAFETY: fd is valid, buf is valid
        let ret = unsafe {
            libc::send(
                fd,
                buf.as_ptr() as *const libc::c_void,
                buf.len(),
                libc::MSG_DONTWAIT,
            )
        };

        if ret < 0 {
            let err = io::Error::last_os_error();
            // EAGAIN/EWOULDBLOCK means buffer is full - that's fine, already signaled
            if err.kind() != ErrorKind::WouldBlock {
                tracing::warn!(fd = fd, error = %err, "doorbell signal failed");
            } else {
                tracing::trace!(fd = fd, "doorbell signal: buffer full (already signaled)");
            }
        } else {
            tracing::trace!(fd = fd, "doorbell signal: sent 1 byte");
        }
    }

    /// Wait for a signal from the other side.
    ///
    /// Returns when the doorbell becomes readable.
    pub async fn wait(&self) -> io::Result<()> {
        let fd = self.async_fd.get_ref().as_raw_fd();
        tracing::trace!(fd = fd, "doorbell wait: entering wait loop");

        // CRITICAL: Check for pending data BEFORE the first await!
        // If a signal was sent before we started waiting, it's already in the buffer.
        // The epoll might not notify us for data that arrived before registration.
        if self.try_drain() {
            tracing::trace!(
                fd = fd,
                "doorbell wait: found pending data, returning immediately"
            );
            return Ok(());
        }

        loop {
            tracing::trace!(fd = fd, "doorbell wait: awaiting readiness");
            let mut guard = self.async_fd.ready(Interest::READABLE).await?;
            tracing::trace!(fd = fd, "doorbell wait: ready, trying to drain");

            // Drain using AsyncFd's `try_io` to avoid the classic readiness race:
            // if a byte arrives between a nonblocking read observing EWOULDBLOCK and
            // `clear_ready()`, we can accidentally clear readiness for the new byte.
            match guard.try_io(|inner| drain_fd(inner.as_raw_fd(), true)) {
                Ok(Ok(true)) => {
                    tracing::trace!(fd = fd, "doorbell wait: drained, returning");
                    return Ok(());
                }
                Ok(Ok(false)) => {
                    // No bytes read, but also not WouldBlock. Loop and await again.
                    continue;
                }
                Ok(Err(e)) => return Err(e),
                Err(_would_block) => {
                    // Readiness was a false-positive; loop to await again.
                    continue;
                }
            }
        }
    }

    /// Try to drain all pending signals.
    ///
    /// Returns true if at least one byte was read.
    fn try_drain(&self) -> bool {
        let fd = self.async_fd.get_ref().as_raw_fd();
        match drain_fd(fd, false) {
            Ok(drained) => drained,
            Err(err) => {
                tracing::warn!(fd = fd, error = %err, "doorbell drain failed");
                false
            }
        }
    }

    /// Drain any pending signals without blocking.
    ///
    /// Call this after being woken to clear the readable state.
    pub fn drain(&self) {
        self.try_drain();
    }

    /// Get the raw file descriptor.
    pub fn as_raw_fd(&self) -> RawFd {
        self.async_fd.get_ref().as_raw_fd()
    }

    /// Get the number of bytes pending in the socket buffer (for diagnostics).
    ///
    /// Uses ioctl(FIONREAD) to query the receive buffer.
    /// Returns 0 if the ioctl fails.
    pub fn pending_bytes(&self) -> usize {
        let fd = self.async_fd.get_ref().as_raw_fd();
        let mut pending: libc::c_int = 0;

        // SAFETY: fd is valid, pending is valid pointer
        let ret = unsafe { libc::ioctl(fd, libc::FIONREAD, &mut pending) };

        if ret < 0 {
            tracing::warn!(fd = fd, error = %io::Error::last_os_error(), "FIONREAD failed");
            0
        } else {
            pending as usize
        }
    }
}

/// Create a Unix domain socketpair (SOCK_DGRAM, non-blocking).
fn create_socketpair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];

    // SOCK_DGRAM for datagram semantics (each send is a discrete message)
    // Note: SOCK_CLOEXEC is NOT set so fds can be inherited
    //
    // On Linux, we can use SOCK_NONBLOCK directly. On macOS/BSD, this flag
    // doesn't exist, so we set non-blocking mode separately with fcntl.
    #[cfg(target_os = "linux")]
    let sock_type = libc::SOCK_DGRAM | libc::SOCK_NONBLOCK;
    #[cfg(not(target_os = "linux"))]
    let sock_type = libc::SOCK_DGRAM;

    let ret = unsafe { libc::socketpair(libc::AF_UNIX, sock_type, 0, fds.as_mut_ptr()) };

    if ret < 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: socketpair succeeded, fds are valid
    let fd0 = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let fd1 = unsafe { OwnedFd::from_raw_fd(fds[1]) };

    // On non-Linux platforms, set non-blocking mode explicitly
    #[cfg(not(target_os = "linux"))]
    {
        set_nonblocking(fd0.as_raw_fd())?;
        set_nonblocking(fd1.as_raw_fd())?;
    }

    Ok((fd0, fd1))
}

/// Set a file descriptor to non-blocking mode.
fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }

    let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

/// Close the peer end of a socketpair (host side, after spawning plugin).
///
/// # Safety
///
/// fd must be a valid file descriptor that the caller owns.
pub fn close_peer_fd(fd: RawFd) {
    unsafe {
        libc::close(fd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn test_doorbell_signal_and_wait() {
        let (doorbell1, peer_fd) = Doorbell::create_pair().unwrap();
        let doorbell2 = Doorbell::from_raw_fd(peer_fd).unwrap();

        // Signal from 1 to 2
        doorbell1.signal();

        // 2 should be able to wait and receive
        tokio::time::timeout(std::time::Duration::from_millis(100), doorbell2.wait())
            .await
            .expect("timeout waiting for doorbell")
            .expect("wait failed");

        // Signal from 2 to 1
        doorbell2.signal();

        // 1 should be able to wait and receive
        tokio::time::timeout(std::time::Duration::from_millis(100), doorbell1.wait())
            .await
            .expect("timeout waiting for doorbell")
            .expect("wait failed");
    }

    #[tokio::test]
    async fn test_doorbell_multiple_signals() {
        let (doorbell1, peer_fd) = Doorbell::create_pair().unwrap();
        let doorbell2 = Doorbell::from_raw_fd(peer_fd).unwrap();

        // Signal multiple times
        doorbell1.signal();
        doorbell1.signal();
        doorbell1.signal();

        // Single wait should drain all
        tokio::time::timeout(std::time::Duration::from_millis(100), doorbell2.wait())
            .await
            .expect("timeout")
            .expect("wait failed");

        // Drain explicitly (should be no-op now)
        doorbell2.drain();
    }

    #[test]
    fn test_socketpair_creation() {
        let (fd1, fd2) = create_socketpair().unwrap();

        // Both fds should be valid
        assert!(fd1.as_raw_fd() >= 0);
        assert!(fd2.as_raw_fd() >= 0);
        assert_ne!(fd1.as_raw_fd(), fd2.as_raw_fd());
    }

    /// Stress test to verify no missed wakeups under load.
    ///
    /// This test is tuned to be robust when run in parallel with other async tests.
    /// Each `#[tokio::test(flavor="multi_thread")]` spins up its own runtime, so
    /// running many such tests concurrently causes CPU contention. We mitigate this
    /// by: (a) using fewer iterations, (b) yielding the sender frequently, and
    /// (c) wrapping the entire test in an overall timeout.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_doorbell_no_missed_wakeup_under_load() {
        // Overall timeout for the entire test body
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let (doorbell1, peer_fd) = Doorbell::create_pair().unwrap();
            let doorbell2 = Doorbell::from_raw_fd(peer_fd).unwrap();

            let done = std::sync::Arc::new(AtomicBool::new(false));
            let done_sender = done.clone();

            let sender = tokio::spawn(async move {
                let mut i: u64 = 0;
                while !done_sender.load(Ordering::Relaxed) {
                    doorbell1.signal();
                    i += 1;
                    // Yield frequently to share CPU with receiver and other tests
                    if i.is_multiple_of(16) {
                        tokio::task::yield_now().await;
                    }
                }
            });

            // 1000 iterations is enough to catch missed-wakeup bugs while staying
            // fast enough for parallel test runs (~20-100ms in isolation).
            for _ in 0..1_000usize {
                tokio::time::timeout(std::time::Duration::from_millis(100), doorbell2.wait())
                    .await
                    .expect("timeout waiting for doorbell")
                    .expect("wait failed");
            }

            done.store(true, Ordering::Relaxed);
            sender.await.expect("sender task panicked");
        })
        .await
        .expect("test timed out after 10 seconds");
    }
}
