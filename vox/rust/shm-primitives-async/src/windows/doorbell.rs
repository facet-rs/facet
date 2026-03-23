//! Win32 Event doorbell for cross-process wakeup (Windows).
//!
//! Uses named auto-reset Win32 Events for bidirectional signaling between
//! processes sharing memory. Each doorbell pair uses two named events:
//! one for host→guest signals and one for guest→host signals.
//!
//! This replaces the previous named-pipe implementation which suffered from
//! IOCP spurious readability when the remote process died, causing a
//! busy-loop that starved the tokio runtime.

use std::io::{self, ErrorKind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_EVENT, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, EVENT_ALL_ACCESS, INFINITE, OpenEventW, SetEvent, WaitForMultipleObjects,
    WaitForSingleObject,
};

use std::format;
use std::string::{String, ToString};

/// Result of a doorbell signal attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalResult {
    /// Signal was sent successfully.
    Sent,
    /// Buffer was full but peer is alive (signal coalesced with pending ones).
    BufferFull,
    /// Peer has disconnected (pipe broken).
    PeerDead,
}

/// Opaque handle for passing doorbell endpoints between processes.
///
/// On Windows, this wraps an event name prefix (UUID).
/// On Unix, this wraps a raw file descriptor (see doorbell.rs).
///
/// Use [`Doorbell::create_pair`] to create a pair, then pass this handle
/// to the child process and call [`Doorbell::from_handle`] to reconstruct.
#[derive(Debug)]
pub struct DoorbellHandle(String);

impl DoorbellHandle {
    /// Get the event name prefix (for passing to child processes).
    pub fn as_pipe_name(&self) -> &str {
        &self.0
    }

    /// Create from an event name prefix (in child process after spawn).
    pub fn from_pipe_name(name: String) -> Self {
        Self(name)
    }

    /// Format as a command-line argument value.
    pub fn to_arg(&self) -> String {
        self.0.clone()
    }

    /// Parse from a command-line argument value.
    /// # Safety
    /// The caller must ensure the name refers to a valid doorbell event pair.
    pub unsafe fn from_arg(s: &str) -> Result<Self, std::convert::Infallible> {
        Ok(Self(s.to_string()))
    }

    /// The CLI argument name for this platform.
    pub const ARG_NAME: &'static str = "--doorbell-pipe";
}

/// Internal state shared between Doorbell and its spawned blocking tasks.
struct DoorbellInner {
    /// Auto-reset event that THIS side signals (other side waits on it).
    tx_event: HANDLE,
    /// Auto-reset event that THIS side waits on (other side signals it).
    rx_event: HANDLE,
    /// Manual-reset event, unnamed, signaled on drop to cancel pending waits.
    cancel_event: HANDLE,
    /// Event name prefix for diagnostics.
    name_prefix: String,
    /// Whether we've already logged that the peer is dead (to avoid spam).
    peer_dead_logged: AtomicBool,
}

// SAFETY: HANDLE values are plain kernel object handles (isize), safe to send
// between threads and access concurrently. SetEvent/WaitForMultipleObjects are
// thread-safe Win32 APIs.
unsafe impl Send for DoorbellInner {}
unsafe impl Sync for DoorbellInner {}

impl Drop for DoorbellInner {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.tx_event);
            CloseHandle(self.rx_event);
            CloseHandle(self.cancel_event);
        }
    }
}

/// A doorbell for cross-process wakeup.
///
/// On Windows, uses named Win32 Events for bidirectional signaling.
/// The host creates the events, and guests open them by name.
pub struct Doorbell {
    inner: Arc<DoorbellInner>,
}

impl Drop for Doorbell {
    fn drop(&mut self) {
        // Signal cancel to unblock any pending wait() in spawn_blocking.
        unsafe {
            SetEvent(self.inner.cancel_event);
        }
    }
}

impl Doorbell {
    /// Signal the other side without awaiting readiness.
    ///
    /// Sets the tx event, which wakes the other side's `wait()`.
    /// Always non-blocking. With auto-reset events, multiple signals
    /// before a wait are coalesced (event stays signaled until consumed).
    pub fn signal_now(&self) -> SignalResult {
        let ok = unsafe { SetEvent(self.inner.tx_event) };
        if ok != 0 {
            return SignalResult::Sent;
        }
        let err = io::Error::last_os_error();
        if !self.inner.peer_dead_logged.swap(true, Ordering::Relaxed) {
            tracing::debug!(name = %self.inner.name_prefix, error = %err, "doorbell SetEvent failed");
        }
        SignalResult::PeerDead
    }

