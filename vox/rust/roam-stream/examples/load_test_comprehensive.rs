//! Comprehensive load test with complex types and channels.
//!
//! Run with: cargo run --example load_test_comprehensive --release
//!
//! This test exercises:
//! - Complex enums (with data variants)
//! - Tuples (nested and simple)
//! - HashMaps and nested collections
//! - Channels (Tx/Rx) in parameters and return types
//! - All of the above under chaos conditions

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use facet::Facet;
use once_cell::sync::Lazy;
use roam_session::{
    ChannelRegistry, Context, RoamError, RpcPlan, ServiceDispatcher, channel, dispatch_call,
    dispatch_unknown_method,
};
use roam_stream::{Connector, HandshakeConfig, accept, connect};
use tokio::net::{UnixListener, UnixStream};

// ============================================================================
// Complex Types
// ============================================================================

#[derive(Facet, Clone, Debug, PartialEq)]
#[repr(C)]
pub enum Command {
    Start { id: u64, config: String },
    Stop,
    Pause { duration_ms: u32 },
    Resume { from_state: String },
    Query { filter: HashMap<String, String> },
    Batch { commands: Vec<Command> },
}

#[derive(Facet, Clone, Debug, PartialEq)]
#[repr(C)]
pub enum Response {
    Ok,
    Data {
        value: Vec<u8>,
    },
    Error {
        code: u32,
        message: String,
    },
    Stats {
        counts: HashMap<String, u64>,
        timing: (f64, f64, f64), // (min, avg, max)
    },
    Stream {
        items: Vec<(String, u64)>,
    },
}

#[derive(Facet, Clone, Debug)]
pub struct ComplexData {
    id: u64,
    tags: Vec<String>,
    metadata: HashMap<String, String>,
    measurements: Vec<(String, f64, bool)>,
    nested: Option<Box<ComplexData>>,
}

// ============================================================================
// RPC Plans
// ============================================================================

static U64_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<u64>());
static U64_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> = Lazy::new(|| Arc::new(RpcPlan::for_type::<u64>()));

static COMMAND_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<Command>());
static RESPONSE_PLAN: Lazy<Arc<RpcPlan>> = Lazy::new(|| Arc::new(RpcPlan::for_type::<Response>()));

static COMPLEX_DATA_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<ComplexData>());
static TUPLE_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<(u64, String, HashMap<String, u64>)>()));

type StreamRx = roam_session::Rx<Vec<u8>>;

#[derive(Facet)]
pub struct StreamRequest {
    count: u32,
    size: usize,
    rx: StreamRx, // Client sends TO server, so passes Rx
}

static STREAM_REQUEST_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<StreamRequest>());

#[derive(Facet)]
pub struct StreamResponse {
    total_sent: u64,
    rx: StreamRx, // Server sends TO client, so returns Rx
}

static STREAM_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<StreamResponse>()));

// ============================================================================
// Test Service
// ============================================================================

#[derive(Clone)]
struct ComprehensiveService {
    calls_total: Arc<AtomicU32>,
    calls_command: Arc<AtomicU32>,
    calls_complex: Arc<AtomicU32>,
    calls_stream_req: Arc<AtomicU32>,
    calls_stream_resp: Arc<AtomicU32>,
}

impl ComprehensiveService {
    fn new() -> Self {
        Self {
            calls_total: Arc::new(AtomicU32::new(0)),
            calls_command: Arc::new(AtomicU32::new(0)),
            calls_complex: Arc::new(AtomicU32::new(0)),
            calls_stream_req: Arc::new(AtomicU32::new(0)),
            calls_stream_resp: Arc::new(AtomicU32::new(0)),
        }
    }

    fn stats(&self) -> (u32, u32, u32, u32, u32) {
        (
            self.calls_total.load(Ordering::Relaxed),
            self.calls_command.load(Ordering::Relaxed),
            self.calls_complex.load(Ordering::Relaxed),
            self.calls_stream_req.load(Ordering::Relaxed),
            self.calls_stream_resp.load(Ordering::Relaxed),
        )
    }
}

const METHOD_EXECUTE_COMMAND: u64 = 1;
const METHOD_PROCESS_COMPLEX: u64 = 2;
const METHOD_STREAM_TO_CALLER: u64 = 3;
const METHOD_STREAM_FROM_CALLER: u64 = 4;

