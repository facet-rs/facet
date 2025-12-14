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

use tokio::io::unix::AsyncFd;
use tokio::io::Interest;

/// A doorbell for cross-process wakeup.
///
/// Uses a Unix domain socketpair (SOCK_DGRAM) for bidirectional signaling.
/// Wrapped in `AsyncFd` for async readiness notification via epoll/kqueue.
pub struct Doorbell {
    /// The async-ready file descriptor.
    async_fd: AsyncFd<OwnedFd>,
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
        // SAFETY: Caller guarantees fd is valid
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };

        // Ensure non-blocking
        set_nonblocking(fd)?;

        // Wrap in AsyncFd
        let async_fd = AsyncFd::new(owned)?;

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
                tracing::warn!("doorbell signal failed: {}", err);
            }
        }
    }

    /// Wait for a signal from the other side.
    ///
    /// Returns when the doorbell becomes readable.
    pub async fn wait(&self) -> io::Result<()> {
        loop {
            let mut guard = self.async_fd.ready(Interest::READABLE).await?;

            // Try to drain the socket
            if self.try_drain() {
                return Ok(());
            }

            // Nothing to read yet, clear readiness and wait again
            guard.clear_ready();
        }
    }

    /// Try to drain all pending signals.
    ///
    /// Returns true if at least one byte was read.
    fn try_drain(&self) -> bool {
        let fd = self.async_fd.get_ref().as_raw_fd();
        let mut buf = [0u8; 64];
        let mut drained = false;

        loop {
            // SAFETY: fd is valid, buf is valid
            let ret = unsafe {
                libc::recv(
                    fd,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                    libc::MSG_DONTWAIT,
                )
            };

            if ret > 0 {
                drained = true;
                // Keep draining
            } else if ret == 0 {
                // Connection closed
                break;
            } else {
                let err = io::Error::last_os_error();
                if err.kind() == ErrorKind::WouldBlock {
                    // No more data
                    break;
                }
                // Other error
                tracing::warn!("doorbell drain failed: {}", err);
                break;
            }
        }

        drained
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
}

/// Create a Unix domain socketpair (SOCK_DGRAM, non-blocking).
fn create_socketpair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];

    // SOCK_DGRAM for datagram semantics (each send is a discrete message)
    // SOCK_NONBLOCK for non-blocking I/O
    // Note: SOCK_CLOEXEC is NOT set so fds can be inherited
    let ret = unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_DGRAM | libc::SOCK_NONBLOCK,
            0,
            fds.as_mut_ptr(),
        )
    };

    if ret < 0 {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: socketpair succeeded, fds are valid
    let fd0 = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let fd1 = unsafe { OwnedFd::from_raw_fd(fds[1]) };

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
}
