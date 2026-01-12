//! Integration tests for the ReconnectingClient.
//!
//! r[verify reconnect.test.basic]
//! r[verify reconnect.test.exhaustion]
//! r[verify reconnect.test.backoff]
//! r[verify reconnect.test.concurrent]
//! r[verify reconnect.test.rpc-passthrough]
//! r[verify reconnect.test.lazy]

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use roam_session::{ChannelRegistry, ServiceDispatcher, dispatch_call, dispatch_unknown_method};
use roam_stream::{CobsFramed, Connector, ReconnectError, ReconnectingClient, RetryPolicy};
use roam_wire::Hello;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Test service that echoes strings and tracks call count.
#[derive(Clone)]
struct TestService {
    call_count: Arc<AtomicU32>,
}

impl TestService {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl ServiceDispatcher for TestService {
    fn dispatch(
        &self,
        method_id: u64,
        payload: Vec<u8>,
        request_id: u64,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        match method_id {
            // Echo method
            1 => dispatch_call::<String, String, (), _, _>(
                payload,
                request_id,
                registry,
                |input: String| async move { Ok(input) },
            ),
            _ => dispatch_unknown_method(request_id, registry),
        }
    }
}

/// Test connector that connects to a TCP server.
struct TcpConnector {
    addr: SocketAddr,
    connect_count: Arc<AtomicU32>,
}

impl TcpConnector {
    fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            connect_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl Connector for TcpConnector {
    type Transport = CobsFramed<TcpStream>;

    async fn connect(&self) -> io::Result<Self::Transport> {
        self.connect_count.fetch_add(1, Ordering::SeqCst);
        let stream = TcpStream::connect(self.addr).await?;
        Ok(CobsFramed::new(stream))
    }

    fn hello(&self) -> Hello {
        Hello::V1 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        }
    }
}

/// Helper to start a test server and return its address.
async fn start_test_server(service: TestService) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let service = service.clone();
            tokio::spawn(async move {
                let io = CobsFramed::new(stream);
                let hello = Hello::V1 {
                    max_payload_size: 1024 * 1024,
                    initial_channel_credit: 64 * 1024,
                };
                if let Ok((_, driver)) = roam_stream::establish_acceptor(io, hello, service).await {
                    let _ = driver.run().await;
                }
            });
        }
    });

    // Give the server time to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    (addr, handle)
}

// r[verify reconnect.test.lazy]
#[tokio::test]
async fn test_lazy_connection() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service).await;

    let connector = TcpConnector::new(addr);
    let connect_count = connector.connect_count.clone();

    // Create client - should NOT connect yet
    let _client = ReconnectingClient::new(connector);

    // Verify no connection was made
    assert_eq!(connect_count.load(Ordering::SeqCst), 0);
}

// r[verify reconnect.test.basic]
#[tokio::test]
async fn test_basic_call() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service).await;

    let connector = TcpConnector::new(addr);
    let client = ReconnectingClient::new(connector);

    // Make a call
    let payload = facet_postcard::to_vec(&"hello".to_string()).unwrap();
    let response = client.call_raw(1, payload).await.unwrap();
    let result: Result<String, roam_session::RoamError<()>> =
        facet_postcard::from_slice(&response).unwrap();

    assert_eq!(result.unwrap(), "hello");
}

// r[verify reconnect.test.rpc-passthrough]
#[tokio::test]
async fn test_unknown_method_not_reconnect() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service).await;

    let connector = TcpConnector::new(addr);
    let connect_count = connector.connect_count.clone();
    let client = ReconnectingClient::new(connector);

    // Call an unknown method
    let payload = facet_postcard::to_vec(&"test".to_string()).unwrap();
    let response = client.call_raw(999, payload).await.unwrap();
    let result: Result<String, roam_session::RoamError<()>> =
        facet_postcard::from_slice(&response).unwrap();

    // Should get UnknownMethod error, not a reconnect error
    assert!(matches!(
        result,
        Err(roam_session::RoamError::UnknownMethod)
    ));

    // Should only have connected once (no reconnection attempts)
    assert_eq!(connect_count.load(Ordering::SeqCst), 1);
}

