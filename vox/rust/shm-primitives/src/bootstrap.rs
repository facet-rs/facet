//! SHM bootstrap wire primitives.
//!
//! r[impl shm.bootstrap]
//! r[impl shm.bootstrap.request]
//! r[impl shm.bootstrap.response]
//! r[impl shm.bootstrap.status]
//! r[impl shm.bootstrap.sid]

use std::fmt;
use std::io;

pub const BOOTSTRAP_REQUEST_MAGIC: [u8; 4] = *b"RSH0";
pub const BOOTSTRAP_RESPONSE_MAGIC: [u8; 4] = *b"RSP0";

pub const BOOTSTRAP_REQUEST_HEADER_LEN: usize = 6;
pub const BOOTSTRAP_RESPONSE_HEADER_LEN: usize = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BootstrapStatus {
    Success = 0,
    Error = 1,
}

impl TryFrom<u8> for BootstrapStatus {
    type Error = BootstrapError;

    fn try_from(value: u8) -> Result<Self, BootstrapError> {
        match value {
            0 => Ok(Self::Success),
            1 => Ok(Self::Error),
            _ => Err(BootstrapError::UnknownStatus(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootstrapRequestRef<'a> {
    pub sid: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootstrapResponseRef<'a> {
    pub status: BootstrapStatus,
    pub peer_id: u32,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapResponseOwned {
    pub status: BootstrapStatus,
    pub peer_id: u32,
    pub payload: Vec<u8>,
}

#[derive(Debug)]
pub enum BootstrapError {
    Truncated(&'static str),
    InvalidMagic {
        context: &'static str,
        expected: [u8; 4],
        found: [u8; 4],
    },
    LengthMismatch {
        context: &'static str,
        declared: usize,
        actual: usize,
    },
    SidTooLong(usize),
    PayloadTooLong(usize),
    UnknownStatus(u8),
    InvalidErrorPeerId(u32),
    Io(io::Error),
    #[cfg(unix)]
    InvalidFdCount(usize),
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootstrapError::Truncated(context) => write!(f, "truncated {context} frame"),
            BootstrapError::InvalidMagic {
                context,
                expected,
                found,
            } => write!(
                f,
                "invalid {context} magic: expected {:?}, found {:?}",
                expected, found
            ),
            BootstrapError::LengthMismatch {
                context,
                declared,
                actual,
            } => write!(
                f,
                "{context} length mismatch: declared {declared}, actual {actual}"
            ),
            BootstrapError::SidTooLong(len) => write!(f, "sid too long for u16 length: {len}"),
            BootstrapError::PayloadTooLong(len) => {
                write!(f, "payload too long for u16 length: {len}")
            }
            BootstrapError::UnknownStatus(status) => {
                write!(f, "unknown bootstrap status: {status}")
            }
            BootstrapError::InvalidErrorPeerId(peer_id) => {
                write!(f, "error response must have peer_id=0 (got {peer_id})")
            }
            BootstrapError::Io(err) => write!(f, "io error: {err}"),
            #[cfg(unix)]
            BootstrapError::InvalidFdCount(count) => {
                write!(
                    f,
                    "invalid bootstrap fd count: {count} (expected 3 on success, 0 on error)"
                )
            }
        }
    }
}

impl std::error::Error for BootstrapError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BootstrapError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for BootstrapError {
    fn from(value: io::Error) -> Self {
        BootstrapError::Io(value)
    }
}

pub fn encode_request(sid: &[u8]) -> Result<Vec<u8>, BootstrapError> {
    let sid_len_u16 =
        u16::try_from(sid.len()).map_err(|_| BootstrapError::SidTooLong(sid.len()))?;
    let mut out = Vec::with_capacity(BOOTSTRAP_REQUEST_HEADER_LEN + sid.len());
    out.extend_from_slice(&BOOTSTRAP_REQUEST_MAGIC);
    out.extend_from_slice(&sid_len_u16.to_le_bytes());
    out.extend_from_slice(sid);
    Ok(out)
}

pub fn decode_request(frame: &[u8]) -> Result<BootstrapRequestRef<'_>, BootstrapError> {
    if frame.len() < BOOTSTRAP_REQUEST_HEADER_LEN {
        return Err(BootstrapError::Truncated("bootstrap request"));
    }

    let mut found_magic = [0_u8; 4];
    found_magic.copy_from_slice(&frame[0..4]);
    if found_magic != BOOTSTRAP_REQUEST_MAGIC {
        return Err(BootstrapError::InvalidMagic {
            context: "bootstrap request",
            expected: BOOTSTRAP_REQUEST_MAGIC,
            found: found_magic,
        });
    }

    let sid_len = u16::from_le_bytes([frame[4], frame[5]]) as usize;
    let expected_len = BOOTSTRAP_REQUEST_HEADER_LEN + sid_len;
    if frame.len() != expected_len {
        return Err(BootstrapError::LengthMismatch {
            context: "bootstrap request",
            declared: expected_len,
            actual: frame.len(),
        });
    }

    Ok(BootstrapRequestRef {
        sid: &frame[BOOTSTRAP_REQUEST_HEADER_LEN..],
    })
}

pub fn encode_response(
    status: BootstrapStatus,
    peer_id: u32,
    payload: &[u8],
) -> Result<Vec<u8>, BootstrapError> {
    let payload_len_u16 =
        u16::try_from(payload.len()).map_err(|_| BootstrapError::PayloadTooLong(payload.len()))?;

    let mut out = Vec::with_capacity(BOOTSTRAP_RESPONSE_HEADER_LEN + payload.len());
    out.extend_from_slice(&BOOTSTRAP_RESPONSE_MAGIC);
    out.push(status as u8);
    out.extend_from_slice(&peer_id.to_le_bytes());
    out.extend_from_slice(&payload_len_u16.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn decode_response(frame: &[u8]) -> Result<BootstrapResponseRef<'_>, BootstrapError> {
    if frame.len() < BOOTSTRAP_RESPONSE_HEADER_LEN {
        return Err(BootstrapError::Truncated("bootstrap response"));
    }

    let mut found_magic = [0_u8; 4];
    found_magic.copy_from_slice(&frame[0..4]);
    if found_magic != BOOTSTRAP_RESPONSE_MAGIC {
        return Err(BootstrapError::InvalidMagic {
            context: "bootstrap response",
            expected: BOOTSTRAP_RESPONSE_MAGIC,
            found: found_magic,
        });
    }

    let status = BootstrapStatus::try_from(frame[4])?;
    let peer_id = u32::from_le_bytes([frame[5], frame[6], frame[7], frame[8]]);
    let payload_len = u16::from_le_bytes([frame[9], frame[10]]) as usize;

    let expected_len = BOOTSTRAP_RESPONSE_HEADER_LEN + payload_len;
    if frame.len() != expected_len {
        return Err(BootstrapError::LengthMismatch {
            context: "bootstrap response",
            declared: expected_len,
            actual: frame.len(),
        });
    }

    if status == BootstrapStatus::Error && peer_id != 0 {
        return Err(BootstrapError::InvalidErrorPeerId(peer_id));
    }

    Ok(BootstrapResponseRef {
        status,
        peer_id,
        payload: &frame[BOOTSTRAP_RESPONSE_HEADER_LEN..],
    })
}

#[cfg(unix)]
#[derive(Debug)]
pub struct BootstrapSuccessFds {
    pub doorbell_fd: std::os::fd::RawFd,
    pub segment_fd: std::os::fd::RawFd,
    pub mmap_control_fd: std::os::fd::RawFd,
}

#[cfg(unix)]
#[derive(Debug)]
pub struct BootstrapSuccessOwnedFds {
    pub doorbell_fd: std::os::fd::OwnedFd,
    pub segment_fd: std::os::fd::OwnedFd,
    pub mmap_control_fd: std::os::fd::OwnedFd,
}

#[cfg(unix)]
#[derive(Debug)]
pub struct ReceivedBootstrapResponse {
    pub response: BootstrapResponseOwned,
    pub fds: Option<BootstrapSuccessOwnedFds>,
}

#[cfg(unix)]
fn close_raw_fds(fds: impl IntoIterator<Item = std::os::fd::RawFd>) {
    for fd in fds {
        if fd >= 0 {
            // SAFETY: fd is assumed to be owned by this function on error path.
            let _ = unsafe { libc::close(fd) };
        }
    }
}

#[cfg(unix)]
fn parse_fds(msghdr: &libc::msghdr) -> Vec<std::os::fd::RawFd> {
    let mut out = Vec::new();
    // SAFETY: msghdr points at a valid control buffer owned by caller.
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(msghdr);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let cmsg_len = (*cmsg).cmsg_len as usize;
                let base_len = libc::CMSG_LEN(0) as usize;
                if cmsg_len >= base_len + std::mem::size_of::<std::os::fd::RawFd>() {
                    let bytes = cmsg_len - base_len;
                    let count = bytes / std::mem::size_of::<std::os::fd::RawFd>();
                    let data = libc::CMSG_DATA(cmsg).cast::<std::os::fd::RawFd>();
                    for i in 0..count {
                        out.push(*data.add(i));
                    }
                }
            }
            cmsg = libc::CMSG_NXTHDR(msghdr, cmsg);
        }
    }
    out
}

#[cfg(unix)]
/// Send a bootstrap response and success FDs via `SCM_RIGHTS`.
///
/// r[impl shm.bootstrap.success]
/// r[impl shm.bootstrap.error]
/// r[impl shm.bootstrap.unix]
pub fn send_response_unix(
    control_fd: std::os::fd::RawFd,
    status: BootstrapStatus,
    peer_id: u32,
    payload: &[u8],
    success_fds: Option<&BootstrapSuccessFds>,
) -> Result<(), BootstrapError> {
    use std::io::ErrorKind;

    let frame = encode_response(status, peer_id, payload)?;
    let mut iov = libc::iovec {
        iov_base: frame.as_ptr() as *mut libc::c_void,
        iov_len: frame.len(),
    };

    // SAFETY: zeroed msghdr is valid before assigning pointers.
    let mut msghdr: libc::msghdr = unsafe { std::mem::zeroed() };
    msghdr.msg_iov = &mut iov;
    msghdr.msg_iovlen = 1;

    let mut control_buf = Vec::new();
    if status == BootstrapStatus::Success {
        let success_fds = success_fds.ok_or(BootstrapError::InvalidFdCount(0))?;
        let fds = [
            success_fds.doorbell_fd,
            success_fds.segment_fd,
            success_fds.mmap_control_fd,
        ];
        let fd_count = fds.len();
        let data_len = fd_count * std::mem::size_of::<std::os::fd::RawFd>();
        let cmsg_space = unsafe { libc::CMSG_SPACE(data_len as u32) as usize };
        let cmsg_len = unsafe { libc::CMSG_LEN(data_len as u32) as usize };
        control_buf.resize(cmsg_space, 0);

        msghdr.msg_control = control_buf.as_mut_ptr().cast();
        msghdr.msg_controllen = cmsg_len as _;

        // SAFETY: control buffer sized with CMSG_SPACE and owned here.
        let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msghdr) };
        if cmsg.is_null() {
            return Err(BootstrapError::Io(io::Error::new(
                ErrorKind::InvalidData,
                "failed to allocate SCM_RIGHTS cmsg",
            )));
        }

