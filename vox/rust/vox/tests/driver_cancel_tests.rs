//! Tests for request cancellation behavior.
//!
//! Cancellation is triggered by dropping/aborting the client call future.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use vox::memory_link_pair;

#[vox::service]
trait Blocker {
    async fn block(&self) -> u32;
}

#[vox::service]
trait PersistBlocker {
    #[vox(persist)]
    async fn block_persist(&self) -> u32;
}

/// Blocks forever. Tracks cancellation via a drop guard.
#[derive(Clone)]
struct BlockingService {
    was_cancelled: Arc<AtomicBool>,
}

impl Blocker for BlockingService {
    async fn block(&self) -> u32 {
        struct DropGuard(Arc<AtomicBool>);
        impl Drop for DropGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _guard = DropGuard(self.was_cancelled.clone());
        std::future::pending::<u32>().await
    }
}

/// Blocks until released, then returns 123. Tracks cancellation.
#[derive(Clone)]
struct PersistBlockingService {
    was_cancelled: Arc<AtomicBool>,
    release: Arc<tokio::sync::Notify>,
}

impl PersistBlocker for PersistBlockingService {
    async fn block_persist(&self) -> u32 {
        struct DropGuard(Arc<AtomicBool>);
        impl Drop for DropGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _guard = DropGuard(self.was_cancelled.clone());
        self.release.notified().await;
        123
    }
}

// Wire-level RequestCancel (volatile handler abort) is tested in
// vox-core/src/tests/driver_tests.rs::cancel_aborts_in_flight_handler
// using the connection_sender() escape hatch.

#[tokio::test]
async fn abort_does_not_cancel_persist_handler() {
    let (client_link, server_link) = memory_link_pair(16);
    let was_cancelled = Arc::new(AtomicBool::new(false));
    let release = Arc::new(tokio::sync::Notify::new());

    let server = tokio::spawn({
        let was_cancelled = was_cancelled.clone();
        let release = release.clone();
        async move {
            let (s, _sh) = vox::acceptor_on(server_link)
                .establish::<vox::NoopClient>(PersistBlockerDispatcher::new(
                    PersistBlockingService {
                        was_cancelled,
                        release,
                    },
                ))
                .await
                .expect("server establish");
            s
        }
    });

    let (client, _sh) = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<PersistBlockerClient>(())
        .await
        .expect("client establish");

    let _server_guard = server.await.expect("server task");

    // Spawn the call and abort it.
    let call = tokio::spawn(async move { client.block_persist().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    call.abort();

    // Wait a bit — handler should NOT be cancelled.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !was_cancelled.load(Ordering::SeqCst),
        "persist handler should not be cancelled by client abort"
    );

    // Release the handler so it completes normally.
    release.notify_waiters();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Handler completed normally, not via cancellation.
    // (drop guard fires on normal completion too, so we check it wasn't
    // set BEFORE release)
}
