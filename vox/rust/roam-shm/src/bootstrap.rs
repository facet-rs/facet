//! Bootstrap control-plane for SHM guest attachment.
//!
//! This module provides a small rendezvous handshake over a Unix domain socket
//! located in a session directory:
//!
//! - Host validates a `sid`
//! - Host returns guest bootstrap metadata
//! - Host transfers the doorbell fd via SCM_RIGHTS

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::peer::PeerId;

const REQUEST_MAGIC: [u8; 4] = *b"RSH0";
const RESPONSE_MAGIC: [u8; 4] = *b"RSP0";
const STATUS_OK: u8 = 0;
const STATUS_ERROR: u8 = 1;

/// Session identifier used for App Group rendezvous.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Parse and validate a session id.
    ///
    /// Accepted formats:
    /// - 32 hex chars (`0123456789abcdef...`)
    /// - canonical UUID form (`8-4-4-4-12` hex chars)
    pub fn parse(raw: &str) -> Result<Self, SessionIdError> {
        if is_hex_32(raw) || is_uuid_like(raw) {
            return Ok(Self(raw.to_string()));
        }
        Err(SessionIdError::InvalidFormat)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionIdError {
    InvalidFormat,
}

impl fmt::Display for SessionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => write!(f, "invalid sid format"),
        }
    }
}

impl std::error::Error for SessionIdError {}

/// Paths for a single SHM rendezvous session.
#[derive(Debug, Clone)]
pub struct SessionPaths {
    container_root: PathBuf,
    sid: SessionId,
}

impl SessionPaths {
    pub fn new(container_root: impl AsRef<Path>, sid: SessionId) -> io::Result<Self> {
        let container_root = container_root.as_ref();
        if !container_root.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "container root must be absolute",
            ));
        }

        Ok(Self {
            container_root: container_root.to_path_buf(),
            sid,
        })
    }

    pub fn container_root(&self) -> &Path {
        &self.container_root
    }

    pub fn sid(&self) -> &SessionId {
        &self.sid
    }

    pub fn sessions_root(&self) -> PathBuf {
        self.container_root.join("sessions")
    }

    pub fn session_dir(&self) -> PathBuf {
        self.sessions_root().join(self.sid.as_str())
    }

    pub fn shm_path(&self) -> PathBuf {
        self.session_dir().join("shm.dat")
    }

    pub fn control_sock_path(&self) -> PathBuf {
        self.session_dir().join("control.sock")
    }

    /// Create session directories if needed.
    pub fn ensure_dirs(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.session_dir())
    }

    /// Validate that session paths stay under container root and sid dir has no symlinks.
    pub fn validate_containment(&self) -> io::Result<()> {
        assert_under_root(&self.container_root, &self.sessions_root())?;
        assert_under_root(&self.container_root, &self.session_dir())?;

        let session_dir = self.session_dir();
        if session_dir.exists() {
            let md = std::fs::symlink_metadata(&session_dir)?;
            if md.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "session dir must not be a symlink",
                ));
            }
        }

        Ok(())
    }
}

fn assert_under_root(root: &Path, child: &Path) -> io::Result<()> {
    use std::path::Component;

    if !root.is_absolute() || !child.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "paths must be absolute",
        ));
    }

    if child
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "child path contains traversal components",
        ));
    }

    if child.starts_with(root) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "child path escapes root",
        ))
    }
}

#[derive(Debug)]
pub enum BootstrapError {
    Io(io::Error),
    Protocol(String),
    SidMismatch { expected: String, got: String },
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Protocol(e) => write!(f, "protocol error: {e}"),
            Self::SidMismatch { expected, got } => {
                write!(f, "sid mismatch: expected {expected}, got {got}")
            }
        }
    }
}

impl std::error::Error for BootstrapError {}

impl From<io::Error> for BootstrapError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Metadata returned to the guest during bootstrap.
#[derive(Debug, Clone)]
pub struct BootstrapTicket {
    pub peer_id: PeerId,
    pub hub_path: PathBuf,
    pub doorbell_fd: std::os::fd::RawFd,
    pub shm_fd: std::os::fd::RawFd,
}

#[cfg(unix)]
pub mod unix {
    use super::*;
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;
    use std::os::fd::RawFd;

