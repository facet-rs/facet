#[cfg(unix)]
use std::time::Duration;

use vox_core::ConnectionAcceptor;
#[cfg(unix)]
use vox_core::{NoopClient, TransportMode, initiator};

use super::{ServeError, VoxListener, serve_listener};

impl VoxListener for vox_stream::LocalLinkAcceptor {
    type Link = vox_stream::LocalLink;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        vox_stream::LocalLinkAcceptor::accept(self).await
    }
}

#[cfg(unix)]
pub(super) async fn serve_local(
    host: &str,
    acceptor: impl ConnectionAcceptor,
) -> Result<(), ServeError> {
    let lock = match vox_stream::try_local_lock(host)? {
        vox_stream::LocalLockOutcome::Acquired(lock) => {
            let _ = std::fs::remove_file(host);
            lock
        }
        vox_stream::LocalLockOutcome::Held => {
            let health = tokio::time::timeout(Duration::from_secs(5), async {
                let source = vox_stream::local_link_source(host);
                initiator(source, TransportMode::Bare)
                    .establish::<NoopClient>()
                    .await
            })
            .await;
            return match health {
                Ok(Ok(_client)) => Err(ServeError::AddrInUse {
                    addr: host.to_string(),
                }),
                _ => Err(ServeError::LockHeldUnhealthy {
                    addr: host.to_string(),
                }),
            };
        }
    };
    let listener = vox_stream::LocalLinkAcceptor::bind(host)?;
    let _lock = lock;
    Ok(serve_listener(listener, acceptor).await?)
}

#[cfg(not(unix))]
pub(super) async fn serve_local(
    host: &str,
    acceptor: impl ConnectionAcceptor,
) -> Result<(), ServeError> {
    // Named pipes on Windows handle concurrency at the OS level;
    // no file-lock dance is needed.
    let listener = vox_stream::LocalLinkAcceptor::bind(host)?;
    Ok(serve_listener(listener, acceptor).await?)
}