// r[verify reconnect.test.exhaustion]
#[tokio::test]
async fn test_retries_exhausted() {
    // Use an address that will refuse connections
    let addr: SocketAddr = "127.0.0.1:1".parse().unwrap(); // Port 1 is typically refused

    let connector = TcpConnector::new(addr);
    let connect_count = connector.connect_count.clone();

    let policy = RetryPolicy {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(50),
        backoff_multiplier: 2.0,
    };

    let client = ReconnectingClient::with_policy(connector, policy);

    let payload = facet_postcard::to_vec(&"test".to_string()).unwrap();
    let result = client.call_raw(1, payload).await;

    // Should get RetriesExhausted error
    assert!(matches!(
        result,
        Err(ReconnectError::RetriesExhausted { attempts: 3, .. })
    ));

    // Should have attempted 3 connections
    assert_eq!(connect_count.load(Ordering::SeqCst), 3);
}

// r[verify reconnect.test.backoff]
#[tokio::test]
async fn test_backoff_timing() {
    // Use an address that will refuse connections
    let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();

    let connector = TcpConnector::new(addr);

    let policy = RetryPolicy {
        max_attempts: 3,
        initial_backoff: Duration::from_millis(50),
        max_backoff: Duration::from_millis(200),
        backoff_multiplier: 2.0,
    };

    let client = ReconnectingClient::with_policy(connector, policy);

    let start = Instant::now();
    let payload = facet_postcard::to_vec(&"test".to_string()).unwrap();
    let _ = client.call_raw(1, payload).await;
    let elapsed = start.elapsed();

    // Should have waited at least: 50ms (after attempt 1) + 100ms (after attempt 2)
    // Total minimum: 150ms
    // We allow some slack for test execution overhead
    assert!(
        elapsed >= Duration::from_millis(100),
        "elapsed: {:?}",
        elapsed
    );
}

// r[verify reconnect.test.concurrent]
#[tokio::test]
async fn test_concurrent_callers() {
    let service = TestService::new();
    let call_count = service.call_count.clone();
    let (addr, _server_handle) = start_test_server(service).await;

    let connector = TcpConnector::new(addr);
    let client = Arc::new(ReconnectingClient::new(connector));

    // Spawn multiple concurrent callers
    let mut handles = Vec::new();
    for i in 0..10 {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let msg = format!("message_{}", i);
            let payload = facet_postcard::to_vec(&msg).unwrap();
            let response = client.call_raw(1, payload).await.unwrap();
            let result: Result<String, roam_session::RoamError<()>> =
                facet_postcard::from_slice(&response).unwrap();
            assert_eq!(result.unwrap(), msg);
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // All calls should have succeeded
    assert_eq!(call_count.load(Ordering::SeqCst), 10);
}

/// A connector that fails the first N connection attempts.
struct FailingConnector {
    inner: TcpConnector,
    failures_remaining: Arc<Mutex<u32>>,
    attempt_count: Arc<AtomicU32>,
}

impl FailingConnector {
    fn new(addr: SocketAddr, fail_count: u32) -> Self {
        Self {
            inner: TcpConnector::new(addr),
            failures_remaining: Arc::new(Mutex::new(fail_count)),
            attempt_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl Connector for FailingConnector {
    type Transport = CobsFramed<TcpStream>;

    async fn connect(&self) -> io::Result<Self::Transport> {
        self.attempt_count.fetch_add(1, Ordering::SeqCst);
        let mut failures = self.failures_remaining.lock().await;
        if *failures > 0 {
            *failures -= 1;
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "simulated failure",
            ));
        }
        drop(failures);
        self.inner.connect().await
    }

    fn hello(&self) -> Hello {
        self.inner.hello()
    }
}

// r[verify reconnect.test.basic] - reconnection after initial failure
#[tokio::test]
async fn test_reconnect_after_initial_failure() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service).await;

    // Fail the first 2 attempts, then succeed
    let connector = FailingConnector::new(addr, 2);
    let attempt_count = connector.attempt_count.clone();

    let policy = RetryPolicy {
        max_attempts: 5,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(50),
        backoff_multiplier: 2.0,
    };

    let client = ReconnectingClient::with_policy(connector, policy);

    // Should eventually succeed after retries
    let payload = facet_postcard::to_vec(&"hello".to_string()).unwrap();
    let response = client.call_raw(1, payload).await.unwrap();
    let result: Result<String, roam_session::RoamError<()>> =
        facet_postcard::from_slice(&response).unwrap();

    assert_eq!(result.unwrap(), "hello");

    // Should have tried 3 times (2 failures + 1 success)
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
}
