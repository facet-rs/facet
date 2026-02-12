//! Isolate the memory corruption bug.
//!
//! Run with: cargo run --example isolate_bug --release

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use facet::Facet;
use once_cell::sync::Lazy;
use roam_session::{
    ChannelRegistry, Context, RpcPlan, ServiceDispatcher, dispatch_call, dispatch_unknown_method,
};
use roam_stream::{Connector, HandshakeConfig, accept, connect};
use tokio::net::{UnixListener, UnixStream};

// ============================================================================
// RPC Plans
// ============================================================================

static VEC_U8_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(|| RpcPlan::for_type::<Vec<u8>>());
static VEC_U8_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<Vec<u8>>()));

#[derive(Clone)]
struct TestService {
    calls: Arc<AtomicU32>,
}

const METHOD_BIG_VEC: u64 = 1;

impl ServiceDispatcher for TestService {
    fn method_ids(&self) -> Vec<u64> {
        vec![METHOD_BIG_VEC]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.calls.fetch_add(1, Ordering::Relaxed);

        dispatch_call::<Vec<u8>, Vec<u8>, (), _, _>(
            &cx,
            payload,
            registry,
            &VEC_U8_ARGS_PLAN,
            VEC_U8_RESPONSE_PLAN.clone(),
            |data: Vec<u8>| async move {
                let mut result = data;
                result.reverse();
                Ok(result)
            },
        )
    }
}

struct UnixConnector {
    path: PathBuf,
}

impl Connector for UnixConnector {
    type Transport = UnixStream;

    async fn connect(&self) -> std::io::Result<UnixStream> {
        UnixStream::connect(&self.path).await
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;

    rt.block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Testing big Vec<u8> payloads...");

    let socket_path =
        std::env::temp_dir().join(format!("roam-isolate-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);

    let service = TestService {
        calls: Arc::new(AtomicU32::new(0)),
    };
    let calls = service.calls.clone();

    // Start server
    let listener = UnixListener::bind(&socket_path)?;
    tokio::spawn({
        let service = service.clone();
        async move {
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
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let connector = UnixConnector {
        path: socket_path.clone(),
    };
    let client = connect(connector, HandshakeConfig::default(), service);
    let handle = client.handle().await?;

    // Test different sizes
    for size in [100, 1024, 10 * 1024, 50 * 1024, 100 * 1024] {
        println!("Testing size: {} bytes", size);

        let mut tasks = Vec::new();
        for i in 0..100 {
            let handle = handle.clone();
            let task = tokio::spawn(async move {
                let mut data = vec![(i % 256) as u8; size];
                match handle
                    .call(METHOD_BIG_VEC, &mut data, &VEC_U8_ARGS_PLAN)
                    .await
                {
                    Ok(_) => true,
                    Err(e) => {
                        eprintln!("Call failed: {:?}", e);
                        false
                    }
                }
            });
            tasks.push(task);
        }

        let mut succeeded = 0;
        for task in tasks {
            if task.await.unwrap_or(false) {
                succeeded += 1;
            }
        }

        println!("  {} / 100 calls succeeded", succeeded);
    }

    println!();
    println!("Total calls processed: {}", calls.load(Ordering::Relaxed));
    println!("✓ Test completed!");

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}
