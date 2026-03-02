//! Control socket for delivering mmap file descriptors between processes.
//!
//! Uses a Unix domain socketpair (SOCK_SEQPACKET) with SCM_RIGHTS to transfer
//! file descriptors alongside 16-byte metadata messages.
//!
//! r[impl shm.mmap.attach]
//! r[impl shm.mmap.attach.unix]

use std::io::{self, ErrorKind};
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

use super::doorbell::set_nonblocking;

const MMAP_CONTROL_EXPECTED_PAYLOAD_LEN: usize = 16;
const MMAP_CONTROL_SNIFF_BYTES: usize = 256;
const MMAP_CONTROL_DUMP_BYTES: usize = 32;
const MMAP_CONTROL_OVERSIZE_CAPTURE_MAX: usize = 64 * 1024;

/// 16-byte metadata sent alongside each file descriptor.
///
/// r[impl shm.mmap.attach.message]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct MmapAttachMessage {
    pub map_id: u32,
    pub map_generation: u32,
    pub mapping_length: u64,
}

impl MmapAttachMessage {
    pub fn to_le_bytes(self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&self.map_id.to_le_bytes());
        buf[4..8].copy_from_slice(&self.map_generation.to_le_bytes());
        buf[8..16].copy_from_slice(&self.mapping_length.to_le_bytes());
        buf
    }

    pub fn from_le_bytes(buf: [u8; 16]) -> Self {
        Self {
            map_id: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            map_generation: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            mapping_length: u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
        }
    }
}

/// Opaque handle for passing mmap control endpoints between processes.
#[derive(Debug)]
pub struct MmapControlHandle(OwnedFd);

impl MmapControlHandle {
    pub fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }

    /// Consume this handle and return the owned raw fd.
    pub fn into_raw_fd(self) -> RawFd {
        use std::os::unix::io::IntoRawFd;
        self.0.into_raw_fd()
    }

    /// # Safety
    /// The caller must ensure the FD is valid and not owned by anything else.
    pub unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    pub fn to_arg(&self) -> String {
        self.0.as_raw_fd().to_string()
    }

    /// # Safety
    /// The FD must be valid and not owned by anything else.
    pub unsafe fn from_arg(s: &str) -> Result<Self, std::num::ParseIntError> {
        let fd: RawFd = s.parse()?;
        Ok(unsafe { Self::from_raw_fd(fd) })
    }

    pub const ARG_NAME: &'static str = "--mmap-control-fd";
}

/// Sender half of the mmap control socket.
pub struct MmapControlSender {
    fd: OwnedFd,
}

/// Receiver half of the mmap control socket.
pub struct MmapControlReceiver {
    async_fd: AsyncFd<OwnedFd>,
}

fn create_dgram_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0i32; 2];

    // SOCK_DGRAM preserves message boundaries on AF_UNIX (like SEQPACKET)
    // and works on macOS where SEQPACKET is not supported.
    #[cfg(target_os = "linux")]
    let sock_type = libc::SOCK_DGRAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;
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

/// Send one fd + 16-byte metadata using SCM_RIGHTS.
fn send_fd_with_metadata(sock_fd: RawFd, fd: RawFd, msg: &MmapAttachMessage) -> io::Result<()> {
    let payload = msg.to_le_bytes();
    let mut iov = libc::iovec {
        iov_base: payload.as_ptr() as *mut libc::c_void,
        iov_len: payload.len(),
    };

    let fds = [fd];
    let data_len = std::mem::size_of_val(&fds);
    let cmsg_space = unsafe { libc::CMSG_SPACE(data_len as u32) as usize };
    let mut control = vec![0u8; cmsg_space];

    let mut msghdr: libc::msghdr = unsafe { std::mem::zeroed() };
    msghdr.msg_iov = &mut iov;
    msghdr.msg_iovlen = 1;
    msghdr.msg_control = control.as_mut_ptr().cast();
    msghdr.msg_controllen = control.len() as _;

    let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msghdr) };
    if cmsg.is_null() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "failed to build cmsg header",
        ));
    }

    unsafe {
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(data_len as u32) as _;
        let data_ptr = libc::CMSG_DATA(cmsg).cast::<RawFd>();
        std::ptr::copy_nonoverlapping(fds.as_ptr(), data_ptr, 1);
    }

    let n = sendmsg_no_sigpipe(sock_fd, &msghdr).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("mmap control sendmsg failed: {error} {}", fd_telemetry()),
        )
    })?;
    if n == 0 {
        return Err(io::Error::new(
            ErrorKind::WriteZero,
            format!("sendmsg wrote 0 bytes {}", fd_telemetry()),
        ));
    }
    Ok(())
}

