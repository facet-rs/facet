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
//! terminated when the parent dies (even via SIGKILL/TerminateProcess).
//! The child should call [`die_with_parent`] early in its main function.
//!
//! shm[impl shm.spawn.ticket]

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Arc;

use shm_primitives::DoorbellHandle;

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
/// The ticket holds the guest's doorbell handle and keeps it alive until
/// the child process is spawned.
///
/// shm[impl shm.spawn.ticket]
pub struct SpawnTicket {
    /// Path to the SHM segment file
    pub hub_path: PathBuf,
    /// Assigned peer ID
    pub peer_id: PeerId,
    /// Guest's doorbell handle (platform-specific).
    /// Owns the underlying resource (FD on Unix, nothing on Windows).
    doorbell_handle: DoorbellHandle,
}

impl SpawnTicket {
    /// Create a new spawn ticket.
    pub(crate) fn new(hub_path: PathBuf, peer_id: PeerId, doorbell_handle: DoorbellHandle) -> Self {
        Self {
            hub_path,
            peer_id,
            doorbell_handle,
        }
    }

    /// Get the doorbell handle.
    pub fn doorbell_handle(&self) -> &DoorbellHandle {
        &self.doorbell_handle
    }

    /// Convert this ticket into a SpawnArgs, consuming the ticket.
    ///
    /// This is useful for in-process guest creation (e.g., in tests)
    /// where you want to attach a guest using the ticket's peer ID and doorbell.
    ///
    /// The doorbell handle ownership is transferred to SpawnArgs.
    pub fn into_spawn_args(self) -> SpawnArgs {
        SpawnArgs {
            hub_path: self.hub_path,
            peer_id: self.peer_id,
            doorbell_handle: self.doorbell_handle,
        }
    }

    /// Convert to command-line arguments.
    ///
    /// Returns arguments in the format:
    /// - `--hub-path=<path>`
    /// - `--peer-id=<id>`
    /// - `--doorbell-fd=<fd>` (Unix) or `--doorbell-pipe=<name>` (Windows)
    ///
    /// shm[impl shm.spawn.args]
    pub fn to_args(&self) -> Vec<String> {
        vec![
            format!("--hub-path={}", self.hub_path.display()),
            format!("--peer-id={}", self.peer_id.get()),
            format!(
                "{}={}",
                DoorbellHandle::ARG_NAME,
                self.doorbell_handle.to_arg()
            ),
        ]
    }

    /// Spawn a child process using this ticket.
    ///
    /// This is a convenience method that:
    /// 1. Adds the spawn arguments to the command
    /// 2. Spawns the child with die-with-parent behavior
    ///
    /// The spawned child will automatically be terminated when the parent dies,
    /// even if the parent is killed with SIGKILL/TerminateProcess. The child should call
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

// DoorbellHandle now owns its resources (OwnedFd on Unix), so no Drop impl needed

/// Parsed spawn arguments for guest initialization.
///
/// Use this to parse the command-line arguments passed to a spawned guest.
///
/// shm[impl shm.spawn.guest-init]
#[derive(Debug)]
pub struct SpawnArgs {
    /// Path to the SHM segment file
    pub hub_path: PathBuf,
    /// Assigned peer ID
    pub peer_id: PeerId,
    /// Doorbell handle (platform-specific)
    pub doorbell_handle: DoorbellHandle,
}

impl SpawnArgs {
    /// Parse from command-line arguments.
    ///
    /// Looks for `--hub-path=`, `--peer-id=`, and the platform-specific doorbell argument.
    ///
    /// shm[impl shm.spawn.args]
    pub fn from_args<I, S>(args: I) -> Result<Self, SpawnArgsError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut hub_path = None;
        let mut peer_id = None;
        let mut doorbell_arg = None;

        // Build the expected prefix from the platform-specific arg name
        let doorbell_prefix = format!("{}=", DoorbellHandle::ARG_NAME);

        for arg in args {
            let arg = arg.as_ref();
            if let Some(value) = arg.strip_prefix("--hub-path=") {
                hub_path = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--peer-id=") {
                let id: u8 = value.parse().map_err(|_| SpawnArgsError::InvalidPeerId)?;
                peer_id = Some(PeerId::new(id).ok_or(SpawnArgsError::InvalidPeerId)?);
            } else if let Some(value) = arg.strip_prefix(&doorbell_prefix) {
                doorbell_arg = Some(value.to_string());
            }
        }

        // Check all required fields before creating DoorbellHandle
        // (creating the handle takes ownership of the FD, so we must not fail after)
        let hub_path = hub_path.ok_or(SpawnArgsError::MissingHubPath)?;
        let peer_id = peer_id.ok_or(SpawnArgsError::MissingPeerId)?;
        let doorbell_arg = doorbell_arg.ok_or(SpawnArgsError::MissingDoorbellHandle)?;

