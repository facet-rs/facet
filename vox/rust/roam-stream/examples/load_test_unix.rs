//! Load test for stream transport over Unix sockets.
//!
//! Run with: cargo run --example load_test_unix --release
//!
//! This creates a server and multiple clients, then hammers the connection
//! with lots of concurrent calls with varying execution times.

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

// ============================================================================
// RPC Plans
// ============================================================================

static U64_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<u64>());
static U64_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> = Lazy::new(|| Arc::new(RpcPlan::for_type::<u64>()));

// ============================================================================
// Test Service with Fast and Slow Methods
// ============================================================================

#[derive(Clone)]
struct LoadTestService {
    calls_total: Arc<AtomicU32>,
    calls_instant: Arc<AtomicU32>,
    calls_fast: Arc<AtomicU32>,
    calls_medium: Arc<AtomicU32>,
    calls_slow: Arc<AtomicU32>,
}

impl LoadTestService {
    fn new() -> Self {
        Self {
            calls_total: Arc::new(AtomicU32::new(0)),
            calls_instant: Arc::new(AtomicU32::new(0)),
            calls_fast: Arc::new(AtomicU32::new(0)),
            calls_medium: Arc::new(AtomicU32::new(0)),
            calls_slow: Arc::new(AtomicU32::new(0)),
        }
    }

    fn total_calls(&self) -> u32 {
        self.calls_total.load(Ordering::Relaxed)
    }

    fn print_stats(&self) {
        println!(
            "  Total: {} | Instant: {} | Fast: {} | Medium: {} | Slow: {}",
            self.calls_total.load(Ordering::Relaxed),
            self.calls_instant.load(Ordering::Relaxed),
            self.calls_fast.load(Ordering::Relaxed),
            self.calls_medium.load(Ordering::Relaxed),
            self.calls_slow.load(Ordering::Relaxed),
        );
    }
}

const METHOD_INSTANT: u64 = 1;
const METHOD_FAST: u64 = 2;
const METHOD_MEDIUM: u64 = 3;
const METHOD_SLOW: u64 = 4;

impl ServiceDispatcher for LoadTestService {
    fn method_ids(&self) -> Vec<u64> {
        vec![METHOD_INSTANT, METHOD_FAST, METHOD_MEDIUM, METHOD_SLOW]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.calls_total.fetch_add(1, Ordering::Relaxed);

        match cx.method_id().raw() {
            // instant(n: u64) -> u64
            METHOD_INSTANT => {
                self.calls_instant.fetch_add(1, Ordering::Relaxed);
                dispatch_call::<u64, u64, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &U64_ARGS_PLAN,
                    U64_RESPONSE_PLAN.clone(),
                    |n: u64| async move { Ok(n.wrapping_mul(2)) },
                )
            }

            // fast(n: u64) -> u64 - sleeps 1-5ms
            METHOD_FAST => {
                self.calls_fast.fetch_add(1, Ordering::Relaxed);
                dispatch_call::<u64, u64, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &U64_ARGS_PLAN,
                    U64_RESPONSE_PLAN.clone(),
                    |n: u64| async move {
                        tokio::time::sleep(Duration::from_millis(1 + (n % 5))).await;
                        Ok(n.wrapping_mul(3))
                    },
                )
            }

            // medium(n: u64) -> u64 - sleeps 10-30ms
            METHOD_MEDIUM => {
                self.calls_medium.fetch_add(1, Ordering::Relaxed);
                dispatch_call::<u64, u64, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &U64_ARGS_PLAN,
                    U64_RESPONSE_PLAN.clone(),
                    |n: u64| async move {
                        tokio::time::sleep(Duration::from_millis(10 + (n % 20))).await;
                        Ok(n.wrapping_mul(4))
                    },
                )
            }

            // slow(n: u64) -> u64 - sleeps 50-100ms
            METHOD_SLOW => {
                self.calls_slow.fetch_add(1, Ordering::Relaxed);
                dispatch_call::<u64, u64, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &U64_ARGS_PLAN,
                    U64_RESPONSE_PLAN.clone(),
                    |n: u64| async move {
                        tokio::time::sleep(Duration::from_millis(50 + (n % 50))).await;
                        Ok(n.wrapping_mul(5))
                    },
                )
            }

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
    socket_path: PathBuf,
    service: LoadTestService,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>> {
    // Clean up any leftover socket
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    println!("Server listening on {}", socket_path.display());

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
    tokio::time::sleep(Duration::from_millis(50)).await;

    Ok(handle)
}

fn decode_result<T>(response: Vec<u8>) -> T
where
    T: for<'a> facet::Facet<'a>,
{
    let result: Result<T, RoamError<()>> =
        facet_postcard::from_slice(&response).expect("Failed to decode response");
    result.expect("RPC call returned error")
}

