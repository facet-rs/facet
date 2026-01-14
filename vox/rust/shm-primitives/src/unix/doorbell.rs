//! Socketpair doorbell for cross-process wakeup.
//!
//! Uses a Unix domain socketpair (SOCK_DGRAM) wrapped in `tokio::io::unix::AsyncFd`
//! for efficient async notification between processes sharing memory.

use std::io::{self, ErrorKind};
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

/// Result of a doorbell signal attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalResult {
    /// Signal was sent successfully.
    Sent,
    /// Buffer was full but peer is alive (signal coalesced with pending ones).
    BufferFull,
    /// Peer has disconnected (socket broken).
    PeerDead,
}

/// Opaque handle for passing doorbell endpoints between processes.
///
/// On Unix, this wraps a raw file descriptor.
/// On Windows, this wraps a named pipe path (see doorbell_windows.rs).
///
/// Use [`Doorbell::create_pair`] to create a pair, then pass this handle
/// to the child process and call [`Doorbell::from_handle`] to reconstruct.
#[derive(Debug)]
pub struct DoorbellHandle(OwnedFd);

impl DoorbellHandle {
    /// Get the raw file descriptor (for passing to child processes).
    pub fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }

    /// Create from a raw file descriptor (in child process after spawn).
    ///
    /// # Safety
    /// The caller must ensure the FD is valid and not owned by anything else.
    pub unsafe fn from_raw_fd(fd: RawFd) -> Self {
        // SAFETY: Caller ensures FD is valid and not owned
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        Self(fd)
    }

    /// Format as a command-line argument value.
    pub fn to_arg(&self) -> String {
        self.0.as_raw_fd().to_string()
    }

    /// Parse from a command-line argument value.
    ///
    /// # Safety
    /// The FD must be valid and not owned by anything else.
    /// This is typically only safe to call in a child process that inherited the FD.
    pub unsafe fn from_arg(s: &str) -> Result<Self, std::num::ParseIntError> {
        let fd: RawFd = s.parse()?;
        let handle = unsafe { Self::from_raw_fd(fd) };
        Ok(handle)
    }

    /// The CLI argument name for this platform.
    pub const ARG_NAME: &'static str = "--doorbell-fd";
}

/// A doorbell for cross-process wakeup.
///
/// Uses a Unix domain socketpair (SOCK_DGRAM) for bidirectional signaling.
/// Wrapped in `AsyncFd` for async readiness notification via epoll/kqueue.
pub struct Doorbell {
    async_fd: AsyncFd<OwnedFd>,
    /// Whether we've already logged that the peer is dead (to avoid spam).
    peer_dead_logged: AtomicBool,
}

fn drain_fd(fd: RawFd, would_block_is_error: bool) -> io::Result<bool> {
    let mut buf = [0u8; 64];
    let mut drained = false;

    loop {
        let ret = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0) };

        if ret > 0 {
            drained = true;
            continue;
        }

        if ret == 0 {
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

        return Err(err);
    }
}

impl Doorbell {
    /// Create a socketpair and return (host_doorbell, guest_handle).
    ///
    /// The guest_handle should be passed to the plugin (e.g., via command line).
    /// The host keeps the Doorbell.
    pub fn create_pair() -> io::Result<(Self, DoorbellHandle)> {
        let (host_fd, peer_fd) = create_socketpair()?;

        set_nonblocking(host_fd.as_raw_fd())?;

        let async_fd = AsyncFd::new(host_fd)?;

        Ok((
            Self {
                async_fd,
                peer_dead_logged: AtomicBool::new(false),
            },
            DoorbellHandle(peer_fd),
        ))
    }

    /// Create a Doorbell from an opaque handle (guest/plugin side).
    ///
    /// This is the cross-platform way to reconstruct a Doorbell in a spawned process.
    /// Consumes the handle, taking ownership of the underlying file descriptor.
    pub fn from_handle(handle: DoorbellHandle) -> io::Result<Self> {
        use std::os::unix::io::IntoRawFd;
        Self::from_raw_fd(handle.0.into_raw_fd())
    }

    /// Create a Doorbell from a raw file descriptor (plugin side).
    ///
    /// Prefer [`from_handle`] for cross-platform code.
    ///
    /// # Safety
    ///
    /// The fd must be a valid, open file descriptor from a socketpair.
    pub fn from_raw_fd(fd: RawFd) -> io::Result<Self> {
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        set_nonblocking(fd)?;
        let async_fd = AsyncFd::new(owned)?;
        Ok(Self {
            async_fd,
            peer_dead_logged: AtomicBool::new(false),
        })
    }