fn sendmsg_no_sigpipe(fd: RawFd, msghdr: &libc::msghdr) -> io::Result<isize> {
    #[cfg(target_vendor = "apple")]
    ensure_socket_no_sigpipe(fd)?;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    let flags = libc::MSG_NOSIGNAL;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let flags = 0;

    // SAFETY: caller guarantees `msghdr` points to valid iov/cmsg buffers.
    let n = unsafe { libc::sendmsg(fd, msghdr, flags) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(n)
}

#[cfg(target_vendor = "apple")]
fn ensure_socket_no_sigpipe(fd: RawFd) -> io::Result<()> {
    let one: libc::c_int = 1;
    // SAFETY: setsockopt reads `one` for the provided length.
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            (&one as *const libc::c_int).cast(),
            std::mem::size_of_val(&one) as libc::socklen_t,
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Receive one fd + 16-byte metadata using SCM_RIGHTS.
fn recv_fd_with_metadata(sock_fd: RawFd) -> io::Result<(OwnedFd, MmapAttachMessage)> {
    let mut payload = [0u8; MMAP_CONTROL_SNIFF_BYTES];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };

    let data_len = std::mem::size_of::<RawFd>();
    let cmsg_space = unsafe { libc::CMSG_SPACE(data_len as u32) as usize };
    let mut control = vec![0u8; cmsg_space];

    let mut msghdr: libc::msghdr = unsafe { std::mem::zeroed() };
    msghdr.msg_iov = &mut iov;
    msghdr.msg_iovlen = 1;
    msghdr.msg_control = control.as_mut_ptr().cast();
    msghdr.msg_controllen = control.len() as _;

    let n = unsafe { libc::recvmsg(sock_fd, &mut msghdr, libc::MSG_TRUNC) };
    if n < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EMSGSIZE) {
            let diagnostic = diagnose_oversized_mmap_control_datagram(sock_fd);
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "invalid mmap control payload length: recvmsg returned EMSGSIZE (expected={}) {diagnostic} {}",
                    MMAP_CONTROL_EXPECTED_PAYLOAD_LEN,
                    fd_telemetry(),
                ),
            ));
        }
        return Err(io::Error::new(
            error.kind(),
            format!("mmap control recvmsg failed: {error} {}", fd_telemetry()),
        ));
    }
    if n == 0 {
        return Err(io::Error::new(
            ErrorKind::UnexpectedEof,
            format!("peer closed {}", fd_telemetry()),
        ));
    }
    if (n as usize) < MMAP_CONTROL_EXPECTED_PAYLOAD_LEN {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "short mmap control payload: got={} expected={} payload_prefix={} {}",
                n,
                MMAP_CONTROL_EXPECTED_PAYLOAD_LEN,
                hex_prefix(&payload, n as usize),
                fd_telemetry(),
            ),
        ));
    }
    // r[impl shm.mmap.attach.protocol-error]
    if (n as usize) != MMAP_CONTROL_EXPECTED_PAYLOAD_LEN {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "invalid mmap control payload length: got={} expected={} msg_flags=0x{:x} payload_prefix={} {}",
                n,
                MMAP_CONTROL_EXPECTED_PAYLOAD_LEN,
                msghdr.msg_flags,
                hex_prefix(&payload, n as usize),
                fd_telemetry(),
            ),
        ));
    }
    // r[impl shm.mmap.attach.protocol-error]
    if (msghdr.msg_flags & libc::MSG_CTRUNC) != 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "mmap control cmsg truncated: msg_flags=0x{:x} payload_prefix={} {}",
                msghdr.msg_flags,
                hex_prefix(&payload, n as usize),
                fd_telemetry(),
            ),
        ));
    }

    let mut received_fd: Option<RawFd> = None;
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msghdr);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let cmsg_len = (*cmsg).cmsg_len as usize;
                let base_len = libc::CMSG_LEN(0) as usize;
                if cmsg_len >= base_len + std::mem::size_of::<RawFd>() {
                    let data_ptr = libc::CMSG_DATA(cmsg).cast::<RawFd>();
                    received_fd = Some(*data_ptr);
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msghdr, cmsg);
        }
    }

    // r[impl shm.mmap.attach.protocol-error]
    let raw_fd = received_fd.ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "no fd received in mmap control message: msg_flags=0x{:x} payload_prefix={} {}",
                msghdr.msg_flags,
                hex_prefix(&payload, n as usize),
                fd_telemetry(),
            ),
        )
    })?;

    let owned_fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    let msg = MmapAttachMessage::from_le_bytes(
        payload[..MMAP_CONTROL_EXPECTED_PAYLOAD_LEN]
            .try_into()
            .expect("slice length checked above"),
    );
    Ok((owned_fd, msg))
}

