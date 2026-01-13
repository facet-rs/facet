//! Spawn ticket API for spawning guest processes (Windows).
//!
//! This module provides the infrastructure for the host to:
//! 1. Reserve a peer slot before spawning
//! 2. Create a doorbell pair for wakeup and death detection
//! 3. Pass spawn arguments to the child process
//! 4. Register death callbacks for crash notification
//!
//! On Windows, doorbell communication uses named pipes instead of socketpairs.
//! The pipe name is passed to the child process via command line.
//!
//! shm[impl shm.spawn.ticket]

use std::path::PathBuf;
use std::sync::Arc;

use crate::peer::PeerId;

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
/// The ticket holds the guest's doorbell pipe name and keeps the server
/// side alive until the child process is spawned.
///
/// shm[impl shm.spawn.ticket]
pub struct SpawnTicket {
    /// Path to the SHM segment file
    pub hub_path: PathBuf,
    /// Assigned peer ID
    pub peer_id: PeerId,
    /// Guest's doorbell pipe name
    ///
    /// On Windows, we use named pipes instead of file descriptors.
    /// The guest connects to this pipe name.
    doorbell_pipe: String,
}

impl SpawnTicket {
    /// Create a new spawn ticket.
    pub(crate) fn new(hub_path: PathBuf, peer_id: PeerId, doorbell_pipe: String) -> Self {
        Self {
            hub_path,
            peer_id,
            doorbell_pipe,
        }
    }

    /// Get the doorbell pipe name.
    ///
    /// This name will be passed to the child process for connection.
    pub fn doorbell_pipe(&self) -> &str {
        &self.doorbell_pipe
    }

    /// Convert to command-line arguments.
    ///
    /// Returns arguments in the format:
    /// - `--hub-path=<path>`
    /// - `--peer-id=<id>`
    /// - `--doorbell-pipe=<name>`
    ///
    /// shm[impl shm.spawn.args]
    pub fn to_args(&self) -> Vec<String> {
        vec![
            format!("--hub-path={}", self.hub_path.display()),
            format!("--peer-id={}", self.peer_id.get()),
            format!("--doorbell-pipe={}", self.doorbell_pipe),
        ]
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
    /// Doorbell pipe name
    pub doorbell_pipe: String,
}

impl SpawnArgs {
    /// Parse from command-line arguments.
    ///
    /// Looks for `--hub-path=`, `--peer-id=`, and `--doorbell-pipe=` arguments.
    ///
    /// shm[impl shm.spawn.args]
    pub fn from_args<I, S>(args: I) -> Result<Self, SpawnArgsError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut hub_path = None;
        let mut peer_id = None;
        let mut doorbell_pipe = None;

        for arg in args {
            let arg = arg.as_ref();
            if let Some(value) = arg.strip_prefix("--hub-path=") {
                hub_path = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--peer-id=") {
                let id: u8 = value.parse().map_err(|_| SpawnArgsError::InvalidPeerId)?;
                peer_id = Some(PeerId::new(id).ok_or(SpawnArgsError::InvalidPeerId)?);
            } else if let Some(value) = arg.strip_prefix("--doorbell-pipe=") {
                doorbell_pipe = Some(value.to_string());
            }
        }

        Ok(Self {
            hub_path: hub_path.ok_or(SpawnArgsError::MissingHubPath)?,
            peer_id: peer_id.ok_or(SpawnArgsError::MissingPeerId)?,
            doorbell_pipe: doorbell_pipe.ok_or(SpawnArgsError::MissingDoorbellPipe)?,
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
    /// Missing --doorbell-pipe argument
    MissingDoorbellPipe,
    /// Invalid peer ID value
    InvalidPeerId,
}

impl std::fmt::Display for SpawnArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnArgsError::MissingHubPath => write!(f, "missing --hub-path argument"),
            SpawnArgsError::MissingPeerId => write!(f, "missing --peer-id argument"),
            SpawnArgsError::MissingDoorbellPipe => write!(f, "missing --doorbell-pipe argument"),
            SpawnArgsError::InvalidPeerId => write!(f, "invalid peer ID"),
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
        let args = vec![
            "--hub-path=C:\\temp\\test.shm",
            "--peer-id=1",
            "--doorbell-pipe=\\\\.\\pipe\\roam-shm-test",
        ];

        let parsed = SpawnArgs::from_args(args).unwrap();
        assert_eq!(parsed.hub_path, Path::new("C:\\temp\\test.shm"));
        assert_eq!(parsed.peer_id.get(), 1);
        assert_eq!(parsed.doorbell_pipe, "\\\\.\\pipe\\roam-shm-test");
    }

    #[test]
    fn test_spawn_args_missing_hub_path() {
        let args = vec!["--peer-id=1", "--doorbell-pipe=\\\\.\\pipe\\test"];
        let result = SpawnArgs::from_args(args);
        assert_eq!(result.unwrap_err(), SpawnArgsError::MissingHubPath);
    }

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

    #[test]
    fn test_spawn_ticket_to_args() {
        let ticket = SpawnTicket {
            hub_path: PathBuf::from("C:\\temp\\test.shm"),
            peer_id: PeerId::new(1).unwrap(),
            doorbell_pipe: "\\\\.\\pipe\\roam-shm-test".to_string(),
        };

        let args = ticket.to_args();
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "--hub-path=C:\\temp\\test.shm");
        assert_eq!(args[1], "--peer-id=1");
        assert_eq!(args[2], "--doorbell-pipe=\\\\.\\pipe\\roam-shm-test");
    }
}