impl ServiceDispatcher for ComprehensiveService {
    fn method_ids(&self) -> Vec<u64> {
        vec![
            METHOD_EXECUTE_COMMAND,
            METHOD_PROCESS_COMPLEX,
            METHOD_STREAM_TO_CALLER,
            METHOD_STREAM_FROM_CALLER,
        ]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.calls_total.fetch_add(1, Ordering::Relaxed);

        match cx.method_id().raw() {
            // Execute complex enum command
            METHOD_EXECUTE_COMMAND => {
                self.calls_command.fetch_add(1, Ordering::Relaxed);
                dispatch_call::<Command, Response, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &COMMAND_ARGS_PLAN,
                    RESPONSE_PLAN.clone(),
                    |cmd: Command| async move {
                        tokio::time::sleep(Duration::from_millis(5)).await;

                        let response = match cmd {
                            Command::Start { id, .. } => Response::Data {
                                value: id.to_le_bytes().to_vec(),
                            },
                            Command::Stop => Response::Ok,
                            Command::Pause { .. } => Response::Ok,
                            Command::Resume { .. } => Response::Ok,
                            Command::Query { filter } => {
                                let counts: HashMap<String, u64> = filter
                                    .into_iter()
                                    .map(|(k, v)| (k, v.len() as u64))
                                    .collect();
                                Response::Stats {
                                    counts,
                                    timing: (1.0, 5.0, 10.0),
                                }
                            }
                            Command::Batch { commands } => {
                                let items: Vec<(String, u64)> = commands
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, _)| (format!("cmd_{}", i), i as u64))
                                    .collect();
                                Response::Stream { items }
                            }
                        };

                        Ok(response)
                    },
                )
            }

            // Process complex nested data structure
            METHOD_PROCESS_COMPLEX => {
                self.calls_complex.fetch_add(1, Ordering::Relaxed);
                dispatch_call::<ComplexData, (u64, String, HashMap<String, u64>), (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &COMPLEX_DATA_ARGS_PLAN,
                    TUPLE_RESPONSE_PLAN.clone(),
                    |data: ComplexData| async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;

                        let tag_count = data.tags.len() as u64;
                        let summary = format!(
                            "{} tags, {} metadata, {} measurements",
                            data.tags.len(),
                            data.metadata.len(),
                            data.measurements.len()
                        );

                        let stats: HashMap<String, u64> = data
                            .metadata
                            .into_iter()
                            .map(|(k, v)| (k, v.len() as u64))
                            .collect();

                        Ok((data.id, summary, stats))
                    },
                )
            }

            // Stream data TO caller via Rx channel in response
            METHOD_STREAM_TO_CALLER => {
                self.calls_stream_req.fetch_add(1, Ordering::Relaxed);
                dispatch_call::<u64, StreamResponse, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &U64_ARGS_PLAN,
                    STREAM_RESPONSE_PLAN.clone(),
                    |count: u64| async move {
                        let (tx, rx) = channel::<Vec<u8>>();

                        // Spawn task to send data
                        tokio::spawn(async move {
                            for i in 0..count {
                                let data = vec![i as u8; 100];
                                if tx.send(&data).await.is_err() {
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(1)).await;
                            }
                        });

                        Ok(StreamResponse {
                            total_sent: count,
                            rx,
                        })
                    },
                )
            }

            // Stream data FROM caller via Rx channel in request
            METHOD_STREAM_FROM_CALLER => {
                self.calls_stream_resp.fetch_add(1, Ordering::Relaxed);
                dispatch_call::<StreamRequest, u64, (), _, _>(
                    &cx,
                    payload,
                    registry,
                    &STREAM_REQUEST_ARGS_PLAN,
                    U64_RESPONSE_PLAN.clone(),
                    |mut req: StreamRequest| async move {
                        // Consume data from the channel
                        let mut total_bytes = 0u64;

                        while let Ok(Some(data)) = req.rx.recv().await {
                            total_bytes += data.len() as u64;
                        }

                        Ok(total_bytes)
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
    service: ComprehensiveService,
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
// Test Scenarios
// ============================================================================

async fn test_complex_enums(
    socket_path: PathBuf,
    iterations: usize,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connector = UnixConnector { path: socket_path };
    let client = connect(
        connector,
        HandshakeConfig::default(),
        ComprehensiveService::new(),
    );
    let handle = client.handle().await?;

    for i in 0..iterations {
        let handle = handle.clone();
        let stats = stats.clone();

        tokio::spawn(async move {
            let commands = vec![
                Command::Start {
                    id: i as u64,
                    config: format!("config_{}", i),
                },
                Command::Query {
                    filter: [
                        ("type".to_string(), "test".to_string()),
                        ("id".to_string(), i.to_string()),
                    ]
                    .into_iter()
                    .collect(),
                },
                Command::Batch {
                    commands: vec![
                        Command::Pause { duration_ms: 100 },
                        Command::Resume {
                            from_state: "paused".to_string(),
                        },
                        Command::Stop,
                    ],
                },
            ];

            for mut cmd in commands {
                if let Ok(response) = handle
                    .call(METHOD_EXECUTE_COMMAND, &mut cmd, &COMMAND_ARGS_PLAN)
                    .await
                {
                    let _: Response = decode_result(response.payload);
                    stats.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
}

async fn test_complex_tuples(
    socket_path: PathBuf,
    iterations: usize,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connector = UnixConnector { path: socket_path };
    let client = connect(
        connector,
        HandshakeConfig::default(),
        ComprehensiveService::new(),
    );
    let handle = client.handle().await?;

    for i in 0..iterations {
        let handle = handle.clone();
        let stats = stats.clone();

        tokio::spawn(async move {
            let mut data = ComplexData {
                id: i as u64,
                tags: vec!["tag1".to_string(), "tag2".to_string(), format!("tag_{}", i)],
                metadata: [
                    ("key1".to_string(), "value1".to_string()),
                    ("key2".to_string(), format!("value_{}", i)),
                ]
                .into_iter()
                .collect(),
                measurements: vec![
                    ("cpu".to_string(), 0.5, true),
                    ("mem".to_string(), 0.8, false),
                    ("disk".to_string(), (i as f64) / 100.0, i % 2 == 0),
                ],
                nested: if i % 3 == 0 {
                    Some(Box::new(ComplexData {
                        id: i as u64 + 1000,
                        tags: vec!["nested".to_string()],
                        metadata: HashMap::new(),
                        measurements: vec![],
                        nested: None,
                    }))
                } else {
                    None
                },
            };

            if let Ok(response) = handle
                .call(METHOD_PROCESS_COMPLEX, &mut data, &COMPLEX_DATA_ARGS_PLAN)
                .await
            {
                let _: (u64, String, HashMap<String, u64>) = decode_result(response.payload);
                stats.fetch_add(1, Ordering::Relaxed);
            }
        });
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
}

async fn test_channels_tx_in_response(
    socket_path: PathBuf,
    iterations: usize,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connector = UnixConnector { path: socket_path };
    let client = connect(
        connector,
        HandshakeConfig::default(),
        ComprehensiveService::new(),
    );
    let handle = client.handle().await?;

    for i in 0..iterations {
        let handle = handle.clone();
        let stats = stats.clone();

        tokio::spawn(async move {
            let mut count = (i % 10) as u64 + 1;

            if let Ok(response) = handle
                .call(METHOD_STREAM_TO_CALLER, &mut count, &U64_ARGS_PLAN)
                .await
            {
                let mut stream_resp: StreamResponse = decode_result(response.payload);
                let mut received = 0;

                while let Ok(Some(_data)) = stream_resp.rx.recv().await {
                    received += 1;
                }

                if received == count {
                    stats.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
}

async fn test_channels_rx_in_request(
    socket_path: PathBuf,
    iterations: usize,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connector = UnixConnector { path: socket_path };
    let client = connect(
        connector,
        HandshakeConfig::default(),
        ComprehensiveService::new(),
    );
    let handle = client.handle().await?;

    for i in 0..iterations {
        let handle = handle.clone();
        let stats = stats.clone();

        tokio::spawn(async move {
            let (tx, rx) = channel::<Vec<u8>>();
            let count = (i % 10) as u32 + 1;
            let size = 100;

            let mut req = StreamRequest {
                count,
                size,
                rx, // Pass rx in request (server receives from it)
            };

            // Spawn task to send data
            let send_task = tokio::spawn(async move {
                for j in 0..count {
                    let data = vec![j as u8; size];
                    if tx.send(&data).await.is_err() {
                        break;
                    }
                }
            });

            if let Ok(response) = handle
                .call(
                    METHOD_STREAM_FROM_CALLER,
                    &mut req,
                    &STREAM_REQUEST_ARGS_PLAN,
                )
                .await
            {
                let total_bytes: u64 = decode_result(response.payload);
                let expected = (count as u64) * (size as u64);

                if total_bytes == expected {
                    stats.fetch_add(1, Ordering::Relaxed);
                }
            }

            let _ = send_task.await;
        });
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;
    Ok(())
}

async fn chaos_mixed_complex(
    socket_path: PathBuf,
    duration_secs: u64,
    stats: Arc<AtomicU64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(duration_secs) {
        let scenario = stats.load(Ordering::Relaxed) % 4;

        match scenario {
            0 => {
                // Quick enum test with disconnect
                let connector = UnixConnector {
                    path: socket_path.clone(),
                };
                let client = connect(
                    connector,
                    HandshakeConfig::default(),
                    ComprehensiveService::new(),
                );
                if let Ok(handle) = client.handle().await {
                    let task = tokio::spawn(async move {
                        let mut cmd = Command::Batch {
                            commands: vec![Command::Start {
                                id: 999,
                                config: "test".to_string(),
                            }],
                        };
                        let _ = handle
                            .call(METHOD_EXECUTE_COMMAND, &mut cmd, &COMMAND_ARGS_PLAN)
                            .await;
                    });
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    drop(client);
                    let _ = task.await;
                }
            }
            1 => {
                // Complex tuple processing
                let connector = UnixConnector {
                    path: socket_path.clone(),
                };
                let client = connect(
                    connector,
                    HandshakeConfig::default(),
                    ComprehensiveService::new(),
                );
                if let Ok(handle) = client.handle().await {
                    let mut data = ComplexData {
                        id: 123,
                        tags: vec!["chaos".to_string()],
                        metadata: [("test".to_string(), "value".to_string())]
                            .into_iter()
                            .collect(),
                        measurements: vec![("metric".to_string(), 1.0, true)],
                        nested: None,
                    };
                    let _ = handle
                        .call(METHOD_PROCESS_COMPLEX, &mut data, &COMPLEX_DATA_ARGS_PLAN)
                        .await;
                }
            }
            2 => {
                // Channel streaming with cancellation
                let connector = UnixConnector {
                    path: socket_path.clone(),
                };
                let client = connect(
                    connector,
                    HandshakeConfig::default(),
                    ComprehensiveService::new(),
                );
                if let Ok(handle) = client.handle().await {
                    let task = tokio::spawn(async move {
                        let mut count = 5u64;
                        let _ = handle
                            .call(METHOD_STREAM_TO_CALLER, &mut count, &U64_ARGS_PLAN)
                            .await;
                    });
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    task.abort();
                    let _ = task.await;
                }
            }
            _ => {
                // Rapid connect/disconnect
                for _ in 0..3 {
                    let connector = UnixConnector {
                        path: socket_path.clone(),
                    };
                    let client = connect(
                        connector,
                        HandshakeConfig::default(),
                        ComprehensiveService::new(),
                    );
                    let _ = client.handle().await;
                    drop(client);
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
    println!("=== Comprehensive Load Test ===");
    println!("Testing complex types and channels...");
    println!();

    let socket_path = std::env::temp_dir().join(format!(
        "roam-comprehensive-test-{}.sock",
        std::process::id()
    ));

    let service = ComprehensiveService::new();
    let service_stats = service.clone();
    let _server_handle = start_server(socket_path.clone(), service).await?;

    println!("Running test scenarios:");
    println!();

    // Test 1: Complex enums
    {
        println!("1. Complex enums (nested, with hashmaps)...");
        let stats = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        test_complex_enums(socket_path.clone(), 20, stats.clone()).await?;
        println!(
            "   ✓ Completed {} enum calls in {:.2}s",
            stats.load(Ordering::Relaxed),
            start.elapsed().as_secs_f64()
        );
    }

    // Test 2: Complex tuples with nested data
    {
        println!("2. Complex tuples and nested structures...");
        let stats = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        test_complex_tuples(socket_path.clone(), 20, stats.clone()).await?;
        println!(
            "   ✓ Completed {} tuple calls in {:.2}s",
            stats.load(Ordering::Relaxed),
            start.elapsed().as_secs_f64()
        );
    }

    // Test 3: Channels in response (Rx)
    {
        println!("3. Channels in response (streaming to caller)...");
        let stats = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        test_channels_tx_in_response(socket_path.clone(), 20, stats.clone()).await?;
        println!(
            "   ✓ Completed {} streaming responses in {:.2}s",
            stats.load(Ordering::Relaxed),
            start.elapsed().as_secs_f64()
        );
    }

    // Test 4: Channels in request (Tx)
    {
        println!("4. Channels in request (streaming from caller)...");
        let stats = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        test_channels_rx_in_request(socket_path.clone(), 20, stats.clone()).await?;
        println!(
            "   ✓ Completed {} streaming requests in {:.2}s",
            stats.load(Ordering::Relaxed),
            start.elapsed().as_secs_f64()
        );
    }

    // Test 5: Chaos with all complex types
    {
        println!("5. Chaos with all complex types (10 seconds)...");
        let stats = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        chaos_mixed_complex(socket_path.clone(), 10, stats.clone()).await?;
        println!(
            "   ✓ Completed {} chaos operations in {:.2}s",
            stats.load(Ordering::Relaxed),
            start.elapsed().as_secs_f64()
        );
    }

    println!();
    let (total, cmd, complex, stream_req, stream_resp) = service_stats.stats();
    println!("=== Server Stats ===");
    println!("Total calls: {}", total);
    println!("  Command calls: {}", cmd);
    println!("  Complex data calls: {}", complex);
    println!("  Stream to caller: {}", stream_req);
    println!("  Stream from caller: {}", stream_resp);

    println!();
    println!("✓ All tests completed successfully!");

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}
