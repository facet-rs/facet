//! Spawn ticket API for spawning guest processes.
//!
//! This module provides the infrastructure for the host to:
//! 1. Reserve a peer slot before spawning
//! 2. Create a doorbell pair for wakeup and death detection
//! 3. Pass spawn arguments to the child process
//! 4. Register death callbacks for crash notification
//!
//! # Ensuring child processes die with parent
//!
//! Use [`SpawnTicket::spawn`] to spawn a child that will automatically be
//! terminated when the parent dies (even via SIGKILL). The child should call
//! [`die_with_parent`] early in its main function.
//!
//! shm[impl shm.spawn.ticket]

use std::io;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Arc;

use crate::peer::PeerId;

pub use ur_taking_me_with_you::die_with_parent;

/// Callback invoked when a peer dies.
///
/// shm[impl shm.death.callback]
pub type DeathCallback = Arc<dyn Fn(PeerId) + Send + Sync>;

/// Options for adding a peer.
///
/// shm[impl shm.spawn.ticket]
#[derive(Default)]
pub struct AddPeerOptions {
    /// Human-readable name for debugging
    pub peer_name: Option<String>,
    /// Callback when peer dies (doorbell death or heartbeat timeout)
    ///
    /// shm[impl shm.death.callback]
    pub on_death: Option<DeathCallback>,
}

/// Information needed by a spawned guest to attach.
///
/// The ticket holds the guest's doorbell fd and keeps it alive until
/// the child process is spawned. After spawn, drop the ticket to close
/// the parent's copy of the fd.
///
/// shm[impl shm.spawn.ticket]
pub struct SpawnTicket {
    /// Path to the SHM segment file
    pub hub_path: PathBuf,
    /// Assigned peer ID
    pub peer_id: PeerId,
    /// Guest's doorbell fd (inheritable, CLOEXEC cleared)
    ///
    /// This fd is owned by the ticket. When the ticket is dropped,
    /// the fd is closed. The child process inherits it via fork/exec.
    doorbell_fd: RawFd,
}

impl SpawnTicket {
    /// Create a new spawn ticket.
    ///
    /// The doorbell_fd should already have CLOEXEC cleared.
    pub(crate) fn new(hub_path: PathBuf, peer_id: PeerId, doorbell_fd: RawFd) -> Self {
        Self {
            hub_path,
            peer_id,
            doorbell_fd,
        }
    }

    /// Get the doorbell file descriptor.
    ///
    /// This fd will be inherited by the child process.
    pub fn doorbell_fd(&self) -> RawFd {
        self.doorbell_fd
    }

    /// Convert to command-line arguments.
    ///
    /// Returns arguments in the format:
    /// - `--hub-path=<path>`
    /// - `--peer-id=<id>`
    /// - `--doorbell-fd=<fd>`
    ///
    /// shm[impl shm.spawn.args]
    pub fn to_args(&self) -> Vec<String> {
        vec![
            format!("--hub-path={}", self.hub_path.display()),
            format!("--peer-id={}", self.peer_id.get()),
            format!("--doorbell-fd={}", self.doorbell_fd),
        ]
    }

    /// Spawn a child process using this ticket.
    ///
    /// This is a convenience method that:
    /// 1. Adds the spawn arguments to the command
    /// 2. Spawns the child with die-with-parent behavior
    ///
    /// The spawned child will automatically be terminated when the parent dies,
    /// even if the parent is killed with SIGKILL. The child should call
    /// [`die_with_parent`] early in its main function.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ticket = host.add_peer(AddPeerOptions::default())?;
    /// let mut cmd = Command::new("my-cell");
    /// let child = ticket.spawn(cmd)?;
    /// ```
    ///
    /// shm[impl shm.spawn.ticket]
    pub fn spawn(&self, mut command: Command) -> io::Result<Child> {
        command.args(self.to_args());
        ur_taking_me_with_you::spawn_dying_with_parent(command)
    }
}

impl Drop for SpawnTicket {
    fn drop(&mut self) {
        // Close our copy of the guest's doorbell fd.
        // The child process has inherited it and will use it.
        unsafe {
            libc::close(self.doorbell_fd);
        }
    }
}

/// Parsed spawn arguments for guest initialization.
///
/// Use this to parse the command-line arguments passed to a spawned guest.
///
/// shm[impl shm.spawn.guest-init]
#[derive(Debug, Clone)]
pub struct SpawnArgs {
    /// Path to the SHM segment file
    pub hub_path: PathBuf,
    /// Assigned peer ID
    pub peer_id: PeerId,
    /// Doorbell file descriptor
    pub doorbell_fd: RawFd,
}