    /// Signal the other side.
    ///
    /// Sends a 1-byte datagram. If the socket buffer is full (EAGAIN),
    /// the signal is dropped (the other side is already signaled).
    ///
    /// Returns `SignalResult::PeerDead` if the peer has disconnected
    /// (EPIPE, ECONNRESET, ENOTCONN). This is logged once per doorbell
    /// to avoid spam.
    pub fn signal(&self) -> SignalResult {
        let fd = self.async_fd.get_ref().as_raw_fd();
        let buf = [1u8];

        let ret = unsafe {
            libc::send(
                fd,
                buf.as_ptr() as *const libc::c_void,
                buf.len(),
                libc::MSG_DONTWAIT,
            )
        };

        if ret > 0 {
            return SignalResult::Sent;
        }

        if ret == 0 {
            // Shouldn't happen for SOCK_DGRAM, but treat as success
            return SignalResult::Sent;
        }

        let err = io::Error::last_os_error();
        match err.kind() {
            ErrorKind::WouldBlock => SignalResult::BufferFull,
            // These all indicate the peer is gone
            ErrorKind::BrokenPipe | ErrorKind::ConnectionReset | ErrorKind::NotConnected => {
                SignalResult::PeerDead
            }
            _ => {
                // Some other error - also indicates peer is dead, but log it once
                if !self.peer_dead_logged.swap(true, Ordering::Relaxed) {
                    tracing::debug!(fd, error = %err, "doorbell signal failed (peer likely dead)");
                }
                SignalResult::PeerDead
            }
        }
    }

    /// Check if the peer appears to be dead (signal has failed).
    pub fn is_peer_dead(&self) -> bool {
        self.peer_dead_logged.load(Ordering::Relaxed)
    }

    /// Wait for a signal from the other side.
    pub async fn wait(&self) -> io::Result<()> {
        if self.try_drain() {
            return Ok(());
        }

        loop {
            let mut guard = self.async_fd.ready(Interest::READABLE).await?;

            let drained = guard.try_io(|inner| {
                let fd = inner.get_ref().as_raw_fd();
                drain_fd(fd, true).map(|_| ())
            });

            match drained {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) => return Err(e),
                Err(_would_block) => continue,
            }
        }
    }

    fn try_drain(&self) -> bool {
        let fd = self.async_fd.get_ref().as_raw_fd();
        match drain_fd(fd, false) {
            Ok(drained) => drained,
            Err(err) => {
                tracing::warn!(fd, error = %err, "doorbell drain failed");
                false
            }
        }
    }

    /// Drain any pending signals without blocking.
    pub fn drain(&self) {
        self.try_drain();
    }

    /// Get the number of bytes pending in the socket buffer (for diagnostics).
    pub fn pending_bytes(&self) -> usize {
        let fd = self.async_fd.get_ref().as_raw_fd();
        let mut pending: libc::c_int = 0;
        let ret = unsafe { libc::ioctl(fd, libc::FIONREAD, &mut pending) };
        if ret < 0 { 0 } else { pending as usize }
    }
}

fn create_socketpair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];

    #[cfg(target_os = "linux")]
    let sock_type = libc::SOCK_DGRAM | libc::SOCK_NONBLOCK;
    #[cfg(not(target_os = "linux"))]
    let sock_type = libc::SOCK_DGRAM;

    let ret = unsafe { libc::socketpair(libc::AF_UNIX, sock_type, 0, fds.as_mut_ptr()) };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }

    let fd0 = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let fd1 = unsafe { OwnedFd::from_raw_fd(fds[1]) };

    #[cfg(not(target_os = "linux"))]
    {
        set_nonblocking(fd0.as_raw_fd())?;
        set_nonblocking(fd1.as_raw_fd())?;
    }

    Ok((fd0, fd1))
}

/// Set a file descriptor to non-blocking mode.
pub fn set_nonblocking(fd: RawFd) -> io::Result<()> {
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

/// Validate that a file descriptor is valid and open.
///
/// Uses fcntl(F_GETFL) to check if the fd is valid.
pub fn validate_fd(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Clear the close-on-exec flag so the fd is inherited by children.
///
/// Call this on the guest's doorbell fd before spawning.
///
/// shm[impl shm.spawn.fd-inheritance]
pub fn clear_cloexec(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }

    let ret = unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
