//! Cross-process tests for HTTP over Rapace.
//!
//! These tests spawn a child process (the helper binary) to run the plugin
//! side of the RPC (the axum HTTP service), while the test runs the host side.
//! This proves that HTTP-over-rapace works across real process boundaries.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use rapace::transport::shm::{ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{RpcSession, StreamTransport, Transport};
use rapace_http::{HttpRequest, HttpResponse};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpListener;

use rapace_http_over_rapace::HttpServiceClient;

/// Find an available port for testing.
async fn find_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Run the host side of the scenario with a stream transport.
async fn run_host_scenario_stream<R, W>(
    transport: StreamTransport<R, W>,
) -> Vec<(HttpRequest, HttpResponse)>
where
    R: tokio::io::AsyncRead + Unpin + Send + Sync + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
{
    let transport = Arc::new(transport);
    run_host_scenario(transport).await
}

/// Run the host side of the scenario with any transport.
async fn run_host_scenario<T: Transport + Send + Sync + 'static>(
    transport: Arc<T>,
) -> Vec<(HttpRequest, HttpResponse)> {
    // Host uses odd channel IDs (1, 3, 5, ...)
    // Note: The host doesn't have a dispatcher since it only calls the plugin
    let session = Arc::new(RpcSession::with_channel_start(transport.clone(), 1));

    // Spawn the session runner
    let session_clone = session.clone();
    let session_handle = tokio::spawn(async move { session_clone.run().await });

    // Give the plugin a moment to set up
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create HTTP client
    let client = HttpServiceClient::new(session.clone());

    // Test multiple HTTP endpoints
    let mut results = Vec::new();

    // Test 1: Health endpoint
    let req = HttpRequest::get("/health");
    eprintln!("[test] GET /health");
    let resp = client.handle(req.clone()).await.expect("RPC call failed");
    eprintln!(
        "[test] Got response: {} {:?}",
        resp.status,
        String::from_utf8_lossy(&resp.body)
    );
    results.push((req, resp));

    // Test 2: Hello with parameter
    let req = HttpRequest::get("/hello/CrossProcess");
    eprintln!("[test] GET /hello/CrossProcess");
    let resp = client.handle(req.clone()).await.expect("RPC call failed");
    eprintln!(
        "[test] Got response: {} {:?}",
        resp.status,
        String::from_utf8_lossy(&resp.body)
    );
    results.push((req, resp));

    // Test 3: JSON endpoint
    let req = HttpRequest::get("/json");
    eprintln!("[test] GET /json");
    let resp = client.handle(req.clone()).await.expect("RPC call failed");
    eprintln!(
        "[test] Got response: {} {:?}",
        resp.status,
        String::from_utf8_lossy(&resp.body)
    );
    results.push((req, resp));

    // Test 4: POST echo
    let req = HttpRequest::post("/echo", b"Hello from cross-process test!".to_vec());
    eprintln!("[test] POST /echo");
    let resp = client.handle(req.clone()).await.expect("RPC call failed");
    eprintln!(
        "[test] Got response: {} {:?}",
        resp.status,
        String::from_utf8_lossy(&resp.body)
    );
    results.push((req, resp));

    // Test 5: 404 Not Found
    let req = HttpRequest::get("/nonexistent");
    eprintln!("[test] GET /nonexistent");
    let resp = client.handle(req.clone()).await.expect("RPC call failed");
    eprintln!(
        "[test] Got response: {} {:?}",
        resp.status,
        String::from_utf8_lossy(&resp.body)
    );
    results.push((req, resp));

    // Clean up
    let _ = transport.close().await;
    session_handle.abort();

    results
}

/// Verify the responses from the HTTP service.
fn verify_results(results: &[(HttpRequest, HttpResponse)]) {
    // Health check
    assert_eq!(results[0].1.status, 200);
    assert_eq!(results[0].1.body, b"ok");

    // Hello with param
    assert_eq!(results[1].1.status, 200);
    assert_eq!(results[1].1.body, b"Hello, CrossProcess!");

    // JSON
    assert_eq!(results[2].1.status, 200);
    let json: serde_json::Value = serde_json::from_slice(&results[2].1.body).unwrap();
    assert_eq!(json["status"], "success");

    // POST echo
    assert_eq!(results[3].1.status, 200);
    assert_eq!(results[3].1.body, b"Hello from cross-process test!");

    // 404
    assert_eq!(results[4].1.status, 404);
}