        // SAFETY: cmsg points into `control_buf` and `fds` has `fd_count` entries.
        unsafe {
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = cmsg_len as _;
            std::ptr::copy_nonoverlapping(
                fds.as_ptr(),
                libc::CMSG_DATA(cmsg).cast::<std::os::fd::RawFd>(),
                fd_count,
            );
        }
    } else if success_fds.is_some() {
        return Err(BootstrapError::InvalidFdCount(3));
    }

    // SAFETY: msghdr points to live iov/control buffers.
    let n = sendmsg_no_sigpipe(control_fd, &msghdr)?;
    if n < 0 {
        return Err(BootstrapError::Io(io::Error::last_os_error()));
    }
    if n == 0 {
        return Err(BootstrapError::Io(io::Error::new(
            ErrorKind::WriteZero,
            "sendmsg wrote zero bytes for bootstrap response",
        )));
    }

    Ok(())
}

#[cfg(unix)]
fn sendmsg_no_sigpipe(
    fd: std::os::fd::RawFd,
    msghdr: &libc::msghdr,
) -> Result<isize, BootstrapError> {
    #[cfg(target_vendor = "apple")]
    ensure_socket_no_sigpipe(fd)?;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    let flags = libc::MSG_NOSIGNAL;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let flags = 0;

    // SAFETY: caller guarantees `msghdr` points to valid iov/cmsg buffers.
    let n = unsafe { libc::sendmsg(fd, msghdr, flags) };
    if n < 0 {
        return Err(BootstrapError::Io(io::Error::last_os_error()));
    }
    Ok(n)
}

