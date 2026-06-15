//! Tests for vox::serve().

use std::sync::Arc;

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

#[vox::service]
trait Ping {
    async fn ping(&self, value: u32) -> u32;
}

#[derive(Clone)]
struct PingService;

impl Ping for PingService {
    async fn ping(&self, value: u32) -> u32 {
        value
    }
}

#[tokio::test]
async fn serve_and_connect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let server = tokio::spawn(async move {
        vox::serve_listener(listener, EchoDispatcher::new(EchoService))
            .await
            .expect("serve");
    });

    // Give server a moment to start accepting.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client: EchoClient = vox::connect_lane(format!("tcp://{addr}"))
        .await
        .expect("connect");
    let result = client.echo(42).await.expect("echo");
    assert_eq!(result, 42);

    server.abort();
}

// r[verify rpc.session-setup]
#[tokio::test]
async fn connect_builder_establish_matches_await() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let server = tokio::spawn(async move {
        vox::serve_listener(listener, EchoDispatcher::new(EchoService))
            .await
            .expect("serve");
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = vox::connect_lane::<EchoClient>(format!("tcp://{addr}"))
        .establish()
        .await
        .expect("connect");
    let result = client.echo(77).await.expect("echo");
    assert_eq!(result, 77);

    server.abort();
}

#[test]
fn try_send_error_is_reexported_from_vox() {
    let err = vox::TrySendError::Full(42_u32);
    match err {
        vox::TrySendError::Full(value) => assert_eq!(value, 42),
        vox::TrySendError::Closed(_) => panic!("expected Full"),
    }
}

// r[verify rpc.flow-control.credit.initial.high-level]
#[tokio::test]
async fn high_level_builders_accept_channel_capacity() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let server = tokio::spawn(async move {
        vox::serve_listener(listener, EchoDispatcher::new(EchoService))
            .channel_capacity(2)
            .await
            .expect("serve");
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client: EchoClient = vox::connect_lane(format!("tcp://{addr}"))
        .channel_capacity(2)
        .await
        .expect("connect");
    assert_eq!(client.echo(11).await.expect("echo"), 11);

    server.abort();
}

// r[verify rpc.flow-control.credit.initial.zero]
#[tokio::test]
async fn connect_builder_rejects_zero_channel_capacity() {
    let result: Result<EchoClient, _> = vox::connect_lane("tcp://127.0.0.1:1")
        .channel_capacity(0)
        .await;
    match result {
        Err(vox::ConnectionError::Protocol(message)) => {
            assert_eq!(message, "channel_capacity must be greater than zero");
        }
        Ok(_) => panic!("connect unexpectedly succeeded"),
        Err(other) => panic!("expected protocol error, got {other:?}"),
    }
}

// r[verify rpc.flow-control.credit.initial.zero]
#[tokio::test]
async fn serve_builder_rejects_zero_channel_capacity() {
    let result = vox::serve("tcp://127.0.0.1:0", EchoDispatcher::new(EchoService))
        .channel_capacity(0)
        .run()
        .await;
    match result {
        Err(vox::ServeError::Connection(vox::ConnectionError::Protocol(message))) => {
            assert_eq!(message, "channel_capacity must be greater than zero");
        }
        Ok(_) => panic!("serve unexpectedly succeeded"),
        Err(other) => panic!("expected protocol error, got {other:?}"),
    }
}

// r[verify rpc.flow-control.credit.initial.zero]
#[tokio::test]
async fn serve_listener_builder_rejects_zero_channel_capacity() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let result = vox::serve_listener(listener, EchoDispatcher::new(EchoService))
        .channel_capacity(0)
        .run()
        .await;
    match result {
        Err(vox::ConnectionError::Protocol(message)) => {
            assert_eq!(message, "channel_capacity must be greater than zero");
        }
        Ok(_) => panic!("serve_listener unexpectedly succeeded"),
        Err(other) => panic!("expected protocol error, got {other:?}"),
    }
}

// r[verify rpc.virtual-connection.accept]
#[tokio::test]
async fn connect_builder_can_configure_inbound_virtual_connections_before_await() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let accepted = Arc::new(tokio::sync::Notify::new());
    let accepted_server = accepted.clone();

    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("accept");
        let root = vox::acceptor_on(vox::transport::tcp::StreamLink::tcp(socket))
            .on_connection(PingDispatcher::new(PingService))
            .establish_connection()
            .await
            .expect("server establish");
        let session = root.clone();
        let client: EchoClient = session
            .open_lane_with_settings(vox::ConnectionSettings {
                parity: vox::Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            })
            .await
            .expect("open echo client");
        let echoed = client.echo(41).await.expect("echo");
        assert_eq!(echoed, 41);
        accepted_server.notify_one();
        root
    });

    let client: PingClient = vox::connect_lane(format!("tcp://{addr}"))
        .on_connection(EchoDispatcher::new(EchoService))
        .await
        .expect("connect");

    let pinged = client.ping(9).await.expect("ping");
    assert_eq!(pinged, 9);

    tokio::time::timeout(std::time::Duration::from_secs(1), accepted.notified())
        .await
        .expect("server never used inbound virtual connection");

    let _server_root = server.await.expect("server task");
}

#[cfg(unix)]
#[tokio::test]
async fn serve_local_with_lock() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock_path = dir.path().join("echo.sock");
    let addr = format!("local://{}", sock_path.display());

    let server = {
        let addr = addr.clone();
        tokio::spawn(async move {
            vox::serve(&addr, EchoDispatcher::new(EchoService))
                .await
                .expect("serve");
        })
    };

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client: EchoClient = vox::connect_lane(&addr).await.expect("connect");
    let result = client.echo(99).await.expect("echo");
    assert_eq!(result, 99);

    // A second bind should fail with AddrInUse while the first is alive.
    let err = vox_stream::LocalLinkAcceptor::bind_with_lock(sock_path.to_str().unwrap())
        .err()
        .expect("second bind should fail");
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);

    server.abort();
}

#[cfg(feature = "transport-websocket")]
#[tokio::test]
async fn serve_websocket() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let server = tokio::spawn(async move {
        vox::serve_listener(
            vox::WsListener::from_tcp(listener),
            EchoDispatcher::new(EchoService),
        )
        .await
        .expect("serve");
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client: EchoClient = vox::connect_lane(format!("ws://{addr}"))
        .await
        .expect("connect");
    let result = client.echo(7).await.expect("echo");
    assert_eq!(result, 7);

    server.abort();
}

#[cfg(feature = "transport-websocket")]
#[tokio::test]
async fn serve_websocket_string() {
    let server = tokio::spawn(async move {
        vox::serve("ws://127.0.0.1:0", EchoDispatcher::new(EchoService))
            .await
            .expect("serve");
    });

    // Port 0 won't work with the string API since we can't discover the bound port.
    // This test just verifies the code path compiles and starts. Abort immediately.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    server.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn serve_local_lock_released_after_drop() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock_path = dir.path().join("echo.sock");
    let path_str = sock_path.to_str().unwrap();

    // First bind succeeds.
    let (acceptor, lock) =
        vox_stream::LocalLinkAcceptor::bind_with_lock(path_str).expect("first bind");
    drop(acceptor);
    drop(lock);

    // After dropping the lock, a second bind should succeed.
    let (_acceptor2, _lock2) = vox_stream::LocalLinkAcceptor::bind_with_lock(path_str)
        .expect("second bind after lock release");
}
