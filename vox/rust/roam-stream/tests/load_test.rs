//! Load test for stream transport over Unix sockets.
//!
//! Exercises the transport with:
//! - Multiple concurrent connections
//! - High volume of calls
//! - Varying execution times (fast and slow methods)
//! - Random delays to stress concurrency
//! - All calls should eventually complete successfully

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use facet::Facet;
use once_cell::sync::Lazy;
use roam_session::{
    ChannelRegistry, Context, RoamError, RpcPlan, ServiceDispatcher, dispatch_call,
    dispatch_unknown_method,
};
use roam_stream::{ConnectionError, Connector, HandshakeConfig, accept, connect};
use tokio::net::{UnixListener, UnixStream};

// ============================================================================
// RPC Plans
// ============================================================================

static UNIT_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<()>());
static U32_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> = Lazy::new(|| Arc::new(RpcPlan::for_type::<u32>()));

static U32_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<u32>());

static STRING_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<String>());
static STRING_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<String>()));

// ============================================================================
// Test Service with Fast and Slow Methods
// ============================================================================

/// Service with methods that take varying amounts of time to complete.
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

    fn calls(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

const METHOD_INSTANT: u64 = 1;
const METHOD_FAST: u64 = 2;
const METHOD_MEDIUM: u64 = 3;
const METHOD_SLOW: u64 = 4;
const METHOD_VERY_SLOW: u64 = 5;
const METHOD_ECHO: u64 = 6;

impl ServiceDispatcher for TestService {
    fn method_ids(&self) -> Vec<u64> {
        vec![
            METHOD_INSTANT,
            METHOD_FAST,
            METHOD_MEDIUM,
            METHOD_SLOW,
            METHOD_VERY_SLOW,
            METHOD_ECHO,
        ]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        match cx.method_id().raw() {
            // instant() -> u32 - returns immediately
            METHOD_INSTANT => dispatch_call::<(), u32, (), _, _>(
                &cx,
                payload,
                registry,
                &UNIT_ARGS_PLAN,
                U32_RESPONSE_PLAN.clone(),
                |_: ()| async move { Ok(42) },
            ),

            // fast(n: u32) -> u32 - sleeps 1-5ms
            METHOD_FAST => dispatch_call::<u32, u32, (), _, _>(
                &cx,
                payload,
                registry,
                &U32_ARGS_PLAN,
                U32_RESPONSE_PLAN.clone(),
                |n: u32| async move {
                    tokio::time::sleep(Duration::from_millis(1 + (n % 5) as u64)).await;
                    Ok(n * 2)
                },
            ),

            // medium(n: u32) -> u32 - sleeps 10-30ms
            METHOD_MEDIUM => dispatch_call::<u32, u32, (), _, _>(
                &cx,
                payload,
                registry,
                &U32_ARGS_PLAN,
                U32_RESPONSE_PLAN.clone(),
                |n: u32| async move {
                    tokio::time::sleep(Duration::from_millis(10 + (n % 20) as u64)).await;
                    Ok(n * 3)
                },
            ),

            // slow(n: u32) -> u32 - sleeps 50-100ms
            METHOD_SLOW => dispatch_call::<u32, u32, (), _, _>(
                &cx,
                payload,
                registry,
                &U32_ARGS_PLAN,
                U32_RESPONSE_PLAN.clone(),
                |n: u32| async move {
                    tokio::time::sleep(Duration::from_millis(50 + (n % 50) as u64)).await;
                    Ok(n * 4)
                },
            ),

            // very_slow(n: u32) -> u32 - sleeps 100-200ms
            METHOD_VERY_SLOW => dispatch_call::<u32, u32, (), _, _>(
                &cx,
                payload,
                registry,
                &U32_ARGS_PLAN,
                U32_RESPONSE_PLAN.clone(),
                |n: u32| async move {
                    tokio::time::sleep(Duration::from_millis(100 + (n % 100) as u64)).await;
                    Ok(n * 5)
                },
            ),

            // echo(s: String) -> String
            METHOD_ECHO => dispatch_call::<String, String, (), _, _>(
                &cx,
                payload,
                registry,
                &STRING_ARGS_PLAN,
                STRING_RESPONSE_PLAN.clone(),
                |s: String| async move { Ok(s) },
            ),

            _ => dispatch_unknown_method(&cx, registry),
        }
    }
}

// ============================================================================
// Unix Socket Infrastructure
// ============================================================================

struct UnixConnector {
    path: PathBuf,
}

impl Connector for UnixConnector {
    type Transport = UnixStream;

    async fn connect(&self) -> std::io::Result<UnixStream> {
        UnixStream::connect(&self.path).await
    }
}

async fn start_server(
    service: TestService,
) -> Result<(PathBuf, tokio::task::JoinHandle<()>), Box<dyn std::error::Error + Send + Sync>> {
    // Create temp socket path
    let socket_path =
        std::env::temp_dir().join(format!("roam-load-test-{}.sock", std::process::id()));

    // Clean up any leftover socket
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    let path = socket_path.clone();

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let service = service.clone();
                    tokio::spawn(async move {
                        if let Ok((handle, _incoming, driver)) =
                            accept(stream, HandshakeConfig::default(), service).await
                        {
                            let _ = driver.run().await;
                            drop(handle);
                        }
                    });
                }
                Err(_) => break,
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    Ok((path, handle))
}