        // SAFETY: We're in a spawned child process that inherited the FD
        let doorbell_handle = unsafe { DoorbellHandle::from_arg(&doorbell_arg) }
            .map_err(|_| SpawnArgsError::InvalidHandle)?;

        Ok(Self {
            hub_path,
            peer_id,
            doorbell_handle,
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
    /// Missing doorbell handle argument
    MissingDoorbellHandle,
    /// Invalid peer ID value
    InvalidPeerId,
    /// Invalid doorbell handle value
    InvalidHandle,
}

impl std::fmt::Display for SpawnArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnArgsError::MissingHubPath => write!(f, "missing --hub-path argument"),
            SpawnArgsError::MissingPeerId => write!(f, "missing --peer-id argument"),
            SpawnArgsError::MissingDoorbellHandle => {
                write!(f, "missing {} argument", DoorbellHandle::ARG_NAME)
            }
            SpawnArgsError::InvalidPeerId => write!(f, "invalid peer ID"),
            SpawnArgsError::InvalidHandle => write!(f, "invalid doorbell handle"),
        }
    }
}

impl std::error::Error for SpawnArgsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg(unix)]
    #[test]
    fn test_spawn_args_parsing() {
        let args = vec!["--hub-path=/tmp/test.shm", "--peer-id=1", "--doorbell-fd=5"];

        let parsed = SpawnArgs::from_args(args).unwrap();
        assert_eq!(parsed.hub_path, Path::new("/tmp/test.shm"));
        assert_eq!(parsed.peer_id.get(), 1);
        assert_eq!(parsed.doorbell_handle.as_raw_fd(), 5);

        // Don't drop - the FD 5 is fake and closing it would SIGABRT
        std::mem::forget(parsed);
    }

    #[cfg(windows)]
    #[test]
    fn test_spawn_args_parsing() {
        let args = vec![
            "--hub-path=C:\\temp\\test.shm",
            "--peer-id=1",
            "--doorbell-pipe=\\\\.\\pipe\\roam-shm-test",
        ];

        let parsed = SpawnArgs::from_args(args).unwrap();
        assert_eq!(parsed.hub_path, Path::new("C:\\temp\\test.shm"));
        assert_eq!(parsed.peer_id.get(), 1);
        assert_eq!(
            parsed.doorbell_handle.as_pipe_name(),
            "\\\\.\\pipe\\roam-shm-test"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_spawn_args_missing_hub_path() {
        let args = vec!["--peer-id=1", "--doorbell-fd=5"];
        let result = SpawnArgs::from_args(args);
        assert_eq!(result.unwrap_err(), SpawnArgsError::MissingHubPath);
        // Note: on error path, no SpawnArgs is created so no fake FD to worry about
    }

    #[cfg(windows)]
    #[test]
    fn test_spawn_args_missing_hub_path() {
        let args = vec!["--peer-id=1", "--doorbell-pipe=\\\\.\\pipe\\test"];
        let result = SpawnArgs::from_args(args);
        assert_eq!(result.unwrap_err(), SpawnArgsError::MissingHubPath);
    }

    #[cfg(unix)]
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

    #[cfg(windows)]
    #[test]
    fn test_spawn_args_invalid_peer_id() {
        let args = vec![
            "--hub-path=C:\\temp\\test.shm",
            "--peer-id=0", // 0 is invalid
            "--doorbell-pipe=\\\\.\\pipe\\test",
        ];
        let result = SpawnArgs::from_args(args);
        assert_eq!(result.unwrap_err(), SpawnArgsError::InvalidPeerId);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_spawn_ticket_to_args() {
        use shm_primitives::Doorbell;

        // Create a real doorbell pair for testing
        let (_host, guest_handle) = Doorbell::create_pair().unwrap();

        // Capture the FD before moving the handle
        let expected_fd = guest_handle.as_raw_fd();

        let ticket = SpawnTicket {
            hub_path: PathBuf::from("/tmp/test.shm"),
            peer_id: PeerId::new(1).unwrap(),
            doorbell_handle: guest_handle,
        };

        let args = ticket.to_args();
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "--hub-path=/tmp/test.shm");
        assert_eq!(args[1], "--peer-id=1");
        assert_eq!(args[2], format!("--doorbell-fd={}", expected_fd));
    }

    #[cfg(windows)]
    #[test]
    fn test_spawn_ticket_to_args() {
        let ticket = SpawnTicket {
            hub_path: PathBuf::from("C:\\temp\\test.shm"),
            peer_id: PeerId::new(1).unwrap(),
            doorbell_handle: DoorbellHandle::from_pipe_name(
                "\\\\.\\pipe\\roam-shm-test".to_string(),
            ),
        };

        let args = ticket.to_args();
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "--hub-path=C:\\temp\\test.shm");
        assert_eq!(args[1], "--peer-id=1");
        assert_eq!(args[2], "--doorbell-pipe=\\\\.\\pipe\\roam-shm-test");
    }
}