    /// Create a named event pair and return (host_doorbell, guest_handle).
    ///
    /// The guest_handle should be passed to the plugin (e.g., via command line).
    /// The host keeps the Doorbell.
    pub fn create_pair() -> io::Result<(Self, DoorbellHandle)> {
        let uuid = generate_uuid();

        // Host→guest event (auto-reset, initially non-signaled)
        let h2g_name = to_wide(&format!("Local\\vox-doorbell-{uuid}-h2g"));
        let h2g = unsafe { CreateEventW(std::ptr::null(), 0, 0, h2g_name.as_ptr()) };
        if h2g.is_null() || h2g == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        // Guest→host event (auto-reset, initially non-signaled)
        let g2h_name = to_wide(&format!("Local\\vox-doorbell-{uuid}-g2h"));
        let g2h = unsafe { CreateEventW(std::ptr::null(), 0, 0, g2h_name.as_ptr()) };
        if g2h.is_null() || g2h == INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(h2g);
            }
            return Err(io::Error::last_os_error());
        }

        // Cancel event (manual-reset, unnamed, initially non-signaled)
        let cancel = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if cancel.is_null() || cancel == INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(h2g);
                CloseHandle(g2h);
            }
            return Err(io::Error::last_os_error());
        }

        Ok((
            Self {
                inner: Arc::new(DoorbellInner {
                    tx_event: h2g, // host signals h2g
                    rx_event: g2h, // host waits on g2h
                    cancel_event: cancel,
                    name_prefix: uuid.clone(),
                    peer_dead_logged: AtomicBool::new(false),
                }),
            },
            DoorbellHandle(uuid),
        ))
    }

    /// Create a Doorbell from an opaque handle (guest/plugin side).
    ///
    /// This is the cross-platform way to reconstruct a Doorbell in a spawned process.
    pub fn from_handle(handle: DoorbellHandle) -> io::Result<Self> {
        Self::connect(&handle.0)
    }

    /// Connect to an existing event pair by name prefix.
    ///
    /// Prefer [`from_handle`] for cross-platform code.
    ///
    /// This is for spawned guest processes that receive the event name prefix.
    pub fn connect(name: &str) -> io::Result<Self> {
        // Open host→guest event (guest waits on this)
        let h2g_name = to_wide(&format!("Local\\vox-doorbell-{name}-h2g"));
        let h2g = unsafe { OpenEventW(EVENT_ALL_ACCESS, 0, h2g_name.as_ptr()) };
        if h2g.is_null() || h2g == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        // Open guest→host event (guest signals this)
        let g2h_name = to_wide(&format!("Local\\vox-doorbell-{name}-g2h"));
        let g2h = unsafe { OpenEventW(EVENT_ALL_ACCESS, 0, g2h_name.as_ptr()) };
        if g2h.is_null() || g2h == INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(h2g);
            }
            return Err(io::Error::last_os_error());
        }

        // Cancel event (local, manual-reset, unnamed)
        let cancel = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if cancel.is_null() || cancel == INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(h2g);
                CloseHandle(g2h);
            }
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            inner: Arc::new(DoorbellInner {
                tx_event: g2h, // guest signals g2h
                rx_event: h2g, // guest waits on h2g
                cancel_event: cancel,
                name_prefix: name.to_string(),
                peer_dead_logged: AtomicBool::new(false),
            }),
        })
    }

    /// Signal the other side.
    ///
    /// Sets the auto-reset event. If already signaled (multiple signals
    /// before a wait), the signal is coalesced.
    pub async fn signal(&self) -> SignalResult {
        let signal_result = self.signal_now();
        tracing::trace!(name = %self.inner.name_prefix, ?signal_result, "doorbell signal");
        signal_result
    }

    /// Check if the peer appears to be dead (signal has failed).
    pub fn is_peer_dead(&self) -> bool {
        self.inner.peer_dead_logged.load(Ordering::Relaxed)
    }

    /// Wait for a signal from the other side.
    ///
    /// Uses `spawn_blocking` + `WaitForMultipleObjects` so the tokio
    /// runtime is never starved — the kernel wait happens on a separate
    /// thread from the blocking pool.
    pub async fn wait(&self) -> io::Result<()> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let handles = [inner.rx_event, inner.cancel_event];
            let result: WAIT_EVENT =
                unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
            match result {
                WAIT_OBJECT_0 => Ok(()),
                v if v == WAIT_OBJECT_0 + 1 => {
                    Err(io::Error::new(ErrorKind::Interrupted, "doorbell cancelled"))
                }
                _ => Err(io::Error::last_os_error()),
            }
        })
        .await
        .unwrap_or_else(|e| Err(io::Error::new(ErrorKind::Other, e)))
    }

    /// Drain any pending signals without blocking.
    ///
    /// With auto-reset events, a zero-timeout wait atomically checks
    /// and consumes any pending signal.
    pub fn drain(&self) {
        unsafe {
            WaitForSingleObject(self.inner.rx_event, 0);
        }
    }

    /// Accept an incoming connection.
    ///
    /// With Win32 Events, this is a no-op — events exist immediately
    /// after creation and don't require a connection handshake.
    /// Kept for API compatibility with the cross-platform Doorbell trait.
    pub async fn accept(&self) -> io::Result<()> {
        Ok(())
    }

    /// Get the event name prefix (for diagnostics).
    pub fn pipe_name(&self) -> &str {
        &self.inner.name_prefix
    }
}