#[cfg(all(unix, target_vendor = "apple"))]
fn ensure_socket_no_sigpipe(fd: std::os::fd::RawFd) -> Result<(), BootstrapError> {
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
        return Err(BootstrapError::Io(io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(unix)]
/// Receive a bootstrap response and success FDs via `SCM_RIGHTS`.
///
/// r[impl shm.bootstrap.success]
/// r[impl shm.bootstrap.error]
/// r[impl shm.bootstrap.unix]
pub fn recv_response_unix(
    control_fd: std::os::fd::RawFd,
) -> Result<ReceivedBootstrapResponse, BootstrapError> {
    use std::io::ErrorKind;
    use std::os::fd::{FromRawFd, OwnedFd};

    let mut payload = vec![0_u8; BOOTSTRAP_RESPONSE_HEADER_LEN + u16::MAX as usize];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };

    let cmsg_space =
        unsafe { libc::CMSG_SPACE((3 * std::mem::size_of::<std::os::fd::RawFd>()) as u32) }
            as usize;
    let mut control = vec![0_u8; cmsg_space];

    // SAFETY: zeroed msghdr is valid before assigning pointers.
    let mut msghdr: libc::msghdr = unsafe { std::mem::zeroed() };
    msghdr.msg_iov = &mut iov;
    msghdr.msg_iovlen = 1;
    msghdr.msg_control = control.as_mut_ptr().cast();
    msghdr.msg_controllen = control.len() as _;

    // SAFETY: msghdr points to live iov/control buffers.
    let n = unsafe { libc::recvmsg(control_fd, &mut msghdr, 0) };
    if n < 0 {
        return Err(BootstrapError::Io(io::Error::last_os_error()));
    }
    if n == 0 {
        return Err(BootstrapError::Io(io::Error::new(
            ErrorKind::UnexpectedEof,
            "bootstrap control peer closed",
        )));
    }

    if (msghdr.msg_flags & libc::MSG_CTRUNC) != 0 {
        return Err(BootstrapError::Io(io::Error::new(
            ErrorKind::InvalidData,
            "truncated bootstrap control message",
        )));
    }

    let n = n as usize;
    payload.truncate(n);
    let response_ref = decode_response(&payload)?;

    let raw_fds = parse_fds(&msghdr);
    let response = BootstrapResponseOwned {
        status: response_ref.status,
        peer_id: response_ref.peer_id,
        payload: response_ref.payload.to_vec(),
    };

    match response.status {
        BootstrapStatus::Success => {
            if raw_fds.len() != 3 {
                let fd_count = raw_fds.len();
                close_raw_fds(raw_fds);
                return Err(BootstrapError::InvalidFdCount(fd_count));
            }

            let mut iter = raw_fds.into_iter();
            let doorbell_raw = iter.next().expect("len checked");
            let segment_raw = iter.next().expect("len checked");
            let mmap_raw = iter.next().expect("len checked");

            // SAFETY: FDs came from SCM_RIGHTS and are owned by receiver now.
            let doorbell_fd = unsafe { OwnedFd::from_raw_fd(doorbell_raw) };
            // SAFETY: FDs came from SCM_RIGHTS and are owned by receiver now.
            let segment_fd = unsafe { OwnedFd::from_raw_fd(segment_raw) };
            // SAFETY: FD came from SCM_RIGHTS and is owned by receiver now.
            let mmap_control_fd = unsafe { OwnedFd::from_raw_fd(mmap_raw) };

            Ok(ReceivedBootstrapResponse {
                response,
                fds: Some(BootstrapSuccessOwnedFds {
                    doorbell_fd,
                    segment_fd,
                    mmap_control_fd,
                }),
            })
        }
        BootstrapStatus::Error => {
            if !raw_fds.is_empty() {
                let fd_count = raw_fds.len();
                close_raw_fds(raw_fds);
                return Err(BootstrapError::InvalidFdCount(fd_count));
            }
            Ok(ReceivedBootstrapResponse {
                response,
                fds: None,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Stream-based send/recv (Windows and cross-platform)
// ---------------------------------------------------------------------------

/// Names carried in a successful bootstrap response on platforms without
/// fd-passing (e.g. Windows).
///
/// The payload is encoded as three `\0`-separated UTF-8 strings:
/// `{segment_path}\0{doorbell_name}\0{mmap_ctrl_name}`.
#[derive(Debug, Clone)]
pub struct BootstrapSuccessNames {
    pub segment_path: String,
    pub doorbell_name: String,
    pub mmap_ctrl_name: String,
}

/// Result of receiving a bootstrap response over a byte stream.
#[derive(Debug)]
pub struct ReceivedBootstrapResponseStream {
    pub response: BootstrapResponseOwned,
    pub names: Option<BootstrapSuccessNames>,
}

impl BootstrapSuccessNames {
    /// Encode the three names into a `\0`-separated payload.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(self.segment_path.as_bytes());
        out.push(0);
        out.extend_from_slice(self.doorbell_name.as_bytes());
        out.push(0);
        out.extend_from_slice(self.mmap_ctrl_name.as_bytes());
        out
    }

    /// Decode from a `\0`-separated payload.
    pub fn decode(payload: &[u8]) -> Result<Self, BootstrapError> {
        let parts: Vec<&[u8]> = payload.splitn(3, |&b| b == 0).collect();
        if parts.len() != 3 {
            return Err(BootstrapError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "bootstrap success payload: expected 3 null-separated fields, got {}",
                    parts.len()
                ),
            )));
        }
        let segment_path = String::from_utf8(parts[0].to_vec()).map_err(|e| {
            BootstrapError::Io(io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
        })?;
        let doorbell_name = String::from_utf8(parts[1].to_vec()).map_err(|e| {
            BootstrapError::Io(io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
        })?;
        let mmap_ctrl_name = String::from_utf8(parts[2].to_vec()).map_err(|e| {
            BootstrapError::Io(io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
        })?;
        Ok(Self {
            segment_path,
            doorbell_name,
            mmap_ctrl_name,
        })
    }
}

/// Send a bootstrap response over a byte stream (no fd-passing).
///
/// r[impl shm.bootstrap.success]
/// r[impl shm.bootstrap.error]
pub fn send_response_stream(
    stream: &mut impl io::Write,
    status: BootstrapStatus,
    peer_id: u32,
    payload: &[u8],
) -> Result<(), BootstrapError> {
    let frame = encode_response(status, peer_id, payload)?;
    stream.write_all(&frame)?;
    stream.flush()?;
    Ok(())
}

/// Receive a bootstrap response from a byte stream (no fd-passing).
///
/// On success, parses the payload as `\0`-separated names.
///
/// r[impl shm.bootstrap.success]
/// r[impl shm.bootstrap.error]
pub fn recv_response_stream(
    stream: &mut impl io::Read,
) -> Result<ReceivedBootstrapResponseStream, BootstrapError> {
    // Read the fixed header first.
    let mut header = [0u8; BOOTSTRAP_RESPONSE_HEADER_LEN];
    stream.read_exact(&mut header)?;

    let mut found_magic = [0u8; 4];
    found_magic.copy_from_slice(&header[0..4]);
    if found_magic != BOOTSTRAP_RESPONSE_MAGIC {
        return Err(BootstrapError::InvalidMagic {
            context: "bootstrap response",
            expected: BOOTSTRAP_RESPONSE_MAGIC,
            found: found_magic,
        });
    }

    let status = BootstrapStatus::try_from(header[4])?;
    let peer_id = u32::from_le_bytes([header[5], header[6], header[7], header[8]]);
    let payload_len = u16::from_le_bytes([header[9], header[10]]) as usize;

    if status == BootstrapStatus::Error && peer_id != 0 {
        return Err(BootstrapError::InvalidErrorPeerId(peer_id));
    }

    // Read the variable-length payload.
    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream.read_exact(&mut payload)?;
    }

    let response = BootstrapResponseOwned {
        status,
        peer_id,
        payload: payload.clone(),
    };

    match status {
        BootstrapStatus::Success => {
            let names = BootstrapSuccessNames::decode(&payload)?;
            Ok(ReceivedBootstrapResponseStream {
                response,
                names: Some(names),
            })
        }
        BootstrapStatus::Error => Ok(ReceivedBootstrapResponseStream {
            response,
            names: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        let sid = b"session-123";
        let frame = encode_request(sid).expect("encode request");
        let req = decode_request(&frame).expect("decode request");
        assert_eq!(req.sid, sid);
    }

    #[test]
    fn request_rejects_bad_magic() {
        let mut frame = encode_request(b"sid").expect("encode request");
        frame[0..4].copy_from_slice(b"NOPE");
        let err = decode_request(&frame).expect_err("must reject bad magic");
        assert!(matches!(err, BootstrapError::InvalidMagic { .. }));
    }

    #[test]
    fn response_roundtrip() {
        let frame = encode_response(BootstrapStatus::Success, 7, b"ok").expect("encode response");
        let resp = decode_response(&frame).expect("decode response");
        assert_eq!(resp.status, BootstrapStatus::Success);
        assert_eq!(resp.peer_id, 7);
        assert_eq!(resp.payload, b"ok");
    }

    #[test]
    fn response_rejects_unknown_status() {
        let mut frame =
            encode_response(BootstrapStatus::Success, 7, b"ok").expect("encode response");
        frame[4] = 9;
        let err = decode_response(&frame).expect_err("must reject unknown status");
        assert!(matches!(err, BootstrapError::UnknownStatus(9)));
    }

    #[test]
    fn response_rejects_nonzero_peer_id_for_error() {
        let frame = encode_response(BootstrapStatus::Error, 42, b"err").expect("encode response");
        let err = decode_response(&frame).expect_err("must reject invalid error peer_id");
        assert!(matches!(err, BootstrapError::InvalidErrorPeerId(42)));
    }

    #[cfg(unix)]
    fn socketpair_dgram() -> (std::os::fd::OwnedFd, std::os::fd::OwnedFd) {
        use std::os::fd::{FromRawFd, OwnedFd};

        let mut fds = [0_i32; 2];
        // SAFETY: `fds` points to 2 valid i32 slots.
        let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM, 0, fds.as_mut_ptr()) };
        assert_eq!(rc, 0, "socketpair failed: {}", io::Error::last_os_error());
        // SAFETY: ownership transferred from raw fds created by socketpair.
        let a = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        // SAFETY: ownership transferred from raw fds created by socketpair.
        let b = unsafe { OwnedFd::from_raw_fd(fds[1]) };
        (a, b)
    }

    #[cfg(unix)]
    #[test]
    fn unix_response_roundtrip_with_three_fds() {
        use std::os::fd::AsRawFd;

        let (host, guest) = socketpair_dgram();
        let (db_a, db_b) = socketpair_dgram();
        let (seg_a, seg_b) = socketpair_dgram();
        let (mm_a, mm_b) = socketpair_dgram();

        let send_fds = BootstrapSuccessFds {
            doorbell_fd: db_a.as_raw_fd(),
            segment_fd: seg_a.as_raw_fd(),
            mmap_control_fd: mm_a.as_raw_fd(),
        };

        send_response_unix(
            host.as_raw_fd(),
            BootstrapStatus::Success,
            5,
            b"ready",
            Some(&send_fds),
        )
        .expect("send response");

        let got = recv_response_unix(guest.as_raw_fd()).expect("recv response");
        assert_eq!(got.response.status, BootstrapStatus::Success);
        assert_eq!(got.response.peer_id, 5);
        assert_eq!(got.response.payload, b"ready");
        let fds = got.fds.expect("success must include fds");
        assert!(fds.doorbell_fd.as_raw_fd() >= 0);
        assert!(fds.segment_fd.as_raw_fd() >= 0);
        assert!(fds.mmap_control_fd.as_raw_fd() >= 0);

        // Keep originals alive through the test.
        drop((db_b, seg_b, mm_b));
    }

    #[cfg(unix)]
    #[test]
    fn unix_response_roundtrip_error_without_fds() {
        use std::os::fd::AsRawFd;

        let (host, guest) = socketpair_dgram();

        send_response_unix(
            host.as_raw_fd(),
            BootstrapStatus::Error,
            0,
            b"no slot",
            None,
        )
        .expect("send response");

        let got = recv_response_unix(guest.as_raw_fd()).expect("recv response");
        assert_eq!(got.response.status, BootstrapStatus::Error);
        assert_eq!(got.response.peer_id, 0);
        assert_eq!(got.response.payload, b"no slot");
        assert!(got.fds.is_none());
    }

    #[test]
    fn stream_response_roundtrip_success() {
        let names = BootstrapSuccessNames {
            segment_path: "/tmp/test.shm".to_string(),
            doorbell_name: r"\\.\pipe\vox-db-1234".to_string(),
            mmap_ctrl_name: r"\\.\pipe\vox-mc-5678".to_string(),
        };
        let payload = names.encode();

        let mut buf = Vec::new();
        send_response_stream(&mut buf, BootstrapStatus::Success, 3, &payload)
            .expect("send stream response");

        let mut cursor = io::Cursor::new(buf);
        let got = recv_response_stream(&mut cursor).expect("recv stream response");
        assert_eq!(got.response.status, BootstrapStatus::Success);
        assert_eq!(got.response.peer_id, 3);
        let got_names = got.names.expect("success must have names");
        assert_eq!(got_names.segment_path, "/tmp/test.shm");
        assert_eq!(got_names.doorbell_name, r"\\.\pipe\vox-db-1234");
        assert_eq!(got_names.mmap_ctrl_name, r"\\.\pipe\vox-mc-5678");
    }

    #[test]
    fn stream_response_roundtrip_error() {
        let mut buf = Vec::new();
        send_response_stream(&mut buf, BootstrapStatus::Error, 0, b"rejected")
            .expect("send stream error");

        let mut cursor = io::Cursor::new(buf);
        let got = recv_response_stream(&mut cursor).expect("recv stream error");
        assert_eq!(got.response.status, BootstrapStatus::Error);
        assert_eq!(got.response.peer_id, 0);
        assert!(got.names.is_none());
    }
}
