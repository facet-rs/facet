#[cfg(not(unix))]
use std::sync::Arc;
#[cfg(unix)]
use std::{sync::Arc, time::Duration};

#[cfg(unix)]
use vox_core::initiator;
use vox_core::{IdentityResolver, LaneAcceptor};
use vox_types::{Metadata, VoxObserverHandle};

use super::{IdentityResolverRef, ServeError, VoxListener, serve_listener};

impl VoxListener for vox_stream::LocalLinkAcceptor {
    type Link = vox_stream::LocalLink;

    async fn accept(&mut self) -> std::io::Result<Self::Link> {
        vox_stream::LocalLinkAcceptor::accept(self).await
    }
}

#[cfg(unix)]
pub(super) async fn serve_local(
    host: &str,
    acceptor: impl LaneAcceptor,
    metadata: Metadata,
    channel_capacity: u32,
    observer: Option<VoxObserverHandle>,
    identity_resolver: Option<Arc<dyn IdentityResolver>>,
) -> Result<(), ServeError> {
    let lock = match vox_stream::try_local_lock(host)? {
        vox_stream::LocalLockOutcome::Acquired(lock) => {
            let _ = std::fs::remove_file(host);
            lock
        }
        vox_stream::LocalLockOutcome::Held => {
            let health = tokio::time::timeout(Duration::from_secs(5), async {
                let source = vox_stream::local_link_source(host);
                initiator(source).establish_connection().await
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
    let mut builder = serve_listener(listener, acceptor).channel_capacity(channel_capacity);
    builder = builder.metadata(metadata);
    if let Some(observer) = observer {
        builder = builder.observer_handle(observer);
    }
    if let Some(resolver) = identity_resolver {
        builder = builder.identity_resolver(IdentityResolverRef(resolver));
    }
    Ok(builder.await?)
}

#[cfg(not(unix))]
pub(super) async fn serve_local(
    host: &str,
    acceptor: impl LaneAcceptor,
    metadata: Metadata,
    channel_capacity: u32,
    observer: Option<VoxObserverHandle>,
    identity_resolver: Option<Arc<dyn IdentityResolver>>,
) -> Result<(), ServeError> {
    // Named pipes on Windows handle concurrency at the OS level;
    // no file-lock dance is needed.
    let listener = vox_stream::LocalLinkAcceptor::bind(host)?;
    let mut builder = serve_listener(listener, acceptor).channel_capacity(channel_capacity);
    builder = builder.metadata(metadata);
    if let Some(observer) = observer {
        builder = builder.observer_handle(observer);
    }
    if let Some(resolver) = identity_resolver {
        builder = builder.identity_resolver(IdentityResolverRef(resolver));
    }
    Ok(builder.await?)
}