    use roam_fdpass::{recv_fds, send_fds};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};

    /// Bind a session control socket for bootstrap.
    pub fn bind_control_socket(paths: &SessionPaths) -> io::Result<UnixListener> {
        paths.validate_containment()?;
        paths.ensure_dirs()?;

        let control_sock = paths.control_sock_path();
        if control_sock.exists() {
            std::fs::remove_file(&control_sock)?;
        }

        UnixListener::bind(control_sock)
    }

    /// Host side: accept one bootstrap connection and transfer doorbell fd + shm backing fd.
    pub async fn accept_and_send_ticket(
        listener: &UnixListener,
        expected_sid: &SessionId,
        peer_id: PeerId,
        hub_path: &Path,
        doorbell_fd: RawFd,
    ) -> Result<(), BootstrapError> {
        let (mut stream, _) = listener.accept().await?;

        let sid = read_request_sid(&mut stream)
            .await
            .map_err(|e| BootstrapError::Protocol(format!("read request sid failed: {e}")))?;
        if sid != expected_sid.as_str() {
            let msg = format!(
                "sid mismatch: expected {}, got {sid}",
                expected_sid.as_str()
            );
            write_error(&mut stream, &msg).await?;
            return Err(BootstrapError::SidMismatch {
                expected: expected_sid.as_str().to_string(),
                got: sid,
            });
        }

        write_ok(&mut stream, peer_id, hub_path)
            .await
            .map_err(|e| BootstrapError::Protocol(format!("write response failed: {e}")))?;
        read_fd_ack(&mut stream)
            .await
            .map_err(|e| BootstrapError::Protocol(format!("read ack0 failed: {e}")))?;
        let shm_file = OpenOptions::new().read(true).write(true).open(hub_path)?;
        let shm_fd = shm_file.as_raw_fd();
        send_fds(&stream, &[doorbell_fd, shm_fd])
            .await
            .map_err(|e| BootstrapError::Protocol(format!("send fds failed: {e}")))?;

        Ok(())
    }

    /// Guest side: connect to control socket, request ticket, and receive doorbell + shm fds.
    pub async fn request_ticket(
        control_sock: &Path,
        sid: &SessionId,
    ) -> Result<BootstrapTicket, BootstrapError> {
        let mut stream = UnixStream::connect(control_sock)
            .await
            .map_err(|e| BootstrapError::Protocol(format!("connect failed: {e}")))?;
        write_request_sid(&mut stream, sid.as_str())
            .await
            .map_err(|e| BootstrapError::Protocol(format!("write request sid failed: {e}")))?;

        let (peer_id, hub_path) = read_response(&mut stream)
            .await
            .map_err(|e| BootstrapError::Protocol(format!("read response failed: {e}")))?;
        write_fd_ack(&mut stream)
            .await
            .map_err(|e| BootstrapError::Protocol(format!("write ack0 failed: {e}")))?;
        let mut fds = recv_fds(&stream, 2)
            .await
            .map_err(|e| BootstrapError::Protocol(format!("recv fds failed: {e}")))?;
        let doorbell_fd = fds.remove(0);
        let shm_fd = fds.remove(0);

        Ok(BootstrapTicket {
            peer_id,
            hub_path,
            doorbell_fd,
            shm_fd,
        })
    }

    async fn write_request_sid(stream: &mut UnixStream, sid: &str) -> Result<(), BootstrapError> {
        let sid_bytes = sid.as_bytes();
        let sid_len = u16::try_from(sid_bytes.len())
            .map_err(|_| BootstrapError::Protocol("sid too long".to_string()))?;

        stream.write_all(&REQUEST_MAGIC).await?;
        stream.write_all(&sid_len.to_le_bytes()).await?;
        stream.write_all(sid_bytes).await?;
        stream.flush().await?;

        Ok(())
    }

    async fn write_fd_ack(stream: &mut UnixStream) -> Result<(), BootstrapError> {
        stream.write_all(&[0xA5]).await?;
        stream.flush().await?;
        Ok(())
    }

    async fn read_fd_ack(stream: &mut UnixStream) -> Result<(), BootstrapError> {
        let mut ack = [0u8; 1];
        stream.read_exact(&mut ack).await?;
        if ack[0] != 0xA5 {
            return Err(BootstrapError::Protocol("bad fd ack".to_string()));
        }
        Ok(())
    }

    async fn read_request_sid(stream: &mut UnixStream) -> Result<String, BootstrapError> {
        let mut magic = [0u8; 4];
        stream.read_exact(&mut magic).await?;
        if magic != REQUEST_MAGIC {
            return Err(BootstrapError::Protocol("bad request magic".to_string()));
        }

        let mut len = [0u8; 2];
        stream.read_exact(&mut len).await?;
        let sid_len = u16::from_le_bytes(len) as usize;
        if sid_len == 0 {
            return Err(BootstrapError::Protocol("empty sid".to_string()));
        }

        let mut sid = vec![0u8; sid_len];
        stream.read_exact(&mut sid).await?;
        let sid = String::from_utf8(sid)
            .map_err(|_| BootstrapError::Protocol("sid not utf-8".to_string()))?;

        Ok(sid)
    }

    async fn write_ok(
        stream: &mut UnixStream,
        peer_id: PeerId,
        hub_path: &Path,
    ) -> Result<(), BootstrapError> {
        let hub_bytes = hub_path.as_os_str().as_encoded_bytes();
        let hub_len = u16::try_from(hub_bytes.len())
            .map_err(|_| BootstrapError::Protocol("hub path too long".to_string()))?;

        stream.write_all(&RESPONSE_MAGIC).await?;
        stream.write_all(&[STATUS_OK]).await?;
        stream.write_all(&[peer_id.get()]).await?;
        stream.write_all(&hub_len.to_le_bytes()).await?;
        stream.write_all(hub_bytes).await?;
        stream.flush().await?;

        Ok(())
    }

    async fn write_error(stream: &mut UnixStream, msg: &str) -> Result<(), BootstrapError> {
        let msg_bytes = msg.as_bytes();
        let msg_len = u16::try_from(msg_bytes.len())
            .map_err(|_| BootstrapError::Protocol("error message too long".to_string()))?;

        stream.write_all(&RESPONSE_MAGIC).await?;
        stream.write_all(&[STATUS_ERROR]).await?;
        stream.write_all(&[0]).await?;
        stream.write_all(&msg_len.to_le_bytes()).await?;
        stream.write_all(msg_bytes).await?;
        stream.flush().await?;

        Ok(())
    }

    async fn read_response(stream: &mut UnixStream) -> Result<(PeerId, PathBuf), BootstrapError> {
        let mut magic = [0u8; 4];
        stream.read_exact(&mut magic).await?;
        if magic != RESPONSE_MAGIC {
            return Err(BootstrapError::Protocol("bad response magic".to_string()));
        }

        let mut status = [0u8; 1];
        stream.read_exact(&mut status).await?;

        let mut peer = [0u8; 1];
        stream.read_exact(&mut peer).await?;

        let mut len = [0u8; 2];
        stream.read_exact(&mut len).await?;
        let payload_len = u16::from_le_bytes(len) as usize;

        let mut payload = vec![0u8; payload_len];
        stream.read_exact(&mut payload).await?;

        match status[0] {
            STATUS_OK => {
                let peer_id = PeerId::new(peer[0])
                    .ok_or_else(|| BootstrapError::Protocol("invalid peer id".to_string()))?;
                let hub_path = PathBuf::from(
                    String::from_utf8(payload)
                        .map_err(|_| BootstrapError::Protocol("hub path not utf-8".to_string()))?,
                );
                Ok((peer_id, hub_path))
            }
            STATUS_ERROR => {
                let message = String::from_utf8(payload)
                    .map_err(|_| BootstrapError::Protocol("error payload not utf-8".to_string()))?;
                Err(BootstrapError::Protocol(message))
            }
            other => Err(BootstrapError::Protocol(format!(
                "unknown response status {other}"
            ))),
        }
    }
}

