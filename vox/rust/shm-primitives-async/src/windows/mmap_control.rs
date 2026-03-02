//! Named-pipe mmap control channel for Windows.
//!
//! On Unix the mmap control channel passes file descriptors via SCM_RIGHTS.
//! Windows has no fd-passing primitive, so we send the *path* of each mmap
//! region alongside the 16-byte [`MmapAttachMessage`] metadata.  The receiver
//! opens the region with `MmapRegion::attach(path)`.
//!
//! Wire format per message (little-endian):
//! ```text
//! [u16 path_len] [path_bytes …] [16-byte MmapAttachMessage]
//! ```
//!
//! r[impl shm.mmap.attach]
//! r[impl shm.mmap.attach.windows]

use std::io::{self, ErrorKind};
use std::os::windows::io::RawHandle;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
    PeekNamedPipe,
};

use super::MmapAttachMessage;

const PIPE_BUFFER_SIZE: u32 = 64 * 1024;

/// Opaque handle for passing mmap control endpoints between processes.
///
/// On Windows this wraps a named pipe name string (no fd involved).
#[derive(Debug)]
pub struct MmapControlHandle(String);

impl MmapControlHandle {
    /// Get the pipe name.
    pub fn pipe_name(&self) -> &str {
        &self.0
    }

    /// Format as a command-line argument value.
    pub fn to_arg(&self) -> String {
        self.0.clone()
    }

    /// Parse from a command-line argument value.
    ///
    /// # Safety
    /// The pipe name must refer to a valid mmap control pipe.
    pub unsafe fn from_arg(s: &str) -> Result<Self, std::convert::Infallible> {
        Ok(Self(s.to_string()))
    }

    /// The CLI argument name for this platform.
    pub const ARG_NAME: &'static str = "--mmap-control-pipe";
}

/// Sender half of the mmap control channel (host side).
///
/// Wraps a named pipe server handle.  On the first `send_path` call,
/// `ConnectNamedPipe` is called to transition the server handle into the
/// connected state.  If the client already connected (via `CreateFileW`),
/// `ConnectNamedPipe` returns `ERROR_PIPE_CONNECTED` which is handled as
/// success.
pub struct MmapControlSender {
    handle: Mutex<Option<RawHandle>>,
    connected: AtomicBool,
}

// SAFETY: The RawHandle is used exclusively behind a Mutex; only one thread
// writes at a time.
unsafe impl Send for MmapControlSender {}
unsafe impl Sync for MmapControlSender {}

impl MmapControlSender {
    /// Connect to an existing pipe server as a client-side sender.
    ///
    /// The server should have been created with
    /// [`create_mmap_control_receiver_server`].  This is the cross-process
    /// counterpart: the remote process passes the pipe name and the local
    /// process opens it for writing.
    pub fn connect(pipe_name: &str) -> io::Result<Self> {
        let wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();
        let h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut() as _,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: Mutex::new(Some(h as RawHandle)),
            connected: AtomicBool::new(true), // already connected as client
        })
    }

    /// Send an mmap region's path + metadata to the receiver.
    pub fn send_path(&self, path: &Path, msg: &MmapAttachMessage) -> io::Result<()> {
        let guard = self.handle.lock().unwrap();
        let &raw_handle = guard
            .as_ref()
            .ok_or_else(|| io::Error::new(ErrorKind::BrokenPipe, "mmap control pipe closed"))?;

        // Lazy connect: the first send transitions the server handle into the
        // connected state.  If the client already called CreateFileW,
        // ConnectNamedPipe returns FALSE with ERROR_PIPE_CONNECTED — that's OK.
        if !self.connected.swap(true, Ordering::Relaxed) {
            let ok = unsafe { ConnectNamedPipe(raw_handle as _, std::ptr::null_mut()) };
            if ok == 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32) {
                    self.connected.store(false, Ordering::Relaxed);
                    return Err(err);
                }
            }
        }

        write_message(raw_handle, path, msg)
    }
}

impl Drop for MmapControlSender {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.get_mut().unwrap().take() {
            unsafe { CloseHandle(handle as _) };
        }
    }
}

/// Receiver half of the mmap control channel.
///
/// May hold either a pipe client handle (when constructed via
/// [`from_handle`](Self::from_handle)) or a pipe server handle (when
/// constructed via [`create_mmap_control_receiver_server`]).
pub struct MmapControlReceiver {
    handle: RawHandle,
}

// SAFETY: handle is used exclusively by this struct.
unsafe impl Send for MmapControlReceiver {}
unsafe impl Sync for MmapControlReceiver {}

impl MmapControlReceiver {
    /// Reconstruct a receiver from a handle (in the peer process).
    ///
    /// Connects as a pipe client.
    pub fn from_handle(handle: MmapControlHandle) -> io::Result<Self> {
        let pipe_name = handle.0;
        let wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();
        let h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut() as _,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: h as RawHandle,
        })
    }

    /// Non-blocking receive of one (path, metadata) pair.
    ///
    /// For server-side handles (from [`create_mmap_control_receiver_server`]),
    /// no explicit `ConnectNamedPipe` is needed: `PeekNamedPipe` returns an
    /// error (treated as "no data yet") until the client connects, then works
    /// normally once connected.
    pub fn try_recv_path(&self) -> io::Result<Option<(PathBuf, MmapAttachMessage)>> {
        // Check if data is available without blocking.
        let mut bytes_available: u32 = 0;
        let ok = unsafe {
            PeekNamedPipe(
                self.handle as _,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut bytes_available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let err = io::Error::last_os_error();
            if err.kind() == ErrorKind::BrokenPipe {
                return Err(err);
            }
            return Ok(None);
        }
        // Need at least 2 bytes (path_len header) to start reading.
        if bytes_available < 2 {
            return Ok(None);
        }
        read_message(self.handle).map(Some)
    }
}

