//! Regression tests for channel lifetimes across RPC request boundaries.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;
use vox::Rx;

#[derive(Clone, Debug, facet::Facet)]
#[repr(u8)]
enum AttachError {
    Rejected,
}

#[vox::service]
trait BulkChannelStash {
    async fn attach(&self, input: Rx<Vec<u64>>) -> Result<(), AttachError>;
}

type Accepted = Option<oneshot::Sender<Rx<Vec<u64>>>>;

#[derive(Clone)]
struct BulkChannelStashService {
    accepted: Arc<Mutex<Accepted>>,
}

impl BulkChannelStash for BulkChannelStashService {
    async fn attach(&self, input: Rx<Vec<u64>>) -> Result<(), AttachError> {
        let sender = self
            .accepted
            .lock()
            .expect("accepted mutex poisoned")
            .take()
            .expect("attach called more than once");
        assert!(sender.send(input).is_ok(), "test receiver was dropped");
        Ok(())
    }
}

#[cfg(unix)]
// r[verify rpc.channel.delivery.reliable]
#[tokio::test]
async fn local_keepalive_rx_argument_survives_large_item_burst() {
    const ITEMS: u64 = 65;
    const WORDS_PER_ITEM: usize = 8192;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("channel.sock");
    let addr = path.display().to_string();
    let listener = vox::transport::local::LocalLinkAcceptor::bind(addr.clone()).expect("bind");
    let (accepted_tx, accepted_rx) = oneshot::channel::<Rx<Vec<u64>>>();
    let service = BulkChannelStashService {
        accepted: Arc::new(Mutex::new(Some(accepted_tx))),
    };

    let server = tokio::spawn(async move {
        let server_link = listener.accept().await.expect("accept");
        let client = vox::acceptor_on(server_link)
            .channel_capacity(64)
            .keepalive(vox::SessionKeepaliveConfig {
                ping_interval: Duration::from_secs(5),
                pong_timeout: Duration::from_secs(30),
            })
            .on_connection(BulkChannelStashDispatcher::new(service))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish");
        client.caller.closed().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client_link = vox::transport::local::LocalLink::connect(&addr)
        .await
        .expect("connect");
    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .channel_capacity(64)
        .keepalive(vox::SessionKeepaliveConfig {
            ping_interval: Duration::from_secs(5),
            pong_timeout: Duration::from_secs(30),
        })
        .establish::<BulkChannelStashClient>()
        .await
        .expect("client establish")
        .with_middleware(vox::ClientLogging::default());

    let (tx, rx) = vox::channel::<Vec<u64>>();
    client.attach(rx).await.expect("attach");
    let mut server_rx = accepted_rx.await.expect("server did not store Rx");

    let receiver = tokio::spawn(async move {
        for expected in 0..ITEMS {
            let received = server_rx
                .recv()
                .await
                .map_err(|err| format!("server Rx recv failed: {err:?}"))?
                .ok_or_else(|| {
                    format!("server Rx closed during large-item burst after {expected} items")
                })?;
            received.map(|item| {
                assert_eq!(item.first().copied(), Some(expected));
                assert_eq!(item.len(), WORDS_PER_ITEM);
            });
        }
        Ok::<(), String>(())
    });

    let sender = tokio::spawn(async move {
        for value in 0..ITEMS {
            let mut item = vec![value; WORDS_PER_ITEM];
            item[WORDS_PER_ITEM - 1] = ITEMS - value;
            tx.send(item)
                .await
                .expect("send failed during large-item burst");
        }
    });
    let sender_abort = sender.abort_handle();

    tokio::select! {
        receiver_result = receiver => {
            sender_abort.abort();
            receiver_result
                .expect("receiver task panicked")
                .expect("receiver failed");
        }
        sender_result = sender => {
            sender_result.expect("sender task failed");
        }
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            panic!("large-item burst did not complete");
        }
    }

    drop(client);
    server.abort();
}
