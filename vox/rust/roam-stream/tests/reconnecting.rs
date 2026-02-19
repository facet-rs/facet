//! Integration tests for the reconnecting Client.
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

use once_cell::sync::Lazy;
use roam_session::{
    Caller, ChannelRegistry, Context, RpcPlan, Rx, ServiceDispatcher, dispatch_call,
    dispatch_unknown_method,
};
use roam_stream::{
    ConnectError, Connector, HandshakeConfig, RetryPolicy, accept, connect, connect_with_policy,
};
use std::sync::Mutex;
use tokio::net::TcpStream;

// ============================================================================
// RPC Plans
// ============================================================================

static STRING_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(RpcPlan::for_type::<String>);
static STRING_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<String>()));

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
    fn method_descriptor(&self, _method_id: u64) -> Option<roam_session::MethodDescriptor> {
        None
    }

    fn method_ids(&self) -> Vec<u64> {
        vec![1]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        match cx.method_id().raw() {
            // Echo method
            1 => dispatch_call::<String, String, (), _, _>(
                &cx,
                payload,
                registry,
                &STRING_ARGS_PLAN,
                STRING_RESPONSE_PLAN.clone(),
                |input: String| async move { Ok(input) },
            ),
            _ => dispatch_unknown_method(&cx, registry),
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
    type Transport = TcpStream;

    async fn connect(&self) -> io::Result<Self::Transport> {
        self.connect_count.fetch_add(1, Ordering::SeqCst);
        // Add timeout to handle Windows where refused connections may hang
        match tokio::time::timeout(Duration::from_millis(500), TcpStream::connect(self.addr)).await
        {
            Ok(result) => result,
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "connection timed out",
            )),
        }
    }
}

/// Helper to start a test server and return its address.
async fn start_dispatcher_server<D>(dispatcher: D) -> (SocketAddr, tokio::task::JoinHandle<()>)
where
    D: ServiceDispatcher + Clone + Send + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let dispatcher = dispatcher.clone();
            tokio::spawn(async move {
                if let Ok((handle, _incoming, driver)) =
                    accept(stream, HandshakeConfig::default(), dispatcher).await
                {
                    let _ = driver.run().await;
                    let _ = handle;
                }
            });
        }
    });

    // Give the server time to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    (addr, handle)
}

/// Helper to start a test server and return its address.
async fn start_test_server(service: TestService) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    start_dispatcher_server(service).await
}

// r[verify reconnect.test.lazy]
#[tokio::test]
async fn test_lazy_connection() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service.clone()).await;

    let connector = TcpConnector::new(addr);
    let connect_count = connector.connect_count.clone();

    // Create client - should NOT connect yet
    let _client = connect(connector, HandshakeConfig::default(), service);

    // Verify no connection was made
    assert_eq!(connect_count.load(Ordering::SeqCst), 0);
}

// r[verify reconnect.test.basic]
#[tokio::test]
async fn test_basic_call() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service.clone()).await;

    let connector = TcpConnector::new(addr);
    let client = connect(connector, HandshakeConfig::default(), service);

    // Make a call
    let payload = facet_postcard::to_vec(&"hello".to_string()).unwrap();
    let response = client.call_raw(1, "test", payload).await.unwrap();
    let result: Result<String, roam_session::RoamError<()>> =
        facet_postcard::from_slice(&response).unwrap();

    assert_eq!(result.unwrap(), "hello");
}

// r[verify reconnect.test.rpc-passthrough]
#[tokio::test]
async fn test_unknown_method_not_reconnect() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service.clone()).await;

    let connector = TcpConnector::new(addr);
    let connect_count = connector.connect_count.clone();
    let client = connect(connector, HandshakeConfig::default(), service);

    // Call an unknown method
    let payload = facet_postcard::to_vec(&"test".to_string()).unwrap();
    let response = client.call_raw(999, "test", payload).await.unwrap();
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

    let service = TestService::new();
    let client = connect_with_policy(connector, HandshakeConfig::default(), service, policy);

    let payload = facet_postcard::to_vec(&"test".to_string()).unwrap();
    let result = client.call_raw(1, "test", payload).await;

    // Should get RetriesExhausted error
    assert!(matches!(
        result,
        Err(ConnectError::RetriesExhausted { attempts: 3, .. })
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

    let service = TestService::new();
    let client = connect_with_policy(connector, HandshakeConfig::default(), service, policy);

    let start = Instant::now();
    let payload = facet_postcard::to_vec(&"test".to_string()).unwrap();
    let _ = client.call_raw(1, "test", payload).await;
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
    let (addr, _server_handle) = start_test_server(service.clone()).await;

    let connector = TcpConnector::new(addr);
    let client = Arc::new(connect(connector, HandshakeConfig::default(), service));

    // Spawn multiple concurrent callers
    let mut handles = Vec::new();
    for i in 0..10 {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let msg = format!("message_{}", i);
            let payload = facet_postcard::to_vec(&msg).unwrap();
            let response = client.call_raw(1, "test", payload).await.unwrap();
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
    type Transport = TcpStream;

    async fn connect(&self) -> io::Result<Self::Transport> {
        self.attempt_count.fetch_add(1, Ordering::SeqCst);
        {
            let mut failures = self.failures_remaining.lock().unwrap();
            if *failures > 0 {
                *failures -= 1;
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    "simulated failure",
                ));
            }
        }
        self.inner.connect().await
    }
}

// r[verify reconnect.test.basic] - reconnection after initial failure
#[tokio::test]
async fn test_reconnect_after_initial_failure() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service.clone()).await;

    // Fail the first 2 attempts, then succeed
    let connector = FailingConnector::new(addr, 2);
    let attempt_count = connector.attempt_count.clone();

    let policy = RetryPolicy {
        max_attempts: 5,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(50),
        backoff_multiplier: 2.0,
    };

    let client = connect_with_policy(connector, HandshakeConfig::default(), service, policy);

    // Should eventually succeed after retries
    let payload = facet_postcard::to_vec(&"hello".to_string()).unwrap();
    let response = client.call_raw(1, "test", payload).await.unwrap();
    let result: Result<String, roam_session::RoamError<()>> =
        facet_postcard::from_slice(&response).unwrap();

    assert_eq!(result.unwrap(), "hello");

    // Should have tried 3 times (2 failures + 1 success)
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_generated_client_binds_response_channels() {
    let service = TestService::new();
    let (addr, _server_handle) = start_test_server(service.clone()).await;

    let connector = TcpConnector::new(addr);
    let client = connect(connector, HandshakeConfig::default(), service);
    client.handle().await.unwrap();

    let mut response = Rx::<u32>::try_from(700u64).unwrap();
    assert!(response.receiver.is_none());
    let rx_u32_plan = RpcPlan::for_type::<Rx<u32>>();
    Caller::bind_response_channels(&client, &mut response, &rx_u32_plan, &[700u64]);
    assert!(response.receiver.is_some());

    let mut response_by_plan = Rx::<u32>::try_from(701u64).unwrap();
    assert!(response_by_plan.receiver.is_none());
    let plan = RpcPlan::for_type::<Rx<u32>>();
    // SAFETY: response_by_plan points to a valid Rx<u32>, matching plan.
    unsafe {
        Caller::bind_response_channels_by_plan(
            &client,
            (&raw mut response_by_plan).cast::<()>(),
            &plan,
            &[701u64],
        );
    }
    assert!(response_by_plan.receiver.is_some());
}
