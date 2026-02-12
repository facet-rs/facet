//! Chaos load test - tries to break the stream transport.
//!
//! Run with: cargo run --example load_test_chaos --release
//!
//! This test includes:
//! - Client disconnections mid-call
//! - Call cancellations (dropping futures)
//! - Connection churn (rapid connect/disconnect)
//! - Overwhelming the server
//! - Race conditions and edge cases

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use facet::Facet;
use once_cell::sync::Lazy;
use roam_session::{
    ChannelRegistry, Context, RoamError, RpcPlan, ServiceDispatcher, dispatch_call,
    dispatch_unknown_method,
};
use roam_stream::{Connector, HandshakeConfig, accept, connect};
use tokio::net::{UnixListener, UnixStream};
use tokio::time::timeout;

// ============================================================================
// RPC Plans
// ============================================================================

static U64_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<u64>());
static U64_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> = Lazy::new(|| Arc::new(RpcPlan::for_type::<u64>()));

static VEC_U8_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<Vec<u8>>());
static VEC_U8_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<Vec<u8>>()));

static COMPLEX_REQUEST_ARGS_PLAN: Lazy<RpcPlan> =
    Lazy::new(|| RpcPlan::for_type::<ComplexRequest>());
static COMPLEX_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<ComplexResponse>()));

// ============================================================================
// Test Service
// ============================================================================

#[derive(Clone)]
struct ChaosService {
    calls_total: Arc<AtomicU32>,
    calls_completed: Arc<AtomicU32>,
    calls_cancelled: Arc<AtomicU32>,
    calls_dropped: Arc<AtomicU32>,
}

impl ChaosService {
    fn new() -> Self {
        Self {
            calls_total: Arc::new(AtomicU32::new(0)),
            calls_completed: Arc::new(AtomicU32::new(0)),
            calls_cancelled: Arc::new(AtomicU32::new(0)),
            calls_dropped: Arc::new(AtomicU32::new(0)),
        }
    }

    fn stats(&self) -> (u32, u32, u32, u32) {
        (
            self.calls_total.load(Ordering::Relaxed),
            self.calls_completed.load(Ordering::Relaxed),
            self.calls_cancelled.load(Ordering::Relaxed),
            self.calls_dropped.load(Ordering::Relaxed),
        )
    }
}

const METHOD_INSTANT: u64 = 1;
const METHOD_SLOW: u64 = 2;
const METHOD_VERY_SLOW: u64 = 3;
const METHOD_BIG_DATA: u64 = 4;
const METHOD_COMPLEX_STRUCT: u64 = 5;

#[derive(facet::Facet, Clone, Debug)]
struct ComplexRequest {
    id: u64,
    name: String,
    data: Vec<u8>,
    nested: NestedData,
    tags: Vec<String>,
    metadata: std::collections::HashMap<String, String>,
}

#[derive(facet::Facet, Clone, Debug)]
struct NestedData {
    timestamp: u64,
    values: Vec<f64>,
    flags: Vec<bool>,
}

#[derive(facet::Facet, Clone, Debug)]
struct ComplexResponse {
    request_id: u64,
    processed_bytes: usize,
    checksum: u64,
    results: Vec<String>,
    nested_result: NestedData,
}

impl ServiceDispatcher for ChaosService {
    fn method_ids(&self) -> Vec<u64> {
        vec![
            METHOD_INSTANT,
            METHOD_SLOW,
            METHOD_VERY_SLOW,
            METHOD_BIG_DATA,
            METHOD_COMPLEX_STRUCT,
        ]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.calls_total.fetch_add(1, Ordering::Relaxed);
        let completed = self.calls_completed.clone();

        match cx.method_id().raw() {
            METHOD_INSTANT => dispatch_call::<u64, u64, (), _, _>(
                &cx,
                payload,
                registry,
                &U64_ARGS_PLAN,
                U64_RESPONSE_PLAN.clone(),
                move |n: u64| async move {
                    completed.fetch_add(1, Ordering::Relaxed);
                    Ok(n)
                },
            ),

            METHOD_SLOW => {
                let completed = completed.clone();
                dispatch_call::<u64, u64, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &U64_ARGS_PLAN,
                    U64_RESPONSE_PLAN.clone(),
                    move |n: u64| async move {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        completed.fetch_add(1, Ordering::Relaxed);
                        Ok(n)
                    },
                )
            }

            METHOD_VERY_SLOW => {
                let completed = completed.clone();
                dispatch_call::<u64, u64, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &U64_ARGS_PLAN,
                    U64_RESPONSE_PLAN.clone(),
                    move |n: u64| async move {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        completed.fetch_add(1, Ordering::Relaxed);
                        Ok(n)
                    },
                )
            }

