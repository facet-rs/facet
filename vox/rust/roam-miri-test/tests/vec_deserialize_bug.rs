//! Minimal reproducer for Vec<u8> deserialization bug found by miri.
//!
//! Run with: cargo +nightly-2026-02-05 miri test

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use once_cell::sync::Lazy;
use roam_session::{
    ChannelRegistry, Context, HandshakeConfig, MessageTransport, NoDispatcher, RpcPlan,
    ServiceDispatcher, accept_framed, dispatch_call, dispatch_unknown_method, initiate_framed,
};
use roam_wire::Message;
use tokio::sync::mpsc;

// ============================================================================
// RPC Plans
// ============================================================================

static VEC_U8_ARGS_PLAN: Lazy<RpcPlan> = Lazy::new(RpcPlan::for_type::<Vec<u8>>);
static VEC_U8_RESPONSE_PLAN: Lazy<Arc<RpcPlan>> =
    Lazy::new(|| Arc::new(RpcPlan::for_type::<Vec<u8>>()));

const METHOD_BIG_DATA: u64 = 1;

#[derive(Clone)]
struct TestService {
    calls_total: Arc<AtomicU32>,
}

impl TestService {
    fn new() -> Self {
        Self {
            calls_total: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl ServiceDispatcher for TestService {
    fn method_ids(&self) -> Vec<u64> {
        vec![METHOD_BIG_DATA]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.calls_total.fetch_add(1, Ordering::Relaxed);

        match cx.method_id().raw() {
            METHOD_BIG_DATA => dispatch_call::<Vec<u8>, Vec<u8>, (), _, _>(
                &cx,
                payload,
                registry,
                &VEC_U8_ARGS_PLAN,
                VEC_U8_RESPONSE_PLAN.clone(),
                move |data: Vec<u8>| async move {
                    let mut result = data.clone();
                    result.reverse();
                    Ok(result)
                },
            ),
            _ => dispatch_unknown_method(&cx, registry),
        }
    }
}

struct InMemoryTransport {
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    last_decoded: Vec<u8>,
}

fn in_memory_transport_pair(buffer: usize) -> (InMemoryTransport, InMemoryTransport) {
    let (a_to_b_tx, a_to_b_rx) = mpsc::channel(buffer);
    let (b_to_a_tx, b_to_a_rx) = mpsc::channel(buffer);

    let a = InMemoryTransport {
        tx: a_to_b_tx,
        rx: b_to_a_rx,
        last_decoded: Vec::new(),
    };
    let b = InMemoryTransport {
        tx: b_to_a_tx,
        rx: a_to_b_rx,
        last_decoded: Vec::new(),
    };

    (a, b)
}

impl MessageTransport for InMemoryTransport {
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        self.tx
            .send(msg.clone())
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "peer disconnected"))
    }

    async fn recv_timeout(&mut self, timeout_duration: Duration) -> io::Result<Option<Message>> {
        match tokio::time::timeout(timeout_duration, self.rx.recv()).await {
            Ok(msg) => Ok(msg),
            Err(_) => Ok(None),
        }
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        Ok(self.rx.recv().await)
    }

    fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

type ClientHandle = roam_session::ConnectionHandle;

async fn create_connection_pair(
    service: TestService,
) -> Result<ClientHandle, Box<dyn std::error::Error + Send + Sync>> {
    let (client_transport, server_transport) = in_memory_transport_pair(8192);

    let client_fut = initiate_framed(client_transport, HandshakeConfig::default(), NoDispatcher);
    let server_fut = accept_framed(server_transport, HandshakeConfig::default(), service);

    let (client_setup, server_setup) = tokio::try_join!(client_fut, server_fut)?;

    let (client_handle, _incoming_client, client_driver) = client_setup;
    let (_server_handle, _incoming_server, server_driver) = server_setup;

    tokio::spawn(async move { client_driver.run().await });
    tokio::spawn(async move { server_driver.run().await });

    Ok(client_handle)
}

#[test]
fn test_concurrent_vec_calls() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let service = TestService::new();
        let client_handle = create_connection_pair(service).await.unwrap();

        let mut tasks = Vec::new();
        for i in 0..50 {
            let handle = client_handle.clone();
            let task = tokio::spawn(async move {
                // Mix of different sizes to trigger the bug
                let size = match i % 5 {
                    0 | 1 => 1024 + (i % 100) * 1024, // 1KB to 100KB
                    2 | 3 => 512,                     // Small
                    _ => 2048,                        // Medium
                };

                let mut data = vec![0u8; size];
                for (idx, byte) in data.iter_mut().enumerate() {
                    *byte = (idx % 256) as u8;
                }

                let _ = handle
                    .call(METHOD_BIG_DATA, "test", &mut data, &VEC_U8_ARGS_PLAN)
                    .await;
            });
            tasks.push(task);
        }

        for task in tasks {
            let _ = task.await;
        }
    });
}
