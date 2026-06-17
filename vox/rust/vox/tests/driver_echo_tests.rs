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
async fn echo_pair() -> (EchoClient, vox::ConnectionHandle) {
    let (client_link, server_link) = memory_link_pair(16);

    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService))
            .establish_connection()
            .await
            .expect("server establish")
    });

    let client = vox::initiator_on(client_link)
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
async fn dropping_one_client_clone_keeps_connection_alive() {
    let (client_link, server_link) = memory_link_pair(16);

    let (server_connection_task_tx, server_connection_task_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();
    let (client_connection_task_tx, client_connection_task_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();

    let server_task = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .spawn_fn(move |fut| {
                let handle = tokio::spawn(fut);
                let _ = server_connection_task_tx.send(handle);
            })
            .on_connection(EchoDispatcher::new(EchoService))
            .establish_connection()
            .await
            .expect("server establish")
    });

    let client = vox::initiator_on(client_link)
        .spawn_fn(move |fut| {
            let handle = tokio::spawn(fut);
            let _ = client_connection_task_tx.send(handle);
        })
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let server_guard = server_task.await.expect("server task");
    let client_connection_task = client_connection_task_rx.await.expect("client connection");
    let server_connection_task = server_connection_task_rx.await.expect("server connection");

    // Clone and drop — connection should survive.
    let client_clone = client.clone();
    drop(client_clone);

    let result = client.echo(42).await.expect("call after clone drop");
    assert_eq!(result, 42);

    let client_connection = client.connection.clone().expect("client connection handle");

    // Dropping clients is inert; shutdown is explicit.
    drop(client);
    client_connection
        .shutdown()
        .expect("client shutdown request");
    let _ = server_guard.shutdown();

    tokio::time::timeout(Duration::from_millis(500), client_connection_task)
        .await
        .expect("client connection exit timeout")
        .expect("client connection failed");
    tokio::time::timeout(Duration::from_millis(500), server_connection_task)
        .await
        .expect("server connection exit timeout")
        .expect("server connection failed");
}

#[tokio::test]
async fn dropping_generated_client_does_not_shut_down_connection() {
    let (client_link, server_link) = memory_link_pair(16);

    let (server_connection_task_tx, server_connection_task_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();
    let (client_connection_task_tx, client_connection_task_rx) =
        tokio::sync::oneshot::channel::<tokio::task::JoinHandle<()>>();

    let server_task = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .spawn_fn(move |fut| {
                let handle = tokio::spawn(fut);
                let _ = server_connection_task_tx.send(handle);
            })
            .on_connection(EchoDispatcher::new(EchoService))
            .establish_connection()
            .await
            .expect("server establish")
    });

    let client = vox::initiator_on(client_link)
        .spawn_fn(move |fut| {
            let handle = tokio::spawn(fut);
            let _ = client_connection_task_tx.send(handle);
        })
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    let server_guard = server_task.await.expect("server task");
    let client_connection_task = client_connection_task_rx.await.expect("client connection");
    let server_connection_task = server_connection_task_rx.await.expect("server connection");
    let client_connection = client.connection.clone().expect("client connection handle");

    drop(client);

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !client_connection_task.is_finished(),
        "dropping the generated client must not shut down the client connection"
    );
    assert!(
        !server_connection_task.is_finished(),
        "dropping the generated client must not shut down the server connection"
    );

    client_connection
        .shutdown()
        .expect("client shutdown request");
    let _ = server_guard.shutdown();

    tokio::time::timeout(Duration::from_millis(500), client_connection_task)
        .await
        .expect("client connection exit timeout")
        .expect("client connection failed");
    tokio::time::timeout(Duration::from_millis(500), server_connection_task)
        .await
        .expect("server connection exit timeout")
        .expect("server connection failed");
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

    // Server: establish then shut down the whole connection.
    let server = tokio::spawn(async move {
        let server = vox::acceptor_on(server_link)
            .on_connection(EchoDispatcher::new(EchoService))
            .establish_connection()
            .await
            .expect("server establish");
        // Keep alive briefly so client can establish, then close.
        tokio::time::sleep(Duration::from_millis(50)).await;
        server.shutdown().expect("server shutdown");
        server.closed().await;
    });

    let client = vox::initiator_on(client_link)
        .establish::<EchoClient>()
        .await
        .expect("client establish");

    server.await.expect("server task");

    // Give the close signal time to propagate.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = client.echo(123).await;
    assert!(result.is_err(), "call should fail after peer closes");
}