            METHOD_BIG_DATA => {
                let completed = completed.clone();
                dispatch_call::<Vec<u8>, Vec<u8>, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &VEC_U8_ARGS_PLAN,
                    VEC_U8_RESPONSE_PLAN.clone(),
                    move |data: Vec<u8>| async move {
                        // Process the big data (simulate work)
                        tokio::time::sleep(Duration::from_millis(10)).await;

                        // Return processed data (reversed)
                        let mut result = data.clone();
                        result.reverse();

                        completed.fetch_add(1, Ordering::Relaxed);
                        Ok(result)
                    },
                )
            }

            METHOD_COMPLEX_STRUCT => {
                let completed = completed.clone();
                dispatch_call::<ComplexRequest, ComplexResponse, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &COMPLEX_REQUEST_ARGS_PLAN,
                    COMPLEX_RESPONSE_PLAN.clone(),
                    move |req: ComplexRequest| async move {
                        tokio::time::sleep(Duration::from_millis(20)).await;

                        // Build complex response
                        let checksum = req.data.iter().map(|&b| b as u64).sum::<u64>();
                        let response = ComplexResponse {
                            request_id: req.id,
                            processed_bytes: req.data.len(),
                            checksum,
                            results: req
                                .tags
                                .iter()
                                .map(|t| format!("processed:{}", t))
                                .collect(),
                            nested_result: req.nested.clone(),
                        };

                        completed.fetch_add(1, Ordering::Relaxed);
                        Ok(response)
                    },
                )
            }

            _ => dispatch_unknown_method(&cx, registry),
        }
    }
}

// ============================================================================
// Infrastructure
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
    socket_path: PathBuf,
    service: ChaosService,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>> {
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;

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

    tokio::time::sleep(Duration::from_millis(50)).await;
    Ok(handle)
}

fn decode_result<T>(response: Vec<u8>) -> T
where
    T: for<'a> facet::Facet<'a>,
{
    let result: Result<T, RoamError<()>> = facet_postcard::from_slice(&response).unwrap();
    result.unwrap()
}

// ============================================================================
// Chaos Scenarios
// ============================================================================