impl Drop for MmapControlReceiver {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle as _) };
    }
}

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

fn write_message(handle: RawHandle, path: &Path, msg: &MmapAttachMessage) -> io::Result<()> {
    let path_bytes = path
        .to_str()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "path is not valid UTF-8"))?
        .as_bytes();
    let path_len = u16::try_from(path_bytes.len())
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "path too long for u16"))?;

    let metadata = msg.to_le_bytes();

    // Build a contiguous buffer: [u16 path_len][path bytes][16-byte metadata]
    let total = 2 + path_bytes.len() + 16;
    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(&path_len.to_le_bytes());
    buf.extend_from_slice(path_bytes);
    buf.extend_from_slice(&metadata);

    write_all_handle(handle, &buf)?;
    // Note: do NOT call FlushFileBuffers here.  On a named pipe server,
    // FlushFileBuffers blocks until the client reads all buffered data,
    // which would deadlock single-threaded callers.  WriteFile on a
    // synchronous byte-mode pipe already commits data to the pipe buffer.
    Ok(())
}

fn read_message(handle: RawHandle) -> io::Result<(PathBuf, MmapAttachMessage)> {
    // Read path length.
    let mut len_buf = [0u8; 2];
    read_exact_handle(handle, &mut len_buf)?;
    let path_len = u16::from_le_bytes(len_buf) as usize;

    // Read path bytes.
    let mut path_buf = vec![0u8; path_len];
    read_exact_handle(handle, &mut path_buf)?;

    // Read 16-byte metadata.
    let mut meta_buf = [0u8; 16];
    read_exact_handle(handle, &mut meta_buf)?;

    let path_str =
        String::from_utf8(path_buf).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
    let msg = MmapAttachMessage::from_le_bytes(meta_buf);
    Ok((PathBuf::from(path_str), msg))
}

fn write_all_handle(handle: RawHandle, mut buf: &[u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let mut written: u32 = 0;
        let ok = unsafe {
            WriteFile(
                handle as _,
                buf.as_ptr(),
                buf.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        buf = &buf[written as usize..];
    }
    Ok(())
}

fn read_exact_handle(handle: RawHandle, mut buf: &mut [u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let mut read: u32 = 0;
        let ok = unsafe {
            ReadFile(
                handle as _,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if read == 0 {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "pipe closed"));
        }
        buf = &mut buf[read as usize..];
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Factory functions
// ---------------------------------------------------------------------------

/// Generate a unique pipe name for mmap control.
fn unique_pipe_name() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!(r"\\.\pipe\roam-mmap-{:x}-{:x}-{}", nanos, pid, seq)
}

/// Create a paired mmap control channel.
///
/// Returns `(sender, receiver_handle)`.  The receiver handle should be
/// passed to the guest process and reconstructed with
/// `MmapControlReceiver::from_handle`.
pub fn create_mmap_control_pair() -> io::Result<(MmapControlSender, MmapControlHandle)> {
    let pipe_name = unique_pipe_name();
    let wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

    let h = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1, // max instances
            PIPE_BUFFER_SIZE,
            PIPE_BUFFER_SIZE,
            0, // default timeout
            std::ptr::null(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let raw = h as RawHandle;

    let sender = MmapControlSender {
        handle: Mutex::new(Some(raw)),
        connected: AtomicBool::new(false),
    };
    Ok((sender, MmapControlHandle(pipe_name)))
}

/// Create a fully connected in-process pair (both sides ready to use).
///
/// This is for tests that run host + guest in the same process.
pub fn create_mmap_control_pair_connected() -> io::Result<(MmapControlSender, MmapControlReceiver)>
{
    let (sender, handle) = create_mmap_control_pair()?;
    let receiver = MmapControlReceiver::from_handle(handle)?;
    Ok((sender, receiver))
}

/// Create a pipe server intended for the **receiver** (reader) side.
///
/// Returns `(receiver, pipe_name)`.  The pipe name should be passed to the
/// remote process which calls [`MmapControlSender::connect`] to open the
/// client (writer) end.
///
/// This is the inverse of [`create_mmap_control_pair`]: the server handle
/// lives with the *reader* instead of the writer, which is needed for the
/// guest→host mmap direction in cross-process setups (the host keeps the
/// server handle).
pub fn create_mmap_control_receiver_server() -> io::Result<(MmapControlReceiver, String)> {
    let pipe_name = unique_pipe_name();
    let wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

    let h = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1, // max instances
            PIPE_BUFFER_SIZE,
            PIPE_BUFFER_SIZE,
            0, // default timeout
            std::ptr::null(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok((
        MmapControlReceiver {
            handle: h as RawHandle,
        },
        pipe_name,
    ))
}

/// No-op on Windows (Unix `clear_cloexec` equivalent).
pub fn clear_cloexec(_: ()) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn create_and_roundtrip() {
        let (sender, handle) = create_mmap_control_pair().unwrap();
        let receiver = MmapControlReceiver::from_handle(handle).unwrap();

        let path = Path::new(r"C:\tmp\test-region.shm");
        let msg = MmapAttachMessage {
            map_id: 42,
            map_generation: 7,
            mapping_length: 65536,
        };

        sender.send_path(path, &msg).unwrap();

        let (recv_path, recv_msg) = receiver.try_recv_path().unwrap().unwrap();
        assert_eq!(recv_path, path);
        assert_eq!(recv_msg, msg);
    }

    #[test]
    fn try_recv_returns_none_when_empty() {
        let (sender, receiver) = create_mmap_control_pair_connected().unwrap();
        assert!(receiver.try_recv_path().unwrap().is_none());
        drop(sender); // keep sender alive until here
    }
}