fn diagnose_oversized_mmap_control_datagram(sock_fd: RawFd) -> String {
    let capture_cap = pending_datagram_len(sock_fd)
        .ok()
        .filter(|len| *len > 0)
        .unwrap_or(MMAP_CONTROL_OVERSIZE_CAPTURE_MAX)
        .min(MMAP_CONTROL_OVERSIZE_CAPTURE_MAX);

    let mut payload = vec![0u8; capture_cap.max(1)];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };

    let data_len = std::mem::size_of::<RawFd>();
    let cmsg_space = unsafe { libc::CMSG_SPACE(data_len as u32) as usize };
    let mut control = vec![0u8; cmsg_space];

    let mut msghdr: libc::msghdr = unsafe { std::mem::zeroed() };
    msghdr.msg_iov = &mut iov;
    msghdr.msg_iovlen = 1;
    msghdr.msg_control = control.as_mut_ptr().cast();
    msghdr.msg_controllen = control.len() as _;

    let n = unsafe { libc::recvmsg(sock_fd, &mut msghdr, libc::MSG_DONTWAIT | libc::MSG_TRUNC) };
    if n < 0 {
        return format!(
            "drain_diag=unavailable error={}",
            io::Error::last_os_error()
        );
    }

    let observed = n as usize;
    let dump = hex_prefix(&payload, observed);
    format!(
        "observed={} capture_cap={} msg_flags=0x{:x} payload_prefix={}",
        observed, capture_cap, msghdr.msg_flags, dump
    )
}

fn pending_datagram_len(sock_fd: RawFd) -> io::Result<usize> {
    let mut pending: libc::c_int = 0;
    let rc = unsafe { libc::ioctl(sock_fd, libc::FIONREAD, &mut pending) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pending as usize)
}

fn hex_prefix(bytes: &[u8], observed_len: usize) -> String {
    let take = observed_len.min(bytes.len()).min(MMAP_CONTROL_DUMP_BYTES);
    let mut out = String::with_capacity(take.saturating_mul(3) + 16);
    for (idx, byte) in bytes.iter().take(take).enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    if observed_len > take {
        out.push_str(&format!(" ... (+{} bytes)", observed_len - take));
    }
    out
}

