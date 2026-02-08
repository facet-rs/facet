//! Integration tests for ForwardingDispatcher.
//!
//! Tests the transparent proxy functionality with streaming support:
//! - Unary calls forwarded through proxy
//! - Client-to-server streaming (Rx) forwarded through proxy
//! - Server-to-client streaming (Tx) forwarded through proxy
//! - Bidirectional streaming forwarded through proxy

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use roam_session::{
    ChannelRegistry, Context, ForwardingDispatcher, RoamError, Rx, ServiceDispatcher, Tx, channel,
    dispatch_call, dispatch_unknown_method,
};
use roam_stream::{Connector, HandshakeConfig, NoDispatcher, accept, connect};
use tokio::net::TcpStream;

// ============================================================================
// Test Service (Backend)
// ============================================================================

/// Backend service that implements streaming methods.
#[derive(Clone)]
struct StreamingService {
    call_count: Arc<AtomicU32>,
}

impl StreamingService {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

const METHOD_ECHO: u64 = 1;
const METHOD_SUM: u64 = 2;
const METHOD_GENERATE: u64 = 3;
const METHOD_TRANSFORM: u64 = 4;

impl ServiceDispatcher for StreamingService {
    fn method_ids(&self) -> Vec<u64> {
        vec![METHOD_ECHO, METHOD_SUM, METHOD_GENERATE, METHOD_TRANSFORM]
    }

    fn dispatch(
        &self,
        cx: Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        match cx.method_id().raw() {
            // echo(message: String) -> String
            METHOD_ECHO => dispatch_call::<String, String, (), _, _>(
                &cx,
                payload,
                registry,
                |input: String| async move { Ok(input) },
            ),

            // sum(numbers: Rx<i32>) -> i64
            METHOD_SUM => dispatch_call::<Rx<i32>, i64, (), _, _>(
                &cx,
                payload,
                registry,
                |mut numbers: Rx<i32>| async move {
                    let mut total: i64 = 0;
                    while let Ok(Some(n)) = numbers.recv().await {
                        total += n as i64;
                    }
                    Ok(total)
                },
            ),

            // generate(count: u32, output: Tx<i32>)
            METHOD_GENERATE => dispatch_call::<(u32, Tx<i32>), (), (), _, _>(
                &cx,
                payload,
                registry,
                |(count, output): (u32, Tx<i32>)| async move {
                    for i in 0..count as i32 {
                        let _ = output.send(&i).await;
                    }
                    Ok(())
                },
            ),

            // transform(input: Rx<String>, output: Tx<String>)
            METHOD_TRANSFORM => dispatch_call::<(Rx<String>, Tx<String>), (), (), _, _>(
                &cx,
                payload,
                registry,
                |(mut input, output): (Rx<String>, Tx<String>)| async move {
                    while let Ok(Some(s)) = input.recv().await {
                        let _ = output.send(&s).await;
                    }
                    Ok(())
                },
            ),

            _ => dispatch_unknown_method(&cx, registry),
        }
    }
}

// ============================================================================
// Test Infrastructure
// ============================================================================

/// Connector for TCP streams.
struct TcpConnector {
    addr: SocketAddr,
}

impl Connector for TcpConnector {
    type Transport = TcpStream;

    async fn connect(&self) -> std::io::Result<TcpStream> {
        TcpStream::connect(self.addr).await
    }
}

/// Start a backend server.
async fn start_backend(service: StreamingService) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
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
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    (addr, handle)
}

