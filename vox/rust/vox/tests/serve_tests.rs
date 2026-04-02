//! Tests for vox::serve().

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

    let client: EchoClient = vox::connect(format!("tcp://{addr}"))
        .await
        .expect("connect");
    let result = client.echo(42).await.expect("echo");
    assert_eq!(result, 42);

    server.abort();
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

    let client: EchoClient = vox::connect(&addr).await.expect("connect");
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

    let client: EchoClient = vox::connect(format!("ws://{addr}")).await.expect("connect");
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
