//! Tests for virtual connections through the driver.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use vox::{ConnectionSettings, Driver, Metadata, Parity, SessionHandle, memory_link_pair};

#[vox::service]
trait Echo {
    async fn echo(&self, value: u32) -> u32;
}

#[derive(Clone)]
struct EchoService;

impl Echo for EchoService {
    async fn echo(&self, value: u32) -> u32 {
        value
    }
}

async fn vconn_server(server_link: impl vox::Link + Send + 'static) -> vox::NoopClient {
    let server = vox::acceptor_on(server_link)
        .on_connection(EchoDispatcher::new(EchoService))
        .establish::<vox::NoopClient>()
        .await
        .expect("server establish");
    server
}

async fn open_echo_vconn(session: &SessionHandle) -> EchoClient {
    let vconn_handle = session
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = vox::Caller::new(vconn_driver.caller());
    tokio::spawn(async move { vconn_driver.run().await });

    EchoClient::new(caller)
}

#[tokio::test]
async fn open_virtual_connection_and_call() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move { vconn_server(server_link).await });

    let root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>()
        .await
        .expect("client establish");
    let session = root.session.clone().unwrap();

    let _server_guard = server.await.expect("server task");
    let vconn_client = open_echo_vconn(&session).await;

    let result = vconn_client.echo(123).await.expect("vconn echo");
    assert_eq!(result, 123);

    drop(vconn_client);
    drop(root);
}

// r[verify rpc.caller.liveness.root-internal-close]
// r[verify rpc.caller.liveness.root-teardown-condition]
#[tokio::test]
async fn dropping_root_waits_for_virtual_connections() {
    let (client_link, server_link) = memory_link_pair(16);

    let (session_tx, session_rx) = tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();

    let server = tokio::spawn(async move { vconn_server(server_link).await });

    let root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .spawn_fn(move |fut| {
            let handle = tokio::spawn(fut);
            let _ = session_tx.send(handle);
        })
        .establish::<vox::NoopClient>()
        .await
        .expect("client establish");
    let session = root.session.clone().unwrap();

    let _server_guard = server.await.expect("server task");
    let client_session = session_rx.await.expect("session handle");

    let session_handle = session;
    let vconn_client = open_echo_vconn(&session_handle).await;

    // Drop root — session should stay alive because vconn is still active.
    drop(root);
    drop(session_handle);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "session should remain alive while virtual connection exists"
    );

    // vconn still works.
    let result = vconn_client
        .echo(7)
        .await
        .expect("vconn echo after root drop");
    assert_eq!(result, 7);

    // Drop vconn — session should now shut down.
    drop(vconn_client);

    tokio::time::timeout(Duration::from_millis(500), client_session)
        .await
        .expect("session exit timeout")
        .expect("session failed");
}

#[tokio::test]
async fn schema_tracker_is_per_connection_not_per_session() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish");
        s
    });

    let root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<EchoClient>()
        .await
        .expect("client establish");
    let session = root.session.clone().unwrap();

    let _server_guard = server.await.expect("server task");
    let session_handle = session;

    // Call on root connection.
    let r1 = root.echo(100).await.expect("root echo");
    assert_eq!(r1, 100);

    // Open vconn and call — schemas should not conflict with root.
    let vconn_client = open_echo_vconn(&session_handle).await;
    let r2 = vconn_client.echo(200).await.expect("vconn echo");
    assert_eq!(r2, 200);
}

#[vox::service]
trait Counter {
    async fn increment(&self) -> u32;
}

#[derive(Clone)]
struct CounterService {
    count: std::sync::Arc<AtomicU32>,
}

impl Counter for CounterService {
    async fn increment(&self) -> u32 {
        self.count.fetch_add(1, Ordering::SeqCst) + 1
    }
}

struct RejectAcceptor;

impl vox::ConnectionAcceptor for RejectAcceptor {
    fn accept(
        &self,
        _request: &vox::ConnectionRequest,
        _connection: vox::PendingConnection,
    ) -> Result<(), Metadata<'static>> {
        Err(vec![])
    }
}

#[tokio::test]
async fn reject_virtual_connection() {
    let (client_link, server_link) = memory_link_pair(16);

    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let server = tokio::spawn({
        let call_count = call_count.clone();
        async move {
            let s = vox::acceptor_on(server_link)
                .on_connection(vox::acceptor_fn(
                    move |_req: &vox::ConnectionRequest, conn: vox::PendingConnection| {
                        let n = call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        if n == 0 {
                            conn.handle_with(());
                            Ok(())
                        } else {
                            Err(vec![])
                        }
                    },
                ))
                .establish::<vox::NoopClient>()
                .await
                .expect("server establish");
            s
        }
    });

    let _root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>()
        .await
        .expect("client establish");
    let session = _root.session.clone().unwrap();

    let _server_guard = server.await.expect("server task");
    let session_handle = session;

    let result = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await;

    assert!(result.is_err(), "connection should be rejected");
}

#[tokio::test]
async fn open_virtual_connection_without_acceptor_is_rejected() {
    let (client_link, server_link) = memory_link_pair(16);

    // Server with NO on_connection acceptor.
    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish");
        s
    });

    let _root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>()
        .await
        .expect("client establish");
    let session = _root.session.clone().unwrap();

    let _server_guard = server.await.expect("server task");
    let session_handle = session;

    let result = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await;

    assert!(result.is_ok(), "default acceptor should accept connections");
}

#[tokio::test]
async fn close_virtual_connection() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .on_connection(CounterDispatcher::new(CounterService {
                count: std::sync::Arc::new(AtomicU32::new(0)),
            }))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish");
        s
    });

    let _root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>()
        .await
        .expect("client establish");
    let session = _root.session.clone().unwrap();

    let _server_guard = server.await.expect("server task");
    let session_handle = session;

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open vconn");

    let conn_id = vconn_handle.connection_id();
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = vox::Caller::new(vconn_driver.caller());
    tokio::spawn(async move { vconn_driver.run().await });

    let client = CounterClient::new(caller);
    let r = client.increment().await.expect("increment before close");
    assert_eq!(r, 1);

    session_handle
        .close_connection(conn_id, vec![])
        .await
        .expect("close vconn");

    // Call after close should fail.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = client.increment().await;
    assert!(result.is_err(), "call after close should fail");
}

#[tokio::test]
async fn close_root_connection_is_rejected() {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let s = vox::acceptor_on(server_link)
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish");
        s
    });

    let _root = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<vox::NoopClient>()
        .await
        .expect("client establish");
    let session = _root.session.clone().unwrap();

    let _server_guard = server.await.expect("server task");
    let session_handle = session;

    // Connection ID 0 is the root connection.
    let result = session_handle
        .close_connection(vox::ConnectionId(0), vec![])
        .await;
    assert!(result.is_err(), "closing root connection should fail");
}
