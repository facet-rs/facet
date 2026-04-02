//! Tests for basic echo RPC calls through the driver.
//!
//! Ported from vox-core/src/tests/driver_tests.rs to use generated clients.

use std::time::Duration;
use vox::memory_link_pair;

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

/// Set up a connected client/server pair over in-memory links.
async fn echo_pair() -> (EchoClient, vox::NoopClient) {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        let server = vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService).establish::<vox::NoopClient>())
            .await
            .expect("server establish");
        server
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let server_guard = server.await.expect("server task");
    (client, server_guard)
}

#[tokio::test]
async fn basic_echo_call() {
    let (client, _server) = echo_pair().await;
    let result = client.echo(42).await.expect("echo call");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn multiple_sequential_calls() {
    let (client, _server) = echo_pair().await;
    for i in 0..10 {
        let result = client.echo(i).await.expect("echo call");
        assert_eq!(result, i);
    }
}

#[tokio::test]
async fn dropping_one_client_clone_keeps_session_alive() {
    let (client_link, server_link) = memory_link_pair(16);

    let (server_session_tx, server_session_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();
    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();

    let server_task = tokio::spawn(async move {
        let server = vox::acceptor_on(server_link)
            .spawn_fn(move |fut| {
                let handle = tokio::spawn(fut);
                let _ = server_session_tx.send(handle);
            })
            .on_connection(EchoDispatcher::new(EchoService).establish::<vox::NoopClient>())
            .await
            .expect("server establish");
        server
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .spawn_fn(move |fut| {
            let handle = tokio::spawn(fut);
            let _ = client_session_tx.send(handle);
        })
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let server_guard = server_task.await.expect("server task");
    let client_session = client_session_rx.await.expect("client session");
    let server_session = server_session_rx.await.expect("server session");

    // Clone and drop — session should survive.
    let client_clone = client.clone();
    drop(client_clone);

    let result = client.echo(42).await.expect("call after clone drop");
    assert_eq!(result, 42);

    // Drop everything — sessions should shut down.
    drop(client);
    drop(server_guard);

    tokio::time::timeout(Duration::from_millis(500), client_session)
        .await
        .expect("client session exit timeout")
        .expect("client session failed");
    tokio::time::timeout(Duration::from_millis(500), server_session)
        .await
        .expect("server session exit timeout")
        .expect("server session failed");
}

#[tokio::test]
async fn dropping_root_caller_shuts_down_session() {
    let (client_link, server_link) = memory_link_pair(16);

    let (server_session_tx, server_session_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();
    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();

    let server_task = tokio::spawn(async move {
        let server = vox::acceptor_on(server_link)
            .spawn_fn(move |fut| {
                let handle = tokio::spawn(fut);
                let _ = server_session_tx.send(handle);
            })
            .on_connection(EchoDispatcher::new(EchoService).establish::<vox::NoopClient>())
            .await
            .expect("server establish");
        server
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .spawn_fn(move |fut| {
            let handle = tokio::spawn(fut);
            let _ = client_session_tx.send(handle);
        })
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let server_guard = server_task.await.expect("server task");
    let client_session = client_session_rx.await.expect("client session");
    let server_session = server_session_rx.await.expect("server session");

    drop(client);
    drop(server_guard);

    tokio::time::timeout(Duration::from_millis(500), client_session)
        .await
        .expect("client session exit timeout")
        .expect("client session failed");
    tokio::time::timeout(Duration::from_millis(500), server_session)
        .await
        .expect("server session exit timeout")
        .expect("server session failed");
}

#[tokio::test]
async fn echo_call_across_memory_link() {
    let (client, _server) = echo_pair().await;
    let result = client.echo(42).await.expect("echo across memory link");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn in_flight_call_returns_error_when_peer_closes() {
    let (client_link, server_link) = memory_link_pair(16);

    // Server: establish then immediately drop to close connection.
    let server = tokio::spawn(async move {
        let server = vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService).establish::<vox::NoopClient>())
            .await
            .expect("server establish");
        // Keep alive briefly so client can establish, then drop.
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(server);
    });

    let client = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    server.await.expect("server task");

    // Give the close signal time to propagate.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = client.echo(123).await;
    assert!(result.is_err(), "call should fail after peer closes");
}