fn decode_result<T, E>(response: Vec<u8>) -> Result<T, RoamError<E>>
where
    T: for<'a> facet::Facet<'a>,
    E: for<'a> facet::Facet<'a>,
{
    facet_postcard::from_slice::<Result<T, RoamError<E>>>(&response).unwrap()
}

// ============================================================================
// Load Tests
// ============================================================================

/// Basic load test - single connection, many calls of varying speeds
#[tokio::test]
async fn load_test_single_connection_varied_calls() {
    let service = TestService::new();
    let call_count = service.call_count.clone();

    let (socket_path, _server_handle) = start_server(service).await.unwrap();

    // Connect client
    let connector = UnixConnector {
        path: socket_path.clone(),
    };
    let client = connect(connector, HandshakeConfig::default(), TestService::new());
    let handle = client.handle().await.unwrap();

    const TOTAL_CALLS: usize = 500;

    // Launch lots of concurrent calls
    let mut tasks = Vec::new();
    for i in 0..TOTAL_CALLS {
        let handle = handle.clone();
        let task = tokio::spawn(async move {
            // Pick method based on i
            let (method, arg, expected_multiplier) = match i % 6 {
                0 => (METHOD_INSTANT, 0u32, 0u32), // Returns 42
                1 => (METHOD_FAST, i as u32, 2),
                2 => (METHOD_MEDIUM, i as u32, 3),
                3 => (METHOD_SLOW, i as u32, 4),
                4 => (METHOD_VERY_SLOW, i as u32, 5),
                5 => {
                    // ECHO method
                    let msg = format!("message-{}", i);
                    let mut args = msg.clone();
                    let response = handle
                        .call(METHOD_ECHO, &mut args, &STRING_ARGS_PLAN)
                        .await
                        .unwrap();
                    let result: Result<String, RoamError<()>> = decode_result(response.payload);
                    assert_eq!(result.unwrap(), msg);
                    return;
                }
                _ => unreachable!(),
            };

            let mut args = arg;
            let response = handle
                .call(method, &mut args, &U32_ARGS_PLAN)
                .await
                .unwrap();
            let result: Result<u32, RoamError<()>> = decode_result(response.payload);

            if method == METHOD_INSTANT {
                assert_eq!(result.unwrap(), 42);
            } else {
                assert_eq!(result.unwrap(), arg * expected_multiplier);
            }
        });
        tasks.push(task);
    }

    // Wait for all calls to complete
    for task in tasks {
        task.await.unwrap();
    }

    assert_eq!(call_count.load(Ordering::SeqCst), TOTAL_CALLS as u32);

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);
}

/// Load test - multiple connections, concurrent calls
#[tokio::test]
async fn load_test_multiple_connections() {
    let service = TestService::new();
    let call_count = service.call_count.clone();

    let (socket_path, _server_handle) = start_server(service).await.unwrap();

    const NUM_CONNECTIONS: usize = 10;
    const CALLS_PER_CONNECTION: usize = 50;

    // Create multiple client connections
    let mut connection_tasks = Vec::new();
    for conn_id in 0..NUM_CONNECTIONS {
        let socket_path = socket_path.clone();
        let task = tokio::spawn(async move {
            let connector = UnixConnector { path: socket_path };
            let client = connect(connector, HandshakeConfig::default(), TestService::new());
            let handle = client.handle().await.unwrap();

            // Each connection makes many calls
            let mut call_tasks = Vec::new();
            for call_id in 0..CALLS_PER_CONNECTION {
                let handle = handle.clone();
                let i = conn_id * CALLS_PER_CONNECTION + call_id;
                let call_task = tokio::spawn(async move {
                    // Randomly pick a method
                    let method = match i % 5 {
                        0 => METHOD_INSTANT,
                        1 => METHOD_FAST,
                        2 => METHOD_MEDIUM,
                        3 => METHOD_SLOW,
                        4 => METHOD_VERY_SLOW,
                        _ => unreachable!(),
                    };

                    let arg = i as u32;
                    let mut args = arg;
                    let response = handle
                        .call(method, &mut args, &U32_ARGS_PLAN)
                        .await
                        .unwrap();
                    let result: Result<u32, RoamError<()>> = decode_result(response.payload);

                    // Verify we got a response
                    assert!(result.is_ok());
                });
                call_tasks.push(call_task);
            }

            // Wait for all calls from this connection
            for task in call_tasks {
                task.await.unwrap();
            }
        });
        connection_tasks.push(task);
    }

    // Wait for all connections to complete
    for task in connection_tasks {
        task.await.unwrap();
    }

    assert_eq!(
        call_count.load(Ordering::SeqCst),
        (NUM_CONNECTIONS * CALLS_PER_CONNECTION) as u32
    );

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);
}