/// Start a proxy server that forwards to the backend.
async fn start_proxy(backend_addr: SocketAddr) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        while let Ok((client_stream, _)) = listener.accept().await {
            let backend_addr = backend_addr;
            tokio::spawn(async move {
                // Connect to backend
                let backend_connector = TcpConnector { addr: backend_addr };
                let upstream = connect(backend_connector, HandshakeConfig::default(), NoDispatcher);

                // Get the upstream handle
                let upstream_handle = match upstream.handle().await {
                    Ok(h) => h,
                    Err(_) => return,
                };

                // Create forwarding dispatcher
                let forwarder = ForwardingDispatcher::new(upstream_handle);

                // Accept the client connection with forwarding
                if let Ok((handle, _incoming, driver)) =
                    accept(client_stream, HandshakeConfig::default(), forwarder).await
                {
                    let _ = driver.run().await;
                    drop(handle);
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    (proxy_addr, handle)
}

/// Decode a typed result from raw response bytes.
fn decode_result<T, E>(response: Vec<u8>) -> Result<T, RoamError<E>>
where
    T: for<'a> facet::Facet<'a>,
    E: for<'a> facet::Facet<'a>,
{
    facet_postcard::from_slice::<Result<T, RoamError<E>>>(&response).unwrap()
}

// ============================================================================
// Tests
// ============================================================================

/// r[verify channeling.request.channels] - Unary calls work through proxy
#[tokio::test]
async fn test_forwarding_unary() {
    let service = StreamingService::new();
    let call_count = service.call_count.clone();

    // Start backend
    let (backend_addr, _backend_handle) = start_backend(service).await;

    // Start proxy
    let (proxy_addr, _proxy_handle) = start_proxy(backend_addr).await;

    // Connect client to proxy
    let connector = TcpConnector { addr: proxy_addr };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);

    // Make unary call through proxy
    let payload = facet_postcard::to_vec(&"hello through proxy".to_string()).unwrap();
    let response = client.call_raw(METHOD_ECHO, payload).await.unwrap();
    let result: Result<String, RoamError<()>> = decode_result(response);

    assert_eq!(result.unwrap(), "hello through proxy");
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

/// r[verify channeling.request.channels] - Client-to-server streaming works through proxy
#[tokio::test]
async fn test_forwarding_client_to_server_streaming() {
    let service = StreamingService::new();
    let call_count = service.call_count.clone();

    // Start backend
    let (backend_addr, _backend_handle) = start_backend(service).await;

    // Start proxy
    let (proxy_addr, _proxy_handle) = start_proxy(backend_addr).await;

    // Connect client to proxy
    let connector = TcpConnector { addr: proxy_addr };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);

    // Get handle for typed call
    let handle = client.handle().await.unwrap();

    // Create a channel for client-to-server streaming
    let (tx, rx) = channel::<i32>();

    // Spawn task to send numbers
    tokio::spawn(async move {
        for i in 1..=5i32 {
            if tx.send(&i).await.is_err() {
                break;
            }
        }
        // Drop tx to close the channel
    });

    // Make the streaming call: sum(numbers: Rx<i32>) -> i64
    let mut args = rx;
    let response = handle.call(METHOD_SUM, &mut args).await.unwrap();
    let result: Result<i64, RoamError<()>> = decode_result(response.payload);

    // 1 + 2 + 3 + 4 + 5 = 15
    assert_eq!(result.unwrap(), 15);
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

/// r[verify channeling.lifecycle.caller-closes-pushes] - Empty client streams still close through proxy
#[tokio::test]
async fn test_forwarding_client_to_server_empty_stream() {
    let service = StreamingService::new();
    let call_count = service.call_count.clone();

    let (backend_addr, _backend_handle) = start_backend(service).await;
    let (proxy_addr, _proxy_handle) = start_proxy(backend_addr).await;

    let connector = TcpConnector { addr: proxy_addr };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);
    let handle = client.handle().await.unwrap();

    // Create stream then close immediately without sending data.
    let (tx, rx) = channel::<i32>();
    drop(tx);

    let mut args = rx;
    let response = handle.call(METHOD_SUM, &mut args).await.unwrap();
    let result: Result<i64, RoamError<()>> = decode_result(response.payload);

    assert_eq!(result.unwrap(), 0);
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

/// r[verify channeling.request.channels] - Server-to-client streaming works through proxy
#[tokio::test]
async fn test_forwarding_server_to_client_streaming() {
    let service = StreamingService::new();
    let call_count = service.call_count.clone();

    // Start backend
    let (backend_addr, _backend_handle) = start_backend(service).await;

    // Start proxy
    let (proxy_addr, _proxy_handle) = start_proxy(backend_addr).await;

    // Connect client to proxy
    let connector = TcpConnector { addr: proxy_addr };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);

    // Get handle for typed call
    let handle = client.handle().await.unwrap();

    // Create a channel for server-to-client streaming
    let (tx, mut rx) = channel::<i32>();

    // Spawn task to collect results
    let recv_handle = tokio::spawn(async move {
        let mut received = Vec::new();
        while let Ok(Some(n)) = rx.recv().await {
            received.push(n);
        }
        received
    });

    // Make the streaming call: generate(count: u32, output: Tx<i32>)
    let count: u32 = 5;
    let mut args = (count, tx);
    let response = handle.call(METHOD_GENERATE, &mut args).await.unwrap();
    let result: Result<(), RoamError<()>> = decode_result(response.payload);
    assert!(result.is_ok());

    // Wait for receiver to complete
    let received = recv_handle.await.unwrap();

    // Should have received 0, 1, 2, 3, 4
    assert_eq!(received, vec![0, 1, 2, 3, 4]);
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

/// r[verify channeling.request.channels] - Bidirectional streaming works through proxy
#[tokio::test]
async fn test_forwarding_bidirectional_streaming() {
    let service = StreamingService::new();
    let call_count = service.call_count.clone();

    // Start backend
    let (backend_addr, _backend_handle) = start_backend(service).await;

    // Start proxy
    let (proxy_addr, _proxy_handle) = start_proxy(backend_addr).await;

    // Connect client to proxy
    let connector = TcpConnector { addr: proxy_addr };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);

    // Get handle for typed call
    let handle = client.handle().await.unwrap();

    // Create channels for bidirectional streaming
    let (input_tx, input_rx) = channel::<String>();
    let (output_tx, mut output_rx) = channel::<String>();

    // Spawn task to send input
    tokio::spawn(async move {
        for msg in ["hello", "world", "test"] {
            if input_tx.send(&msg.to_string()).await.is_err() {
                break;
            }
        }
        // Drop input_tx to close the input channel
    });

    // Spawn task to collect output
    let recv_handle = tokio::spawn(async move {
        let mut received = Vec::new();
        while let Ok(Some(s)) = output_rx.recv().await {
            received.push(s);
        }
        received
    });

    // Make the bidirectional streaming call: transform(input: Rx<String>, output: Tx<String>)
    let mut args = (input_rx, output_tx);
    let response = handle.call(METHOD_TRANSFORM, &mut args).await.unwrap();
    let result: Result<(), RoamError<()>> = decode_result(response.payload);
    assert!(result.is_ok());

    // Wait for receiver to complete
    let received = recv_handle.await.unwrap();

    // Should have received echoed strings
    assert_eq!(
        received,
        vec!["hello".to_string(), "world".to_string(), "test".to_string()]
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

/// Test multiple streaming calls through the same proxy
#[tokio::test]
async fn test_forwarding_multiple_streaming_calls() {
    let service = StreamingService::new();
    let call_count = service.call_count.clone();

    // Start backend
    let (backend_addr, _backend_handle) = start_backend(service).await;

    // Start proxy
    let (proxy_addr, _proxy_handle) = start_proxy(backend_addr).await;

    // Connect client to proxy
    let connector = TcpConnector { addr: proxy_addr };
    let client = Arc::new(connect(connector, HandshakeConfig::default(), NoDispatcher));

    // Make multiple streaming calls concurrently
    let mut handles = Vec::new();
    for i in 0..3u32 {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let conn_handle = client.handle().await.unwrap();

            // Create channel
            let (tx, rx) = channel::<i32>();

            // Send numbers: i*10 + 1, i*10 + 2, i*10 + 3
            let base = (i * 10) as i32;
            tokio::spawn(async move {
                for j in 1..=3i32 {
                    let _ = tx.send(&(base + j)).await;
                }
            });

            // Call sum
            let mut args = rx;
            let response = conn_handle.call(METHOD_SUM, &mut args).await.unwrap();
            let result: Result<i64, RoamError<()>> = decode_result(response.payload);

            // Expected: (i*10+1) + (i*10+2) + (i*10+3) = i*30 + 6
            let expected = (i as i64) * 30 + 6;
            assert_eq!(result.unwrap(), expected);
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}

/// r[verify channeling.request.channels] - Empty stream close survives multi-hop forwarding
#[tokio::test]
async fn test_forwarding_client_to_server_empty_stream_multi_hop() {
    let service = StreamingService::new();
    let call_count = service.call_count.clone();

    let (backend_addr, _backend_handle) = start_backend(service).await;
    let (mid_proxy_addr, _mid_proxy_handle) = start_proxy(backend_addr).await;
    let (edge_proxy_addr, _edge_proxy_handle) = start_proxy(mid_proxy_addr).await;

    let connector = TcpConnector {
        addr: edge_proxy_addr,
    };
    let client = connect(connector, HandshakeConfig::default(), NoDispatcher);
    let handle = client.handle().await.unwrap();

    let (tx, rx) = channel::<i32>();
    drop(tx);

    let mut args = rx;
    let response = handle.call(METHOD_SUM, &mut args).await.unwrap();
    let result: Result<i64, RoamError<()>> = decode_result(response.payload);

    assert_eq!(result.unwrap(), 0);
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}
