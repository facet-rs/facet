//! Named pipe doorbell for cross-process wakeup (Windows).
//!
//! Uses Windows named pipes for bidirectional signaling between processes
//! sharing memory. The host creates a named pipe server, and guests connect
//! as clients using the pipe name.

use std::io::{self, ErrorKind};
use std::string::String;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::Interest;
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};

use std::format;
use std::string::ToString;

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

/// A doorbell for cross-process wakeup.
///
/// On Windows, uses named pipes for bidirectional signaling.
/// The host creates a server pipe, and guests connect as clients.
pub struct Doorbell {
    /// The pipe (either server or client side)
    pipe: DoorbellPipe,
    /// The pipe name (for diagnostics and reconnection)
    pipe_name: String,
    /// Whether we've already logged that the peer is dead (to avoid spam).
    peer_dead_logged: AtomicBool,
}

enum DoorbellPipe {
    Server(NamedPipeServer),
    Client(NamedPipeClient),
}

impl Doorbell {
    /// Create a named pipe server and return (host_doorbell, pipe_name).
    ///
    /// The pipe_name should be passed to the plugin (e.g., via --doorbell-pipe=NAME).
    /// The host keeps the Doorbell.
    pub fn create_pair() -> io::Result<(Self, String)> {
        // Generate unique pipe name
        let uuid = generate_uuid();
        let pipe_name = format!(r"\\.\pipe\roam-shm-{}", uuid);

        // Create server pipe
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&pipe_name)?;

        Ok((
            Self {
                pipe: DoorbellPipe::Server(server),
                pipe_name: pipe_name.clone(),
                peer_dead_logged: AtomicBool::new(false),
            },
            pipe_name,
        ))
    }

    /// Connect to an existing named pipe as a client.
    ///
    /// This is for spawned guest processes that receive the pipe name.
    pub fn connect(pipe_name: &str) -> io::Result<Self> {
        let client = ClientOptions::new().open(pipe_name)?;

        Ok(Self {
            pipe: DoorbellPipe::Client(client),
            pipe_name: pipe_name.to_string(),
            peer_dead_logged: AtomicBool::new(false),
        })
    }

    /// Signal the other side.
    ///
    /// Sends a 1-byte message. If the pipe buffer is full (EAGAIN),
    /// the signal is dropped (the other side is already signaled).
    ///
    /// Returns `SignalResult::PeerDead` if the peer has disconnected.
    pub fn signal(&self) -> SignalResult {
        let buf = [1u8];

        // Use try_write for non-blocking send
        let result = match &self.pipe {
            DoorbellPipe::Server(server) => server.try_write(&buf),
            DoorbellPipe::Client(client) => client.try_write(&buf),
        };

        match result {
            Ok(1) => SignalResult::Sent,
            Ok(0) => SignalResult::Sent, // Shouldn't happen, treat as success
            Ok(_) => SignalResult::Sent,
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => SignalResult::BufferFull,
            Err(ref e) if e.kind() == ErrorKind::BrokenPipe => SignalResult::PeerDead,
            Err(ref e) if e.kind() == ErrorKind::NotConnected => SignalResult::PeerDead,
            Err(ref e) if e.raw_os_error() == Some(232) => SignalResult::PeerDead, // ERROR_NO_DATA
            Err(ref e) if e.raw_os_error() == Some(233) => SignalResult::PeerDead, // ERROR_PIPE_NOT_CONNECTED
            Err(e) => {
                // Some other error - also indicates peer is dead, but log it once
                if !self.peer_dead_logged.swap(true, Ordering::Relaxed) {
                    tracing::debug!(pipe = %self.pipe_name, error = %e, "doorbell signal failed (peer likely dead)");
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
            // Wait for readability
            let ready = match &self.pipe {
                DoorbellPipe::Server(server) => server.ready(Interest::READABLE).await?,
                DoorbellPipe::Client(client) => client.ready(Interest::READABLE).await?,
            };

            if ready.is_readable() {
                match self.try_drain_inner(true) {
                    Ok(true) => return Ok(()),
                    Ok(false) => continue, // spurious wakeup
                    Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e),
                }
            }
        }
    }

    fn try_drain(&self) -> bool {
        match self.try_drain_inner(false) {
            Ok(drained) => drained,
            Err(err) => {
                tracing::warn!(pipe = %self.pipe_name, error = %err, "doorbell drain failed");
                false
            }
        }
    }

    fn try_drain_inner(&self, would_block_is_error: bool) -> io::Result<bool> {
        let mut buf = [0u8; 64];
        let mut drained = false;

        loop {
            let result = match &self.pipe {
                DoorbellPipe::Server(server) => server.try_read(&mut buf),
                DoorbellPipe::Client(client) => client.try_read(&mut buf),
            };

            match result {
                Ok(0) => return Ok(drained), // EOF
                Ok(_) => {
                    drained = true;
                    continue;
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    if drained {
                        return Ok(true);
                    }
                    return if would_block_is_error {
                        Err(io::Error::from(ErrorKind::WouldBlock))
                    } else {
                        Ok(false)
                    };
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Drain any pending signals without blocking.
    pub fn drain(&self) {
        self.try_drain();
    }

    /// Get the pipe name (for diagnostics).
    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }
}

/// Generate a simple UUID-like string for pipe names.
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
pub fn close_handle(handle: HANDLE) {
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
/// shm[impl shm.spawn.fd-inheritance]
pub fn set_handle_inheritable(handle: HANDLE) -> io::Result<()> {
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
        let (host_doorbell, pipe_name) = Doorbell::create_pair().unwrap();

        // Connect in a separate task since server needs to accept
        let connect_handle = tokio::spawn(async move {
            // Small delay to ensure server is ready
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Doorbell::connect(&pipe_name)
        });

        // Server needs to wait for connection
        if let DoorbellPipe::Server(ref server) = host_doorbell.pipe {
            server.connect().await.unwrap();
        }

        let guest_doorbell = connect_handle.await.unwrap().unwrap();

        // Test signaling
        assert_eq!(host_doorbell.signal(), SignalResult::Sent);

        // Give some time for the signal to propagate
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        guest_doorbell.drain();
    }
}