/// Stress test - bursts of fast and slow calls
#[tokio::test]
async fn load_test_mixed_burst() {
    let service = TestService::new();
    let call_count = service.call_count.clone();

    let (socket_path, _server_handle) = start_server(service).await.unwrap();

    // Create several connections
    let mut clients = Vec::new();
    for _ in 0..5 {
        let connector = UnixConnector {
            path: socket_path.clone(),
        };
        let client = connect(connector, HandshakeConfig::default(), TestService::new());
        let handle = client.handle().await.unwrap();
        clients.push(handle);
    }

    // Launch waves of calls
    let mut all_tasks = Vec::new();
    for wave in 0..10 {
        for (client_idx, client) in clients.iter().enumerate() {
            let client = client.clone();

            // Each client makes a burst of calls
            for burst_idx in 0..20 {
                let client = client.clone();
                let i = (wave * 100) + (client_idx * 20) + burst_idx;

                let task = tokio::spawn(async move {
                    // Mix of methods
                    let method = match i % 4 {
                        0 => METHOD_INSTANT,
                        1 => METHOD_FAST,
                        2 => METHOD_MEDIUM,
                        3 => METHOD_SLOW,
                        _ => unreachable!(),
                    };

                    let arg = i as u32;
                    let mut args = arg;
                    let response = client
                        .call(method, &mut args, &U32_ARGS_PLAN)
                        .await
                        .unwrap();
                    let result: Result<u32, RoamError<()>> = decode_result(response.payload);
                    assert!(result.is_ok());
                });
                all_tasks.push(task);
            }
        }

        // Small delay between waves
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Wait for everything to complete
    for task in all_tasks {
        task.await.unwrap();
    }

    // 5 clients * 20 calls per burst * 10 waves = 1000 total calls
    assert_eq!(call_count.load(Ordering::SeqCst), 1000);

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);
}

/// Ultimate stress test - chaos mode with random everything
#[tokio::test]
async fn load_test_chaos() {
    let service = TestService::new();
    let call_count = service.call_count.clone();

    let (socket_path, _server_handle) = start_server(service).await.unwrap();

    const NUM_CLIENTS: usize = 8;
    const CALLS_PER_CLIENT: usize = 100;

    // Track completed calls
    let completed = Arc::new(AtomicU32::new(0));

    let mut client_tasks = Vec::new();
    for client_id in 0..NUM_CLIENTS {
        let socket_path = socket_path.clone();
        let completed = completed.clone();

        let task = tokio::spawn(async move {
            let connector = UnixConnector { path: socket_path };
            let client = connect(connector, HandshakeConfig::default(), TestService::new());
            let handle = client.handle().await.unwrap();

            let mut call_tasks = Vec::new();
            for call_idx in 0..CALLS_PER_CLIENT {
                let handle = handle.clone();
                let completed = completed.clone();
                let seed = (client_id * 1000 + call_idx) as u32;

                let call_task = tokio::spawn(async move {
                    // Random delay before making call (0-10ms)
                    let delay_ms = (seed % 11) as u64;
                    if delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }

                    // Random method selection
                    let method = match seed % 6 {
                        0 => METHOD_INSTANT,
                        1 => METHOD_FAST,
                        2 => METHOD_MEDIUM,
                        3 => METHOD_SLOW,
                        4 => METHOD_VERY_SLOW,
                        5 => METHOD_ECHO,
                        _ => unreachable!(),
                    };

                    if method == METHOD_ECHO {
                        let msg = format!("chaos-{}", seed);
                        let mut args = msg.clone();
                        let response = handle
                            .call(method, &mut args, &STRING_ARGS_PLAN)
                            .await
                            .unwrap();
                        let result: Result<String, RoamError<()>> = decode_result(response.payload);
                        assert_eq!(result.unwrap(), msg);
                    } else {
                        let mut args = seed;
                        let response = handle
                            .call(method, &mut args, &U32_ARGS_PLAN)
                            .await
                            .unwrap();
                        let result: Result<u32, RoamError<()>> = decode_result(response.payload);
                        assert!(result.is_ok());
                    }

                    completed.fetch_add(1, Ordering::SeqCst);
                });
                call_tasks.push(call_task);
            }

            // Wait for all calls from this client
            for task in call_tasks {
                task.await.unwrap();
            }
        });
        client_tasks.push(task);
    }

    // Wait for all clients to complete
    for task in client_tasks {
        task.await.unwrap();
    }

    let total_expected = (NUM_CLIENTS * CALLS_PER_CLIENT) as u32;
    assert_eq!(completed.load(Ordering::SeqCst), total_expected);
    assert_eq!(call_count.load(Ordering::SeqCst), total_expected);

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);
}