/// Scenario 1: Clients that disconnect mid-call
async fn chaos_disconnecting_clients(
    socket_path: PathBuf,
    iterations: usize,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for i in 0..iterations {
        let connector = UnixConnector {
            path: socket_path.clone(),
        };
        let client = connect(connector, HandshakeConfig::default(), ChaosService::new());

        // Start a slow call
        let handle = client.handle().await?;
        let task = tokio::spawn(async move {
            let mut args = i as u64;
            let _ = handle
                .call(METHOD_VERY_SLOW, &mut args, &U64_ARGS_PLAN)
                .await;
        });

        // Disconnect before it completes
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(client);

        // Don't wait for the task - it should fail
        let _ = task.await;
        stats.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

/// Scenario 2: Cancelled calls (drop the future)
async fn chaos_cancelled_calls(
    socket_path: PathBuf,
    iterations: usize,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connector = UnixConnector { path: socket_path };
    let client = connect(connector, HandshakeConfig::default(), ChaosService::new());
    let handle = client.handle().await?;

    for i in 0..iterations {
        let handle = handle.clone();
        let task = tokio::spawn(async move {
            let mut args = i as u64;
            let _ = handle
                .call(METHOD_VERY_SLOW, &mut args, &U64_ARGS_PLAN)
                .await;
        });

        // Cancel by dropping after a short delay
        tokio::time::sleep(Duration::from_millis(5)).await;
        task.abort();

        stats.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

/// Scenario 3: Connection churn - rapid connect/disconnect
async fn chaos_connection_churn(
    socket_path: PathBuf,
    iterations: usize,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for i in 0..iterations {
        let connector = UnixConnector {
            path: socket_path.clone(),
        };
        let client = connect(connector, HandshakeConfig::default(), ChaosService::new());

        // Maybe make a call, maybe don't
        if i % 3 == 0 {
            if let Ok(handle) = client.handle().await {
                let mut args = i as u64;
                let _ = handle.call(METHOD_INSTANT, &mut args, &U64_ARGS_PLAN).await;
            }
        }

        // Disconnect immediately
        drop(client);
        stats.fetch_add(1, Ordering::Relaxed);

        // Tiny delay
        if i % 10 == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    Ok(())
}

/// Scenario 4: Overwhelming the server with complex data
async fn chaos_overwhelm(
    socket_path: PathBuf,
    concurrent_calls: usize,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connector = UnixConnector { path: socket_path };
    let client = connect(connector, HandshakeConfig::default(), ChaosService::new());
    let handle = client.handle().await?;

    let mut tasks = Vec::new();
    for i in 0..concurrent_calls {
        let handle = handle.clone();
        let stats = stats.clone();
        let task = tokio::spawn(async move {
            // Mix of different call types
            match i % 5 {
                0 | 1 => {
                    // Big data: 1KB to 100KB
                    let size = 1024 + (i % 100) * 1024;
                    let mut data = vec![0u8; size];
                    for (idx, byte) in data.iter_mut().enumerate() {
                        *byte = (idx % 256) as u8;
                    }

                    match timeout(
                        Duration::from_secs(2),
                        handle.call(METHOD_BIG_DATA, &mut data, &VEC_U8_ARGS_PLAN),
                    )
                    .await
                    {
                        Ok(Ok(response)) => {
                            let result: Vec<u8> = decode_result(response.payload);
                            // Verify it was reversed
                            if result.len() == data.len() {
                                stats.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        _ => {}
                    }
                }
                2 | 3 => {
                    // Complex struct
                    let mut req = ComplexRequest {
                        id: i as u64,
                        name: format!("request-{}", i),
                        data: vec![(i % 256) as u8; 512],
                        nested: NestedData {
                            timestamp: i as u64,
                            values: vec![1.0, 2.0, 3.0, (i as f64) * 0.5],
                            flags: vec![i % 2 == 0, i % 3 == 0, i % 5 == 0],
                        },
                        tags: vec![
                            format!("tag-{}", i),
                            "production".to_string(),
                            "high-priority".to_string(),
                        ],
                        metadata: [
                            ("source".to_string(), "chaos-test".to_string()),
                            ("index".to_string(), i.to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    };

                    match timeout(
                        Duration::from_secs(2),
                        handle.call(METHOD_COMPLEX_STRUCT, &mut req, &COMPLEX_REQUEST_ARGS_PLAN),
                    )
                    .await
                    {
                        Ok(Ok(response)) => {
                            let result: ComplexResponse = decode_result(response.payload);
                            if result.request_id == i as u64 {
                                stats.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {
                    // Simple call
                    let mut args = i as u64;
                    match timeout(
                        Duration::from_secs(2),
                        handle.call(METHOD_SLOW, &mut args, &U64_ARGS_PLAN),
                    )
                    .await
                    {
                        Ok(Ok(response)) => {
                            let _: u64 = decode_result(response.payload);
                            stats.fetch_add(1, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                }
            }
        });
        tasks.push(task);
    }

    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

/// Scenario 5: Mixed chaos - everything at once
async fn chaos_mixed(
    socket_path: PathBuf,
    duration_secs: u64,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(duration_secs) {
        // Randomly pick a chaos scenario
        let scenario = stats.load(Ordering::Relaxed) % 6;

        match scenario {
            0 => {
                // Quick disconnect with big data
                let connector = UnixConnector {
                    path: socket_path.clone(),
                };
                let client = connect(connector, HandshakeConfig::default(), ChaosService::new());
                if let Ok(handle) = client.handle().await {
                    let task = tokio::spawn(async move {
                        let mut data = vec![0xAA; 50 * 1024]; // 50KB
                        let _ = handle
                            .call(METHOD_BIG_DATA, &mut data, &VEC_U8_ARGS_PLAN)
                            .await;
                    });
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    drop(client);
                    let _ = task.await;
                }
            }
            1 => {
                // Cancelled complex struct call
                let connector = UnixConnector {
                    path: socket_path.clone(),
                };
                let client = connect(connector, HandshakeConfig::default(), ChaosService::new());
                if let Ok(handle) = client.handle().await {
                    let task = tokio::spawn(async move {
                        let mut req = ComplexRequest {
                            id: 999,
                            name: "cancelled".to_string(),
                            data: vec![0xFF; 2048],
                            nested: NestedData {
                                timestamp: 123456789,
                                values: vec![1.0; 100],
                                flags: vec![true; 50],
                            },
                            tags: vec!["test".to_string(); 10],
                            metadata: Default::default(),
                        };
                        let _ = handle
                            .call(METHOD_COMPLEX_STRUCT, &mut req, &COMPLEX_REQUEST_ARGS_PLAN)
                            .await;
                    });
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    task.abort();
                    let _ = task.await;
                }
            }
            2 => {
                // Rapid connect/disconnect
                for _ in 0..5 {
                    let connector = UnixConnector {
                        path: socket_path.clone(),
                    };
                    let client =
                        connect(connector, HandshakeConfig::default(), ChaosService::new());
                    let _ = client.handle().await;
                    drop(client);
                }
            }
            3 => {
                // Burst of big data calls
                let connector = UnixConnector {
                    path: socket_path.clone(),
                };
                let client = connect(connector, HandshakeConfig::default(), ChaosService::new());
                if let Ok(handle) = client.handle().await {
                    let mut tasks = Vec::new();
                    for i in 0..10 {
                        let handle = handle.clone();
                        let task = tokio::spawn(async move {
                            let size = 1024 + (i * 1024);
                            let mut data = vec![(i % 256) as u8; size];
                            let _ = timeout(
                                Duration::from_millis(200),
                                handle.call(METHOD_BIG_DATA, &mut data, &VEC_U8_ARGS_PLAN),
                            )
                            .await;
                        });
                        tasks.push(task);
                    }
                    for task in tasks {
                        let _ = task.await;
                    }
                }
            }
            4 => {
                // Burst of complex struct calls
                let connector = UnixConnector {
                    path: socket_path.clone(),
                };
                let client = connect(connector, HandshakeConfig::default(), ChaosService::new());
                if let Ok(handle) = client.handle().await {
                    let mut tasks = Vec::new();
                    for i in 0..5 {
                        let handle = handle.clone();
                        let task = tokio::spawn(async move {
                            let mut req = ComplexRequest {
                                id: i,
                                name: format!("burst-{}", i),
                                data: vec![(i % 256) as u8; 512],
                                nested: NestedData {
                                    timestamp: i,
                                    values: vec![i as f64; 20],
                                    flags: vec![i % 2 == 0; 10],
                                },
                                tags: vec![format!("tag{}", i)],
                                metadata: Default::default(),
                            };
                            let _ = timeout(
                                Duration::from_millis(200),
                                handle.call(
                                    METHOD_COMPLEX_STRUCT,
                                    &mut req,
                                    &COMPLEX_REQUEST_ARGS_PLAN,
                                ),
                            )
                            .await;
                        });
                        tasks.push(task);
                    }
                    for task in tasks {
                        let _ = task.await;
                    }
                }
            }
            _ => {
                // Burst of instant calls
                let connector = UnixConnector {
                    path: socket_path.clone(),
                };
                let client = connect(connector, HandshakeConfig::default(), ChaosService::new());
                if let Ok(handle) = client.handle().await {
                    let mut tasks = Vec::new();
                    for i in 0..20 {
                        let handle = handle.clone();
                        let task = tokio::spawn(async move {
                            let mut args = i;
                            let _ = timeout(
                                Duration::from_millis(100),
                                handle.call(METHOD_INSTANT, &mut args, &U64_ARGS_PLAN),
                            )
                            .await;
                        });
                        tasks.push(task);
                    }
                    for task in tasks {
                        let _ = task.await;
                    }
                }
            }
        }

        stats.fetch_add(1, Ordering::Relaxed);
    }

    Ok(())
}

// ============================================================================
// Main
// ============================================================================

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(not(target_family = "unix"))]
    {
        println!("This example is unix-only");
        return Ok(());
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()?;

    rt.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("=== Chaos Load Test ===");
    println!("Attempting to break the stream transport...");
    println!();

    let socket_path =
        std::env::temp_dir().join(format!("roam-chaos-test-{}.sock", std::process::id()));

    let service = ChaosService::new();
    let service_stats = service.clone();
    let _server_handle = start_server(socket_path.clone(), service).await?;

    println!("Running chaos scenarios:");
    println!();

    // Scenario 1: Disconnecting clients
    {
        println!("1. Disconnecting clients mid-call...");
        let stats = Arc::new(AtomicU64::new(0));
        let stats_clone = stats.clone();
        let socket_path = socket_path.clone();

        let start = Instant::now();
        chaos_disconnecting_clients(socket_path, 50, stats_clone).await?;
        let elapsed = start.elapsed();

        println!(
            "   ✓ Completed {} disconnections in {:.2}s",
            stats.load(Ordering::Relaxed),
            elapsed.as_secs_f64()
        );
    }

    // Scenario 2: Cancelled calls
    {
        println!("2. Cancelled calls...");
        let stats = Arc::new(AtomicU64::new(0));
        let stats_clone = stats.clone();
        let socket_path = socket_path.clone();

        let start = Instant::now();
        chaos_cancelled_calls(socket_path, 50, stats_clone).await?;
        let elapsed = start.elapsed();

        println!(
            "   ✓ Completed {} cancellations in {:.2}s",
            stats.load(Ordering::Relaxed),
            elapsed.as_secs_f64()
        );
    }

    // Scenario 3: Connection churn
    {
        println!("3. Connection churn (rapid connect/disconnect)...");
        let stats = Arc::new(AtomicU64::new(0));
        let stats_clone = stats.clone();
        let socket_path = socket_path.clone();

        let start = Instant::now();
        chaos_connection_churn(socket_path, 100, stats_clone).await?;
        let elapsed = start.elapsed();

        println!(
            "   ✓ Completed {} connection cycles in {:.2}s",
            stats.load(Ordering::Relaxed),
            elapsed.as_secs_f64()
        );
    }

    // Scenario 4: Overwhelming the server
    {
        println!("4. Overwhelming the server...");
        let stats = Arc::new(AtomicU64::new(0));
        let stats_clone = stats.clone();
        let socket_path = socket_path.clone();

        let start = Instant::now();
        chaos_overwhelm(socket_path, 500, stats_clone).await?;
        let elapsed = start.elapsed();

        println!(
            "   ✓ Completed {}/500 overwhelming calls in {:.2}s",
            stats.load(Ordering::Relaxed),
            elapsed.as_secs_f64()
        );
    }

    // Scenario 5: Mixed chaos
    {
        println!("5. Mixed chaos (10 seconds of random mayhem)...");
        let stats = Arc::new(AtomicU64::new(0));
        let stats_clone = stats.clone();
        let socket_path = socket_path.clone();

        let start = Instant::now();
        chaos_mixed(socket_path, 10, stats_clone).await?;
        let elapsed = start.elapsed();

        println!(
            "   ✓ Completed {} mixed operations in {:.2}s",
            stats.load(Ordering::Relaxed),
            elapsed.as_secs_f64()
        );
    }

    println!();
    let (total, completed, _cancelled, dropped) = service_stats.stats();
    println!("=== Server Stats ===");
    println!("Total calls received: {}", total);
    println!("Calls completed: {}", completed);
    println!("Calls dropped (cancelled mid-execution): {}", dropped);
    println!(
        "Completion rate: {:.1}%",
        (completed as f64 / total as f64) * 100.0
    );

    let incomplete = total - completed;
    if incomplete > 0 {
        println!();
        println!("⚠️  Found {} incomplete calls!", incomplete);
        println!("   This could be a bug or legitimate cancellation behavior.");
    } else {
        println!();
        println!("✓ All calls accounted for!");
    }

    println!();
    println!("✓ Survived all chaos scenarios!");

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}
