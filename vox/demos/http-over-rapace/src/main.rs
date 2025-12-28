//! HTTP over Rapace - Demo Binary
//!
//! This example demonstrates HTTP request handling where:
//! - The **host** runs a lightweight HTTP server (hyper)
//! - The **cell** owns the axum router with all HTTP logic
//! - Requests flow: client → hyper → rapace RPC → axum → response
//!
//! This pattern keeps the host process light (no axum/tower dependencies)
//! while the cell handles all HTTP routing and business logic.

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use rapace::{AnyTransport, RpcSession};
use rapace_http::HttpRequest;
use tokio::net::TcpListener;

use rapace_http_over_rapace::{
    AxumHttpService, HttpServiceClient, convert_hyper_to_rapace, convert_rapace_to_hyper,
    create_http_service_dispatcher,
};

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main());
}

async fn async_main() {
    println!("=== HTTP over Rapace Demo ===\n");

    // Create a transport pair (in-memory for demo)
    let (host_transport, cell_transport) = AnyTransport::mem_pair();

    // ========== CELL SIDE ==========
    // Create RpcSession for the cell (uses even channel IDs: 2, 4, 6, ...)
    let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));

    // Create the axum-based HTTP service with demo routes
    let http_service = AxumHttpService::with_demo_routes();

    // Set dispatcher for HttpService
    cell_session.set_dispatcher(create_http_service_dispatcher(
        http_service,
        cell_session.buffer_pool().clone(),
    ));

    // Spawn the cell's demux loop
    let cell_session_clone = cell_session.clone();
    let _cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

    // ========== HOST SIDE ==========
    // Create RpcSession for the host (uses odd channel IDs: 1, 3, 5, ...)
    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));

    // Spawn the host's demux loop
    let host_session_clone = host_session.clone();
    let _host_handle = tokio::spawn(async move { host_session_clone.run().await });

    // Create HTTP client for the plugin
    let http_client = Arc::new(HttpServiceClient::new(host_session.clone()));

    // ========== TEST REQUESTS ==========
    println!("--- Testing via direct RPC calls ---\n");

    // Test 1: Health endpoint
    println!("GET /health");
    let response = http_client
        .handle(HttpRequest::get("/health"))
        .await
        .expect("RPC call failed");
    println!(
        "  Status: {}, Body: {:?}\n",
        response.status,
        String::from_utf8_lossy(&response.body)
    );

    // Test 2: Hello endpoint
    println!("GET /hello/World");
    let response = http_client
        .handle(HttpRequest::get("/hello/World"))
        .await
        .expect("RPC call failed");
    println!(
        "  Status: {}, Body: {:?}\n",
        response.status,
        String::from_utf8_lossy(&response.body)
    );

    // Test 3: JSON endpoint
    println!("GET /json");
    let response = http_client
        .handle(HttpRequest::get("/json"))
        .await
        .expect("RPC call failed");
    println!(
        "  Status: {}, Body: {:?}\n",
        response.status,
        String::from_utf8_lossy(&response.body)
    );

    // Test 4: Echo endpoint
    println!("POST /echo with body 'Hello from client!'");
    let response = http_client
        .handle(HttpRequest::post("/echo", b"Hello from client!".to_vec()))
        .await
        .expect("RPC call failed");
    println!(
        "  Status: {}, Body: {:?}\n",
        response.status,
        String::from_utf8_lossy(&response.body)
    );

    // Test 5: 404 Not Found
    println!("GET /nonexistent");
    let response = http_client
        .handle(HttpRequest::get("/nonexistent"))
        .await
        .expect("RPC call failed");
    println!(
        "  Status: {}, Body: {:?}\n",
        response.status,
        String::from_utf8_lossy(&response.body)
    );

    // ========== HTTP SERVER DEMO ==========
    println!("--- Starting HTTP server demo ---\n");
    println!("Starting HTTP server on 127.0.0.1:3000");
    println!("Try: curl http://127.0.0.1:3000/health");
    println!("     curl http://127.0.0.1:3000/hello/YourName");
    println!("     curl http://127.0.0.1:3000/json");
    println!("     curl -X POST -d 'test data' http://127.0.0.1:3000/echo");
    println!("\nPress Ctrl+C to exit\n");

    // Start the HTTP server
    let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");

    loop {
        let (stream, _) = listener.accept().await.expect("Failed to accept");
        let io = TokioIo::new(stream);
        let http_client = http_client.clone();

        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let http_client = http_client.clone();
                async move {
                    // Convert hyper request to rapace request
                    let rapace_req = match convert_hyper_to_rapace(req).await {
                        Ok(r) => r,
                        Err(e) => {
                            return Ok::<_, std::convert::Infallible>(
                                hyper::Response::builder()
                                    .status(500)
                                    .body(http_body_util::Full::new(Bytes::from(format!(
                                        "Request conversion error: {}",
                                        e
                                    ))))
                                    .unwrap(),
                            );
                        }
                    };

                    // Call the plugin via RPC
                    let rapace_resp = match http_client.handle(rapace_req).await {
                        Ok(r) => r,
                        Err(e) => {
                            return Ok(hyper::Response::builder()
                                .status(502)
                                .body(http_body_util::Full::new(Bytes::from(format!(
                                    "RPC error: {:?}",
                                    e
                                ))))
                                .unwrap());
                        }
                    };

                    // Convert rapace response to hyper response
                    match convert_rapace_to_hyper(rapace_resp) {
                        Ok(r) => Ok(r),
                        Err(e) => Ok(hyper::Response::builder()
                            .status(500)
                            .body(http_body_util::Full::new(Bytes::from(format!(
                                "Response conversion error: {}",
                                e
                            ))))
                            .unwrap()),
                    }
                }
            });

            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("Connection error: {:?}", e);
            }
        });
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rapace::AnyTransport;
    use rapace_http::HttpRequest;

    /// Helper to run HTTP scenario with RpcSession
    async fn run_scenario(host_transport: AnyTransport, plugin_transport: AnyTransport) {
        // Plugin session (even channel IDs)
        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        let http_service = AxumHttpService::with_demo_routes();
        plugin_session.set_dispatcher(create_http_service_dispatcher(
            http_service,
            plugin_session.buffer_pool().clone(),
        ));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        // Host session (odd channel IDs)
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        // Create client
        let client = HttpServiceClient::new(host_session.clone());

        // Test health endpoint
        let response = client.handle(HttpRequest::get("/health")).await.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"ok");

        // Test hello endpoint
        let response = client
            .handle(HttpRequest::get("/hello/Alice"))
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"Hello, Alice!");

        // Test JSON endpoint
        let response = client.handle(HttpRequest::get("/json")).await.unwrap();
        assert_eq!(response.status, 200);
        assert!(
            response
                .headers
                .iter()
                .any(|(k, v)| k == "content-type" && v.contains("application/json"))
        );
        let json: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(json["status"], "success");

        // Test echo endpoint
        let response = client
            .handle(HttpRequest::post("/echo", b"test data".to_vec()))
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"test data");

        // Test 404
        let response = client
            .handle(HttpRequest::get("/nonexistent"))
            .await
            .unwrap();
        assert_eq!(response.status, 404);

        // Cleanup
        host_session.close();
        plugin_session.close();
        host_handle.abort();
        plugin_handle.abort();
    }

    #[tokio_test_lite::test]
    async fn test_mem_transport() {
        let (host_transport, plugin_transport) = AnyTransport::mem_pair();
        run_scenario(host_transport, plugin_transport).await;
    }

    #[tokio_test_lite::test]
    async fn test_stream_transport_tcp() {
        use rapace::StreamTransport;
        use tokio::net::{TcpListener, TcpStream};

        // Start a TCP listener
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn accept task
        let accept_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            AnyTransport::new(StreamTransport::new(stream))
        });

        // Connect from host side
        let stream = TcpStream::connect(addr).await.unwrap();
        let host_transport = AnyTransport::new(StreamTransport::new(stream));

        let plugin_transport = accept_task.await.unwrap();

        run_scenario(host_transport, plugin_transport).await;
    }

    #[cfg(unix)]
    #[tokio_test_lite::test]
    async fn test_stream_transport_unix() {
        use rapace::StreamTransport;
        use tokio::net::{UnixListener, UnixStream};

        // Create a temp socket path
        let socket_path = format!("/tmp/rapace-http-test-{}.sock", std::process::id());
        let _ = std::fs::remove_file(&socket_path);

        // Start a Unix listener
        let listener = UnixListener::bind(&socket_path).unwrap();

        // Spawn accept task
        let socket_path_clone = socket_path.clone();
        let accept_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            AnyTransport::new(StreamTransport::new(stream))
        });

        // Connect from host side
        let stream = UnixStream::connect(&socket_path).await.unwrap();
        let host_transport = AnyTransport::new(StreamTransport::new(stream));

        let plugin_transport = accept_task.await.unwrap();

        run_scenario(host_transport, plugin_transport).await;

        // Cleanup
        let _ = std::fs::remove_file(&socket_path_clone);
    }

    #[tokio_test_lite::test]
    async fn test_shm_transport() {
        use rapace::transport::shm::ShmTransport;

        // Create a hub pair for in-process testing
        let (host_shm, plugin_shm) = ShmTransport::hub_pair().expect("Failed to create hub pair");
        let host_transport = AnyTransport::new(host_shm);
        let plugin_transport = AnyTransport::new(plugin_shm);

        run_scenario(host_transport, plugin_transport).await;
    }

    #[tokio_test_lite::test]
    async fn test_health_endpoint() {
        let (host_transport, plugin_transport) = AnyTransport::mem_pair();

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        let http_service = AxumHttpService::with_demo_routes();
        plugin_session.set_dispatcher(create_http_service_dispatcher(
            http_service,
            plugin_session.buffer_pool().clone(),
        ));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let client = HttpServiceClient::new(host_session.clone());
        let response = client.handle(HttpRequest::get("/health")).await.unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(String::from_utf8_lossy(&response.body), "ok");

        host_session.close();
        plugin_session.close();
        host_handle.abort();
        plugin_handle.abort();
    }

    #[tokio_test_lite::test]
    async fn test_hello_with_param() {
        let (host_transport, plugin_transport) = AnyTransport::mem_pair();

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        let http_service = AxumHttpService::with_demo_routes();
        plugin_session.set_dispatcher(create_http_service_dispatcher(
            http_service,
            plugin_session.buffer_pool().clone(),
        ));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let client = HttpServiceClient::new(host_session.clone());

        // Test with different names
        for name in ["Alice", "Bob", "World", "RapaceUser"] {
            let response = client
                .handle(HttpRequest::get(format!("/hello/{}", name)))
                .await
                .unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(
                String::from_utf8_lossy(&response.body),
                format!("Hello, {}!", name)
            );
        }

        host_session.close();
        plugin_session.close();
        host_handle.abort();
        plugin_handle.abort();
    }

    #[tokio_test_lite::test]
    async fn test_json_response() {
        let (host_transport, plugin_transport) = AnyTransport::mem_pair();

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        let http_service = AxumHttpService::with_demo_routes();
        plugin_session.set_dispatcher(create_http_service_dispatcher(
            http_service,
            plugin_session.buffer_pool().clone(),
        ));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let client = HttpServiceClient::new(host_session.clone());
        let response = client.handle(HttpRequest::get("/json")).await.unwrap();

        assert_eq!(response.status, 200);

        // Check content-type header
        let content_type = response
            .headers
            .iter()
            .find(|(k, _)| k == "content-type")
            .map(|(_, v)| v.as_str());
        assert!(content_type.unwrap().contains("application/json"));

        // Parse JSON
        let json: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(json["message"], "This is a JSON response");
        assert_eq!(json["status"], "success");
        assert_eq!(json["version"], 1);

        host_session.close();
        plugin_session.close();
        host_handle.abort();
        plugin_handle.abort();
    }

    #[tokio_test_lite::test]
    async fn test_post_echo() {
        let (host_transport, plugin_transport) = AnyTransport::mem_pair();

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        let http_service = AxumHttpService::with_demo_routes();
        plugin_session.set_dispatcher(create_http_service_dispatcher(
            http_service,
            plugin_session.buffer_pool().clone(),
        ));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let client = HttpServiceClient::new(host_session.clone());

        // Test with various body sizes
        let test_bodies = [
            b"Hello".to_vec(),
            b"A longer message with more content".to_vec(),
            vec![0u8; 1000],   // Binary data
            vec![42u8; 10000], // Larger payload
        ];

        for body in test_bodies {
            let response = client
                .handle(HttpRequest::post("/echo", body.clone()))
                .await
                .unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(response.body, body);
        }

        host_session.close();
        plugin_session.close();
        host_handle.abort();
        plugin_handle.abort();
    }
}