impl SpawnArgs {
    /// Parse from command-line arguments.
    ///
    /// Looks for `--hub-path=`, `--peer-id=`, and `--doorbell-fd=` arguments.
    ///
    /// shm[impl shm.spawn.args]
    pub fn from_args<I, S>(args: I) -> Result<Self, SpawnArgsError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut hub_path = None;
        let mut peer_id = None;
        let mut doorbell_fd = None;

        for arg in args {
            let arg = arg.as_ref();
            if let Some(value) = arg.strip_prefix("--hub-path=") {
                hub_path = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--peer-id=") {
                let id: u8 = value.parse().map_err(|_| SpawnArgsError::InvalidPeerId)?;
                peer_id = Some(PeerId::new(id).ok_or(SpawnArgsError::InvalidPeerId)?);
            } else if let Some(value) = arg.strip_prefix("--doorbell-fd=") {
                doorbell_fd = Some(value.parse().map_err(|_| SpawnArgsError::InvalidFd)?);
            }
        }

        Ok(Self {
            hub_path: hub_path.ok_or(SpawnArgsError::MissingHubPath)?,
            peer_id: peer_id.ok_or(SpawnArgsError::MissingPeerId)?,
            doorbell_fd: doorbell_fd.ok_or(SpawnArgsError::MissingDoorbellFd)?,
        })
    }

    /// Parse from `std::env::args()`.
    ///
    /// Convenience method for spawned guests.
    pub fn from_env() -> Result<Self, SpawnArgsError> {
        Self::from_args(std::env::args())
    }
}

/// Errors when parsing spawn arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnArgsError {
    /// Missing --hub-path argument
    MissingHubPath,
    /// Missing --peer-id argument
    MissingPeerId,
    /// Missing --doorbell-fd argument
    MissingDoorbellFd,
    /// Invalid peer ID value
    InvalidPeerId,
    /// Invalid file descriptor value
    InvalidFd,
}

impl std::fmt::Display for SpawnArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnArgsError::MissingHubPath => write!(f, "missing --hub-path argument"),
            SpawnArgsError::MissingPeerId => write!(f, "missing --peer-id argument"),
            SpawnArgsError::MissingDoorbellFd => write!(f, "missing --doorbell-fd argument"),
            SpawnArgsError::InvalidPeerId => write!(f, "invalid peer ID"),
            SpawnArgsError::InvalidFd => write!(f, "invalid file descriptor"),
        }
    }
}

impl std::error::Error for SpawnArgsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_spawn_args_parsing() {
        let args = vec!["--hub-path=/tmp/test.shm", "--peer-id=1", "--doorbell-fd=5"];

        let parsed = SpawnArgs::from_args(args).unwrap();
        assert_eq!(parsed.hub_path, Path::new("/tmp/test.shm"));
        assert_eq!(parsed.peer_id.get(), 1);
        assert_eq!(parsed.doorbell_fd, 5);
    }

    #[test]
    fn test_spawn_args_missing_hub_path() {
        let args = vec!["--peer-id=1", "--doorbell-fd=5"];
        let result = SpawnArgs::from_args(args);
        assert_eq!(result.unwrap_err(), SpawnArgsError::MissingHubPath);
    }

    #[test]
    fn test_spawn_args_invalid_peer_id() {
        let args = vec![
            "--hub-path=/tmp/test.shm",
            "--peer-id=0", // 0 is invalid
            "--doorbell-fd=5",
        ];
        let result = SpawnArgs::from_args(args);
        assert_eq!(result.unwrap_err(), SpawnArgsError::InvalidPeerId);
    }

    #[test]
    fn test_spawn_ticket_to_args() {
        // Create a dummy fd for testing (we won't actually use it)
        let ticket = SpawnTicket {
            hub_path: PathBuf::from("/tmp/test.shm"),
            peer_id: PeerId::new(1).unwrap(),
            doorbell_fd: 42,
        };

        let args = ticket.to_args();
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "--hub-path=/tmp/test.shm");
        assert_eq!(args[1], "--peer-id=1");
        assert_eq!(args[2], "--doorbell-fd=42");

        // Prevent the destructor from closing fd 42
        std::mem::forget(ticket);
    }
}