// ============================================================================
// Load Test Scenarios
// ============================================================================

async fn run_client_worker(
    client_id: usize,
    socket_path: PathBuf,
    calls_per_client: usize,
    completed: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Connect to server
    let connector = UnixConnector { path: socket_path };
    let client = connect(
        connector,
        HandshakeConfig::default(),
        LoadTestService::new(),
    );
    let handle = client.handle().await?;

    // Launch concurrent calls
    let mut tasks = Vec::new();
    for i in 0..calls_per_client {
        let handle = handle.clone();
        let completed = completed.clone();
        let seed = (client_id * 10000 + i) as u64;

        let task = tokio::spawn(async move {
            // Random delay before making call (0-10ms)
            let delay_ms = seed % 11;
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }

            // Pick method based on seed
            let (method, multiplier) = match seed % 10 {
                0..=4 => (METHOD_INSTANT, 2), // 50% instant
                5..=7 => (METHOD_FAST, 3),    // 30% fast
                8 => (METHOD_MEDIUM, 4),      // 10% medium
                9 => (METHOD_SLOW, 5),        // 10% slow
                _ => unreachable!(),
            };

            // Make the call
            let mut args = seed;
            let response = handle
                .call(method, &mut args, &U64_ARGS_PLAN)
                .await
                .expect("Call failed");
            let result: u64 = decode_result(response.payload);

            // Verify result
            let expected = seed.wrapping_mul(multiplier);
            assert_eq!(result, expected, "Wrong result for call {}", seed);

            completed.fetch_add(1, Ordering::Relaxed);
        });
        tasks.push(task);
    }

    // Wait for all calls to complete
    for task in tasks {
        task.await?;
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
        .enable_all()
        .build()?;

    rt.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Configuration
    let num_clients = 20;
    let calls_per_client = 100;
    let total_calls = num_clients * calls_per_client;

    println!("=== Unix Socket Load Test ===");
    println!("Clients: {}", num_clients);
    println!("Calls per client: {}", calls_per_client);
    println!("Total calls: {}", total_calls);
    println!("Mix: 50% instant, 30% fast (1-5ms), 10% medium (10-30ms), 10% slow (50-100ms)");
    println!();

    // Create temp socket path
    let socket_path =
        std::env::temp_dir().join(format!("roam-load-test-{}.sock", std::process::id()));

    // Start server
    let service = LoadTestService::new();
    let service_stats = service.clone();
    let service_stats_final = service.clone();
    let _server_handle = start_server(socket_path.clone(), service).await?;

    // Track completed calls
    let completed = Arc::new(AtomicU64::new(0));
    let completed_stats = completed.clone();

    // Spawn stats reporter
    let stats_handle = tokio::spawn(async move {
        let mut last_count = 0u64;
        let mut last_instant = Instant::now();

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let current_count = completed_stats.load(Ordering::Relaxed);
            let now = Instant::now();
            let elapsed = now.duration_since(last_instant).as_secs_f64();
            let rate = (current_count - last_count) as f64 / elapsed;

            print!(
                "\rProgress: {}/{} calls | Rate: {:.0} calls/sec | ",
                current_count, total_calls, rate
            );
            service_stats.print_stats();
            std::io::Write::flush(&mut std::io::stdout()).ok();

            last_count = current_count;
            last_instant = now;

            if current_count >= total_calls as u64 {
                break;
            }
        }
    });

    // Launch all clients
    println!("Launching clients...");
    let start = Instant::now();

    let mut client_tasks = Vec::new();
    for client_id in 0..num_clients {
        let socket_path = socket_path.clone();
        let completed = completed.clone();

        let task = tokio::spawn(async move {
            run_client_worker(client_id, socket_path, calls_per_client, completed)
                .await
                .expect("Client worker failed");
        });
        client_tasks.push(task);
    }

    // Wait for all clients to complete
    for task in client_tasks {
        task.await?;
    }

    let elapsed = start.elapsed();

    // Wait for stats reporter to finish
    stats_handle.await?;

    // Final stats
    println!();
    println!();
    println!("=== Results ===");
    println!("Total time: {:.2}s", elapsed.as_secs_f64());
    println!("Total calls: {}", completed.load(Ordering::Relaxed));
    println!(
        "Average throughput: {:.0} calls/sec",
        total_calls as f64 / elapsed.as_secs_f64()
    );
    println!();
    print!("Call breakdown: ");
    service_stats_final.print_stats();
    println!();
    println!("✓ All calls completed successfully!");

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);

    Ok(())
}
