//! Socketpair doorbell for cross-process wakeup.
//!
//! Uses a Unix domain socketpair (SOCK_STREAM) wrapped in `tokio::io::unix::AsyncFd`
//! for efficient async notification between processes sharing memory.
//!
//! r[impl shm.signal]
//! r[impl shm.signal.doorbell.optional]

use std::io::{self, ErrorKind};
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

/// Result of a doorbell signal attempt.
///
/// r[impl shm.signal.doorbell]
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

    /// Consume this handle and return the owned raw fd.
    pub fn into_raw_fd(self) -> RawFd {
        use std::os::unix::io::IntoRawFd;
        self.0.into_raw_fd()
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
/// Uses a Unix domain socketpair (SOCK_STREAM) for bidirectional signaling.
/// Wrapped in `AsyncFd` for async readiness notification via epoll/kqueue.
pub struct Doorbell {
    async_fd: AsyncFd<OwnedFd>,
    /// Whether we've already logged that the peer is dead (to avoid spam).
    peer_dead_logged: AtomicBool,
}

fn drain_fd(fd: RawFd, would_block_is_error: bool, eof_is_error: bool) -> io::Result<bool> {
    let mut buf = [0u8; 64];
    let mut drained = false;

    loop {
        let ret = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0) };

        if ret > 0 {
            drained = true;
            continue;
        }

        if ret == 0 {
            // EOF: peer closed the connection
            if eof_is_error {
                return Err(io::Error::new(
                    ErrorKind::BrokenPipe,
                    "doorbell peer closed (recv returned 0)",
                ));
            }
            return Ok(drained);
        }

        let err = io::Error::last_os_error();
        if err.kind() == ErrorKind::Interrupted {
            // Retry transparently on EINTR.
            continue;
        }
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
    /// Signal the other side without awaiting readiness.
    ///
    /// Performs a single non-blocking send attempt and returns immediately.
    pub fn signal_now(&self) -> SignalResult {
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
            return SignalResult::PeerDead;
        }

        let err = io::Error::last_os_error();
        if err.kind() == ErrorKind::WouldBlock || err.raw_os_error() == Some(libc::ENOBUFS) {
            return SignalResult::BufferFull;
        }

        match err.kind() {
            ErrorKind::BrokenPipe | ErrorKind::ConnectionReset | ErrorKind::NotConnected => {
                SignalResult::PeerDead
            }
            _ => {
                if !self.peer_dead_logged.swap(true, Ordering::Relaxed) {
                    tracing::debug!(fd, error = %err, "doorbell signal failed (peer likely dead)");
                }
                SignalResult::PeerDead
            }
        }
    }

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
        // Safety: IntoRawFd transfers ownership out of handle.0.
        unsafe { Self::from_raw_fd(handle.0.into_raw_fd()) }
    }

    /// Create a Doorbell from a raw file descriptor (plugin side).
    ///
    /// Prefer [`Self::from_handle`] for cross-platform code.
    ///
    /// # Safety
    ///
    /// `fd` must be a valid, open file descriptor from a socketpair, and the
    /// caller must transfer unique ownership — no other owner may close or
    /// use it afterwards.
    pub unsafe fn from_raw_fd(fd: RawFd) -> io::Result<Self> {
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
    /// Sends a 1-byte message. If the socket buffer is full (EAGAIN),
    /// the signal is dropped (the other side is already signaled).
    ///
    /// Returns `SignalResult::PeerDead` if the peer has disconnected.
    ///
    /// r[impl shm.signal.doorbell.signal]
    /// r[impl shm.signal.doorbell.integration]
    pub async fn signal(&self) -> SignalResult {
        self.signal_now()
    }

    /// Check if the peer appears to be dead (signal has failed).
    pub fn is_peer_dead(&self) -> bool {
        self.peer_dead_logged.load(Ordering::Relaxed)
    }

    /// Wait for a signal from the other side.
    ///
    /// r[impl shm.signal.doorbell.wait]
    pub async fn wait(&self) -> io::Result<()> {
        if self.try_drain() {
            return Ok(());
        }

        loop {
            let mut guard = self.async_fd.ready(Interest::READABLE).await?;

            let drained = guard.try_io(|inner| {
                let fd = inner.get_ref().as_raw_fd();
                drain_fd(fd, true, true).map(|_| ())
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
        match drain_fd(fd, false, false) {
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

    /// Accept an incoming connection (no-op on Unix).
    ///
    /// On Unix, socketpairs are already connected when created, so this is a no-op.
    /// On Windows, named pipe servers must call this to accept the client connection.
    pub async fn accept(&self) -> io::Result<()> {
        // Unix socketpairs are already connected
        Ok(())
    }

    /// Get the number of bytes pending in the socket buffer (for diagnostics).
    ///
    /// r[impl shm.signal.doorbell.death]
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
    let sock_type = libc::SOCK_STREAM | libc::SOCK_NONBLOCK;
    #[cfg(not(target_os = "linux"))]
    let sock_type = libc::SOCK_STREAM;

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
/// r[impl shm.spawn.fd-inheritance]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::IntoRawFd;
    use std::os::unix::io::AsRawFd;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn create_blocking_socketpair() -> io::Result<(OwnedFd, OwnedFd)> {
        let mut fds = [0i32; 2];
        let ret =
            unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd0 = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        let fd1 = unsafe { OwnedFd::from_raw_fd(fds[1]) };
        Ok((fd0, fd1))
    }

    extern "C" fn noop_signal_handler(_sig: libc::c_int) {}

    #[test]
    fn wait_returns_broken_pipe_when_peer_closed() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("tokio runtime");

        let err = rt
            .block_on(async {
                let (doorbell, peer) = Doorbell::create_pair().expect("create doorbell pair");
                drop(peer);
                doorbell.wait().await
            })
            .expect_err("wait should fail when peer has closed");
        assert_eq!(
            err.kind(),
            ErrorKind::BrokenPipe,
            "expected BrokenPipe, got {err:?}"
        );
    }

    #[test]
    fn drain_fd_reports_eof_as_error_when_requested() {
        let (a, b) = create_socketpair().expect("create socketpair");
        drop(b);
        let err = drain_fd(a.as_raw_fd(), true, true).expect_err("expected eof error");
        assert_eq!(
            err.kind(),
            ErrorKind::BrokenPipe,
            "expected BrokenPipe, got {err:?}"
        );

        let (a2, b2) = create_socketpair().expect("create socketpair");
        drop(b2);
        let drained = drain_fd(a2.as_raw_fd(), false, false).expect("drain without eof error");
        assert!(!drained, "expected no drained bytes on clean EOF");
    }

    #[test]
    fn drain_fd_retries_on_eintr() {
        let (reader, writer) = create_blocking_socketpair().expect("create blocking socketpair");

        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = noop_signal_handler as *const () as usize;
        action.sa_flags = 0;
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
        }
        let mut old_action: libc::sigaction = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::sigaction(libc::SIGUSR1, &action, &mut old_action) };
        assert_eq!(rc, 0, "install SIGUSR1 handler");

        let reader_fd = reader.into_raw_fd();
        let writer_fd = writer.into_raw_fd();
        let (tid_tx, tid_rx) = mpsc::channel();

        let waiter = thread::spawn(move || {
            let tid = unsafe { libc::pthread_self() };
            tid_tx.send(tid).expect("send pthread id");
            let result = drain_fd(reader_fd, false, false);
            unsafe {
                libc::close(reader_fd);
            }
            result
        });

        let tid = tid_rx.recv().expect("receive pthread id");
        thread::sleep(Duration::from_millis(20));
        let kill_rc = unsafe { libc::pthread_kill(tid, libc::SIGUSR1) };
        assert_eq!(kill_rc, 0, "deliver SIGUSR1 to waiting thread");

        let byte = [1_u8];
        let write_rc = unsafe { libc::write(writer_fd, byte.as_ptr().cast::<libc::c_void>(), 1) };
        assert_eq!(write_rc, 1, "write wake byte");
        unsafe {
            libc::close(writer_fd);
        }

        let drained = waiter
            .join()
            .expect("waiter thread panicked")
            .expect("drain_fd should not fail on EINTR");
        assert!(drained, "expected to drain at least one byte");

        let restore_rc =
            unsafe { libc::sigaction(libc::SIGUSR1, &old_action, std::ptr::null_mut()) };
        assert_eq!(restore_rc, 0, "restore SIGUSR1 handler");
    }

    #[tokio::test]
    async fn handle_roundtrip_allows_bidirectional_signal_wait() {
        let (host, handle) = Doorbell::create_pair().expect("create doorbell pair");
        let guest = Doorbell::from_handle(handle).expect("reconstruct guest doorbell from handle");

        assert_eq!(guest.signal_now(), SignalResult::Sent);
        host.wait()
            .await
            .expect("host wait should receive guest signal");

        assert_eq!(host.signal_now(), SignalResult::Sent);
        guest
            .wait()
            .await
            .expect("guest wait should receive host signal");
    }

    #[tokio::test]
    async fn signal_now_reports_peer_dead_after_peer_closed() {
        let (host, peer) = Doorbell::create_pair().expect("create doorbell pair");
        drop(peer);
        assert_eq!(host.signal_now(), SignalResult::PeerDead);
    }

    #[tokio::test]
    async fn clear_cloexec_clears_fd_cloexec_flag() {
        let (_host, handle) = Doorbell::create_pair().expect("create doorbell pair");
        let fd = handle.as_raw_fd();

        let set_ret = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
        assert_eq!(set_ret, 0, "setting FD_CLOEXEC should succeed");

        clear_cloexec(fd).expect("clear_cloexec should succeed");

        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert!(flags >= 0, "F_GETFD should succeed");
        assert_eq!(flags & libc::FD_CLOEXEC, 0, "FD_CLOEXEC should be cleared");
    }

    #[tokio::test]
    async fn close_peer_fd_closes_descriptor_and_validate_fd_reports_error() {
        let (_host, handle) = Doorbell::create_pair().expect("create doorbell pair");
        let fd = handle.into_raw_fd();
        validate_fd(fd).expect("fd should be valid before close");
        close_peer_fd(fd);
        let err = validate_fd(fd).expect_err("fd should be invalid after close_peer_fd");
        assert_eq!(err.raw_os_error(), Some(libc::EBADF));
    }
}
