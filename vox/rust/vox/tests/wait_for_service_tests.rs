//! Tests for ConnectBuilder::wait_for_service initial-connect waiting.

use std::time::Duration;
use vox_core::SessionError;

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

// r[verify session.initial-connect-waiting]
// r[verify session.initial-connect-waiting.retryable]
// r[verify session.initial-connect-waiting.timeout]
#[cfg(unix)]
#[tokio::test]
async fn wait_for_service_retries_until_service_appears() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock_path = dir.path().join("echo.sock");
    let addr = format!("local://{}", sock_path.display());

    // Socket does not exist yet. Server starts after a short delay.
    let server = {
        let addr = addr.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            vox::serve(&addr, EchoDispatcher::new(EchoService))
                .await
                .expect("serve");
        })
    };

    let client: EchoClient = vox::connect(&addr)
        .connect_timeout(Duration::from_millis(100))
        .wait_for_service(Duration::from_secs(5))
        .await
        .expect("should connect once service starts");

    let result = client.echo(42).await.expect("echo");
    assert_eq!(result, 42);

    server.abort();
}

// r[verify session.initial-connect-waiting]
// r[verify session.initial-connect-waiting.retryable]
// r[verify session.initial-connect-waiting.timeout]
// r[verify session.initial-connect-waiting.backoff]
// r[verify session.initial-connect-waiting.no-session]
#[tokio::test]
async fn wait_for_service_times_out_when_service_never_starts() {
    // Bind to get a free port then drop the listener so nothing is listening.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);

    let result = vox::connect::<EchoClient>(format!("tcp://{addr}"))
        .connect_timeout(Duration::from_millis(50))
        .wait_for_service(Duration::from_millis(200))
        .establish()
        .await;

    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected failure when service never starts"),
    };
    assert!(
        matches!(err, SessionError::Io(_) | SessionError::ConnectTimeout),
        "error should be retryable kind (Io or ConnectTimeout), got: {err:?}"
    );
}

// r[verify session.initial-connect-waiting.non-retryable]
#[tokio::test]
async fn wait_for_service_fails_immediately_on_protocol_error() {
    // Server that immediately closes the connection without speaking vox.
    // This causes SessionError::Protocol (link closed during transport prologue),
    // which is non-retryable and must not consume the full wait_for_service timeout.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let server = tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.expect("accept");
            drop(socket);
        }
    });

    let start = std::time::Instant::now();
    let result = vox::connect::<EchoClient>(format!("tcp://{addr}"))
        .connect_timeout(Duration::from_millis(100))
        .wait_for_service(Duration::from_secs(10)) // long timeout — should NOT be consumed
        .establish()
        .await;
    let elapsed = start.elapsed();
    server.abort();

    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("should fail with protocol error"),
    };
    assert!(
        elapsed < Duration::from_secs(2),
        "non-retryable error should fail fast, not wait the full timeout; elapsed: {elapsed:?}"
    );
    assert!(
        matches!(err, SessionError::Protocol(_)),
        "expected SessionError::Protocol for peer that closed connection, got: {err:?}"
    );
}