fn is_hex_32(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_uuid_like(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }

    let bytes = s.as_bytes();
    for &idx in &[8, 13, 18, 23] {
        if bytes[idx] != b'-' {
            return false;
        }
    }

    for (i, &b) in bytes.iter().enumerate() {
        if [8, 13, 18, 23].contains(&i) {
            continue;
        }
        if !b.is_ascii_hexdigit() {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sid_accepts_hex32() {
        let sid = SessionId::parse("0123456789abcdef0123456789abcdef");
        assert!(sid.is_ok());
    }

    #[test]
    fn sid_accepts_uuid() {
        let sid = SessionId::parse("123e4567-e89b-12d3-a456-426614174000");
        assert!(sid.is_ok());
    }

    #[test]
    fn sid_rejects_invalid() {
        let bad = [
            "",
            "abc",
            "123e4567-e89b-12d3-a456-42661417400z",
            "123e4567e89b12d3a45642661417400",
            "../../oops",
        ];

        for sid in bad {
            assert!(SessionId::parse(sid).is_err(), "accepted bad sid: {sid}");
        }
    }

    #[test]
    fn session_paths_under_root() {
        let sid = SessionId::parse("0123456789abcdef0123456789abcdef").unwrap();
        let paths = SessionPaths::new("/tmp/roam-test-root", sid).unwrap();
        assert!(paths.sessions_root().starts_with(paths.container_root()));
        assert!(paths.session_dir().starts_with(paths.container_root()));
        assert!(
            paths
                .control_sock_path()
                .starts_with(paths.container_root())
        );
    }
}