fn fd_telemetry() -> String {
    let open = open_fd_count()
        .map(|count| count.to_string())
        .unwrap_or_else(|error| format!("error:{error}"));
    let (soft, hard) = nofile_limits();
    format!("fd_usage(open={open}, soft_limit={soft}, hard_limit={hard})")
}

fn open_fd_count() -> io::Result<usize> {
    #[cfg(target_os = "linux")]
    let path = "/proc/self/fd";
    #[cfg(not(target_os = "linux"))]
    let path = "/dev/fd";
    Ok(std::fs::read_dir(path)?.count())
}

fn nofile_limits() -> (u64, u64) {
    let mut limits: libc::rlimit = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) };
    if rc != 0 {
        return (0, 0);
    }
    #[allow(clippy::unnecessary_cast)]
    (limits.rlim_cur as u64, limits.rlim_max as u64)
}

impl MmapControlSender {
    /// Expose the sender fd for process-spawn handoff plumbing.
    pub fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    /// Consume this sender and return the owned raw fd.
    pub fn into_raw_fd(self) -> RawFd {
        use std::os::unix::io::IntoRawFd;
        self.fd.into_raw_fd()
    }

    /// Send a file descriptor with metadata to the receiver.
    pub fn send(&self, fd: RawFd, msg: &MmapAttachMessage) -> io::Result<()> {
        send_fd_with_metadata(self.fd.as_raw_fd(), fd, msg)
    }
}

impl MmapControlReceiver {
    /// Non-blocking receive of one fd + metadata.
    pub fn try_recv(&self) -> io::Result<Option<(OwnedFd, MmapAttachMessage)>> {
        match recv_fd_with_metadata(self.async_fd.get_ref().as_raw_fd()) {
            Ok(pair) => Ok(Some(pair)),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Async receive — waits for readiness then receives.
    pub async fn recv(&self) -> io::Result<(OwnedFd, MmapAttachMessage)> {
        loop {
            let mut guard = self.async_fd.ready(Interest::READABLE).await?;
            match guard.try_io(|inner| recv_fd_with_metadata(inner.get_ref().as_raw_fd())) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }
}

/// Create a paired mmap control channel.
///
/// Returns `(sender, receiver_handle)`. The receiver handle should be passed
/// to the peer process and reconstructed with `MmapControlReceiver::from_handle`.
pub fn create_mmap_control_pair() -> io::Result<(MmapControlSender, MmapControlHandle)> {
    let (sender_fd, receiver_fd) = create_dgram_pair()?;
    Ok((
        MmapControlSender { fd: sender_fd },
        MmapControlHandle(receiver_fd),
    ))
}

impl MmapControlReceiver {
    /// Reconstruct a receiver from a handle (in the peer process).
    pub fn from_handle(handle: MmapControlHandle) -> io::Result<Self> {
        let fd = handle.0;
        set_nonblocking(fd.as_raw_fd())?;
        let async_fd = AsyncFd::new(fd)?;
        Ok(Self { async_fd })
    }

    /// Create directly from an owned fd.
    ///
    /// # Safety
    /// The fd must be valid and from a socketpair.
    pub unsafe fn from_raw_fd(fd: RawFd) -> io::Result<Self> {
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        set_nonblocking(fd)?;
        let async_fd = AsyncFd::new(owned)?;
        Ok(Self { async_fd })
    }
}

impl MmapControlSender {
    /// Create directly from an owned fd.
    ///
    /// # Safety
    /// The fd must be valid and from a socketpair.
    pub unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        }
    }
}

/// Create a fully connected in-process pair (both sides ready to use).
pub fn create_mmap_control_pair_connected() -> io::Result<(MmapControlSender, MmapControlReceiver)>
{
    let (sender_fd, receiver_fd) = create_dgram_pair()?;
    set_nonblocking(receiver_fd.as_raw_fd())?;
    let async_fd = AsyncFd::new(receiver_fd)?;
    Ok((
        MmapControlSender { fd: sender_fd },
        MmapControlReceiver { async_fd },
    ))
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use std::os::unix::io::AsRawFd;

    fn send_raw_payload_without_fd(sock_fd: RawFd, payload: &[u8]) -> io::Result<()> {
        let n = unsafe {
            libc::send(
                sock_fd,
                payload.as_ptr().cast::<libc::c_void>(),
                payload.len(),
                0,
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n as usize != payload.len() {
            return Err(io::Error::new(
                ErrorKind::WriteZero,
                "short write while sending raw payload",
            ));
        }
        Ok(())
    }

    fn send_two_fds_with_metadata(sock_fd: RawFd, fd_a: RawFd, fd_b: RawFd) -> io::Result<()> {
        let payload = MmapAttachMessage {
            map_id: 7,
            map_generation: 3,
            mapping_length: 1234,
        }
        .to_le_bytes();

        let mut iov = libc::iovec {
            iov_base: payload.as_ptr() as *mut libc::c_void,
            iov_len: payload.len(),
        };

        let fds = [fd_a, fd_b];
        let data_len = std::mem::size_of_val(&fds);
        let cmsg_space = unsafe { libc::CMSG_SPACE(data_len as u32) as usize };
        let mut control = vec![0u8; cmsg_space];

        let mut msghdr: libc::msghdr = unsafe { std::mem::zeroed() };
        msghdr.msg_iov = &mut iov;
        msghdr.msg_iovlen = 1;
        msghdr.msg_control = control.as_mut_ptr().cast();
        msghdr.msg_controllen = control.len() as _;

        let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msghdr) };
        if cmsg.is_null() {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "failed to build cmsg header for test",
            ));
        }

