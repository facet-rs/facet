use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use super::hub_session::{HubHost, HubSessionError};
use super::{ShmTransport, close_peer_fd};
use crate::AnyTransport;

#[cfg(unix)]
use std::os::unix::io::RawFd;

/// Arguments and resources needed to spawn a hub-based peer (cell/plugin).
///
/// The `doorbell_fd` is the *peer* end of the doorbell socketpair and must be
/// inherited by the child process (i.e. it must not have CLOEXEC set).
///
/// Drop closes `doorbell_fd` on the host side (you typically keep it alive
/// until after the child has been spawned).
#[cfg(unix)]
pub struct HubPeerTicket {
    pub hub_path: PathBuf,
    pub peer_id: u16,
    pub doorbell_fd: RawFd,
}

#[cfg(unix)]
impl HubPeerTicket {
    /// Add `--hub-path=... --peer-id=... --doorbell-fd=...` to a command.
    pub fn apply_to_command<'a>(&self, cmd: &'a mut Command) -> &'a mut Command {
        cmd.arg(format!("--hub-path={}", self.hub_path.display()))
            .arg(format!("--peer-id={}", self.peer_id))
            .arg(format!("--doorbell-fd={}", self.doorbell_fd))
    }
}

#[cfg(unix)]
impl Drop for HubPeerTicket {
    fn drop(&mut self) {
        close_peer_fd(self.doorbell_fd);
    }
}

#[cfg(unix)]
impl HubHost {
    /// Allocate a new peer in this hub and return:
    /// - A host-side `AnyTransport` wired to that peer's ring pair.
    /// - A `HubPeerTicket` containing the CLI args/fd needed to spawn the peer process.
    pub fn add_peer_transport(
        self: &Arc<Self>,
    ) -> Result<(AnyTransport, HubPeerTicket), HubSessionError> {
        let peer_info = self.add_peer()?;

        let transport = AnyTransport::new(ShmTransport::hub_host_peer(
            self.clone(),
            peer_info.peer_id,
            peer_info.doorbell,
        ));

        let ticket = HubPeerTicket {
            hub_path: self.path().to_path_buf(),
            peer_id: peer_info.peer_id,
            doorbell_fd: peer_info.peer_doorbell_fd,
        };

        Ok((transport, ticket))
    }
}