#[tokio::test]
async fn test_cross_process_tcp() {
    // First, build the helper binary
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "http-plugin-helper",
            "-p",
            "rapace-http-over-rapace",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Find an available port
    let port = find_available_port().await;
    let addr = format!("127.0.0.1:{}", port);

    eprintln!("[test] Using TCP address: {}", addr);

    // Start listening
    let listener = TcpListener::bind(&addr).await.unwrap();

    // Find the helper binary
    let helper_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("http-plugin-helper");

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Spawn the helper (it will connect to us)
    let mut helper = Command::new(&helper_path)
        .arg("--transport=stream")
        .arg(format!("--addr={}", addr))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn helper");

    // Accept the connection with a timeout
    let stream = match tokio::time::timeout(Duration::from_secs(5), listener.accept()).await {
        Ok(Ok((stream, peer))) => {
            eprintln!("[test] Accepted connection from {:?}", peer);
            stream
        }
        Ok(Err(e)) => {
            helper.kill().ok();
            panic!("Accept failed: {:?}", e);
        }
        Err(_) => {
            helper.kill().ok();
            panic!("Accept timed out");
        }
    };

    let transport: StreamTransport<
        ReadHalf<tokio::net::TcpStream>,
        WriteHalf<tokio::net::TcpStream>,
    > = StreamTransport::new(stream);

    // Run the host scenario
    let results = run_host_scenario_stream(transport).await;

    // Verify results
    verify_results(&results);

    // Clean up helper
    let _ = helper.kill();
    let _ = helper.wait();

    eprintln!("[test] Test passed!");
}

#[cfg(unix)]
#[tokio::test]
async fn test_cross_process_unix() {
    use tokio::net::UnixListener;

    // First, build the helper binary
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "http-plugin-helper",
            "-p",
            "rapace-http-over-rapace",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Create a temp socket path
    let socket_path = format!("/tmp/rapace-http-test-{}.sock", std::process::id());

    // Remove if exists
    let _ = std::fs::remove_file(&socket_path);

    eprintln!("[test] Using Unix socket: {}", socket_path);

    // Start listening
    let listener = UnixListener::bind(&socket_path).unwrap();

    // Find the helper binary
    let helper_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("http-plugin-helper");

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Spawn the helper (it will connect to us)
    let mut helper = Command::new(&helper_path)
        .arg("--transport=stream")
        .arg(format!("--addr={}", socket_path))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn helper");

    // Accept the connection with a timeout
    let stream = match tokio::time::timeout(Duration::from_secs(5), listener.accept()).await {
        Ok(Ok((stream, _peer))) => {
            eprintln!("[test] Accepted connection");
            stream
        }
        Ok(Err(e)) => {
            helper.kill().ok();
            let _ = std::fs::remove_file(&socket_path);
            panic!("Accept failed: {:?}", e);
        }
        Err(_) => {
            helper.kill().ok();
            let _ = std::fs::remove_file(&socket_path);
            panic!("Accept timed out");
        }
    };

    let transport: StreamTransport<
        ReadHalf<tokio::net::UnixStream>,
        WriteHalf<tokio::net::UnixStream>,
    > = StreamTransport::new(stream);

    // Run the host scenario
    let results = run_host_scenario_stream(transport).await;

    // Verify results
    verify_results(&results);

    // Clean up
    let _ = helper.kill();
    let _ = helper.wait();
    let _ = std::fs::remove_file(&socket_path);

    eprintln!("[test] Test passed!");
}

#[cfg(unix)]
#[tokio::test]
async fn test_cross_process_shm() {
    // First, build the helper binary
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "http-plugin-helper",
            "-p",
            "rapace-http-over-rapace",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Create a temp SHM file path
    let shm_path = format!("/tmp/rapace-http-test-{}.shm", std::process::id());

    // Remove if exists
    let _ = std::fs::remove_file(&shm_path);

    eprintln!("[test] Using SHM file: {}", shm_path);

    // Create the SHM session (host is Peer A)
    let session = ShmSession::create_file(&shm_path, ShmSessionConfig::default())
        .expect("failed to create SHM file");
    let transport = Arc::new(ShmTransport::new(session));

    eprintln!("[test] SHM file created, spawning helper...");

    // Find the helper binary
    let helper_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("http-plugin-helper");

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Spawn the helper (it will open the SHM file)
    let mut helper = Command::new(&helper_path)
        .arg("--transport=shm")
        .arg(format!("--addr={}", shm_path))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn helper");

    // Give the helper a moment to map the SHM
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Run the host scenario
    let results = run_host_scenario(transport).await;

    // Verify results
    verify_results(&results);

    // Clean up
    let _ = helper.kill();
    let _ = helper.wait();
    let _ = std::fs::remove_file(&shm_path);

    eprintln!("[test] Test passed!");
}