        unsafe {
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(data_len as u32) as _;
            let data_ptr = libc::CMSG_DATA(cmsg).cast::<RawFd>();
            std::ptr::copy_nonoverlapping(fds.as_ptr(), data_ptr, fds.len());
        }

        let n = sendmsg_no_sigpipe(sock_fd, &msghdr)?;
        if n == 0 {
            return Err(io::Error::new(
                ErrorKind::WriteZero,
                "sendmsg wrote zero bytes",
            ));
        }
        Ok(())
    }

    #[tokio::test]
    async fn roundtrip_fd_with_metadata() {
        let (sender, receiver_handle) = create_mmap_control_pair().unwrap();
        let receiver = MmapControlReceiver::from_handle(receiver_handle).unwrap();

        // Create a temp file to use as the fd
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let fd = tmp.as_file().as_raw_fd();

        let msg = MmapAttachMessage {
            map_id: 42,
            map_generation: 7,
            mapping_length: 65536,
        };

        sender.send(fd, &msg).unwrap();

        let (received_fd, received_msg) = receiver.recv().await.unwrap();
        assert_eq!(received_msg, msg);

        // Verify the received fd is valid and different from the original
        assert_ne!(received_fd.as_raw_fd(), fd);
        // Verify it's actually usable (can fstat it)
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::fstat(received_fd.as_raw_fd(), &mut stat) };
        assert_eq!(ret, 0);
    }

    #[test]
    fn attach_message_roundtrip() {
        let msg = MmapAttachMessage {
            map_id: 0xDEAD_BEEF,
            map_generation: 0xCAFE_BABE,
            mapping_length: 0x1234_5678_9ABC_DEF0,
        };
        let bytes = msg.to_le_bytes();
        let decoded = MmapAttachMessage::from_le_bytes(bytes);
        assert_eq!(msg, decoded);
    }

    #[tokio::test]
    async fn try_recv_returns_none_when_empty() {
        let (_sender, handle) = create_mmap_control_pair().unwrap();
        let receiver = MmapControlReceiver::from_handle(handle).unwrap();
        assert!(receiver.try_recv().unwrap().is_none());
    }

    #[test]
    // r[verify shm.mmap.attach.protocol-error]
    fn recv_rejects_payload_without_fd_control_message() {
        let (sender_fd, receiver_fd) = create_dgram_pair().expect("create pair");
        let payload = MmapAttachMessage {
            map_id: 1,
            map_generation: 2,
            mapping_length: 3,
        }
        .to_le_bytes();

        send_raw_payload_without_fd(sender_fd.as_raw_fd(), &payload)
            .expect("send payload without fd should succeed");

        let err = recv_fd_with_metadata(receiver_fd.as_raw_fd())
            .expect_err("recv should fail when SCM_RIGHTS is missing");
        assert_eq!(
            err.kind(),
            ErrorKind::InvalidData,
            "expected InvalidData for missing fd control message, got {err:?}"
        );
        assert!(
            err.to_string().contains("payload_prefix="),
            "expected diagnostic payload dump in error, got {err:?}"
        );
    }

    #[test]
    // r[verify shm.mmap.attach.protocol-error]
    fn recv_rejects_truncated_control_message() {
        let (sender_fd, receiver_fd) = create_dgram_pair().expect("create pair");
        let tmp_a = tempfile::NamedTempFile::new().expect("tmp file a");
        let tmp_b = tempfile::NamedTempFile::new().expect("tmp file b");

        send_two_fds_with_metadata(
            sender_fd.as_raw_fd(),
            tmp_a.as_file().as_raw_fd(),
            tmp_b.as_file().as_raw_fd(),
        )
        .expect("send two-fd metadata message");

        let err = recv_fd_with_metadata(receiver_fd.as_raw_fd())
            .expect_err("receiver should reject truncated SCM_RIGHTS");
        assert_eq!(
            err.kind(),
            ErrorKind::InvalidData,
            "expected InvalidData for control truncation, got {err:?}"
        );
        assert!(
            err.to_string().contains("truncated"),
            "expected truncation error message, got {err:?}"
        );
    }

    #[tokio::test]
    async fn receiver_recovers_after_malformed_packet() {
        let (sender, receiver) =
            create_mmap_control_pair_connected().expect("create connected mmap control pair");

        let malformed = MmapAttachMessage {
            map_id: 11,
            map_generation: 22,
            mapping_length: 33,
        }
        .to_le_bytes();
        send_raw_payload_without_fd(sender.as_raw_fd(), &malformed)
            .expect("send malformed packet without fd");

        let err = receiver
            .recv()
            .await
            .expect_err("first malformed packet should fail");
        assert_eq!(err.kind(), ErrorKind::InvalidData);

        let tmp = tempfile::NamedTempFile::new().expect("temp file");
        let good = MmapAttachMessage {
            map_id: 42,
            map_generation: 7,
            mapping_length: 4096,
        };
        sender
            .send(tmp.as_file().as_raw_fd(), &good)
            .expect("send valid packet after malformed");

        let (_fd, msg) = receiver
            .recv()
            .await
            .expect("receiver should recover and accept valid packet");
        assert_eq!(msg, good);
    }

    #[test]
    // r[verify shm.mmap.attach.protocol-error]
    fn recv_rejects_oversized_payload_with_diagnostics() {
        let (sender_fd, receiver_fd) = create_dgram_pair().expect("create pair");
        let oversized = [0xAAu8; 96];
        send_raw_payload_without_fd(sender_fd.as_raw_fd(), &oversized)
            .expect("send oversized malformed packet");

        let err = recv_fd_with_metadata(receiver_fd.as_raw_fd())
            .expect_err("receiver should reject oversized payload");
        assert_eq!(err.kind(), ErrorKind::InvalidData);
        let msg = err.to_string();
        assert!(
            msg.contains("invalid mmap control payload length"),
            "expected length diagnostic in error, got {msg}"
        );
        assert!(
            msg.contains("payload_prefix="),
            "expected payload prefix dump in error, got {msg}"
        );
    }
}