/// Convert a Rust string to a null-terminated wide (UTF-16) string.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Generate a simple UUID-like string for event names.
fn generate_uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let nanos = duration.as_nanos();
    let pid = std::process::id();

    // Combine timestamp and pid for uniqueness
    format!("{:x}-{:x}", nanos, pid)
}

/// Close a handle (Windows equivalent of close_peer_fd).
///
/// # Safety
///
/// handle must be a valid handle that the caller owns.
pub unsafe fn close_handle(handle: HANDLE) {
    if handle != INVALID_HANDLE_VALUE && !handle.is_null() {
        unsafe {
            CloseHandle(handle);
        }
    }
}

/// Validate that a handle is valid.
///
/// On Windows, we check if the handle is not INVALID_HANDLE_VALUE or null.
pub fn validate_handle(handle: HANDLE) -> io::Result<()> {
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid handle",
        ))
    } else {
        Ok(())
    }
}

/// Make a handle inheritable by child processes.
///
/// This is the Windows equivalent of clearing FD_CLOEXEC.
///
/// r[impl shm.spawn.fd-inheritance]
///
/// # Safety
///
/// Handle must be valid
pub unsafe fn set_handle_inheritable(handle: HANDLE) -> io::Result<()> {
    use windows_sys::Win32::Foundation::HANDLE_FLAG_INHERIT;
    use windows_sys::Win32::Foundation::SetHandleInformation;

    let result = unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_connect() {
        let (host_doorbell, guest_handle) = Doorbell::create_pair().unwrap();
        let guest_doorbell = Doorbell::from_handle(guest_handle).unwrap();

        // Test signaling: host → guest
        assert_eq!(host_doorbell.signal_now(), SignalResult::Sent);
        guest_doorbell.wait().await.unwrap();

        // Test signaling: guest → host
        assert_eq!(guest_doorbell.signal_now(), SignalResult::Sent);
        host_doorbell.wait().await.unwrap();
    }

    #[tokio::test]
    async fn test_bidirectional_signal_and_wait() {
        let (host_doorbell, guest_handle) = Doorbell::create_pair().unwrap();
        let guest_doorbell = Doorbell::from_handle(guest_handle).unwrap();

        // Host signals, guest waits
        assert_eq!(host_doorbell.signal_now(), SignalResult::Sent);
        guest_doorbell.wait().await.unwrap();

        // Guest signals, host waits
        assert_eq!(guest_doorbell.signal_now(), SignalResult::Sent);
        host_doorbell.wait().await.unwrap();
    }

    #[tokio::test]
    async fn test_cross_task_signal_and_wait() {
        let (host_doorbell, guest_handle) = Doorbell::create_pair().unwrap();
        let guest_doorbell = std::sync::Arc::new(Doorbell::from_handle(guest_handle).unwrap());
        let host_doorbell = std::sync::Arc::new(host_doorbell);

        // Spawn a task that waits, then signal from the other side
        let hd = host_doorbell.clone();
        let gd = guest_doorbell.clone();
        let waiter = tokio::spawn(async move {
            hd.wait().await.unwrap();
        });
        // Small delay so waiter enters wait() first
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(gd.signal_now(), SignalResult::Sent);
        waiter.await.unwrap();
    }

    #[tokio::test]
    async fn test_signal_coalescing() {
        let (host_doorbell, guest_handle) = Doorbell::create_pair().unwrap();
        let guest_doorbell = Doorbell::from_handle(guest_handle).unwrap();

        // Multiple signals before a wait should coalesce
        assert_eq!(host_doorbell.signal_now(), SignalResult::Sent);
        assert_eq!(host_doorbell.signal_now(), SignalResult::Sent);
        assert_eq!(host_doorbell.signal_now(), SignalResult::Sent);

        // Single wait should consume all
        guest_doorbell.wait().await.unwrap();
    }

    #[tokio::test]
    async fn test_drain() {
        let (host_doorbell, guest_handle) = Doorbell::create_pair().unwrap();
        let guest_doorbell = Doorbell::from_handle(guest_handle).unwrap();

        // Signal then drain
        assert_eq!(host_doorbell.signal_now(), SignalResult::Sent);
        guest_doorbell.drain();

        // After drain, a zero-timeout wait should not find a signal
        let result = unsafe { WaitForSingleObject(guest_doorbell.inner.rx_event, 0) };
        assert_ne!(result, WAIT_OBJECT_0, "signal should have been drained");
    }
}
