//! Cross-process tests for HTTP over Rapace.
//!
//! These tests spawn a child process (the helper binary) to run the plugin
//! side of the RPC (the axum HTTP service), while the test runs the host side.
//! This proves that HTTP-over-rapace works across real process boundaries.

use std::process::{Command, Stdio};
use std::time::Duration;

use rapace::helper_binary::find_helper_binary;
use rapace::transport::shm::{ShmSession, ShmSessionConfig};
use rapace::{RpcSession, StreamTransport, Transport};
use rapace_http::{HttpRequest, HttpResponse};
#[cfg(not(unix))]
use tokio::net::TcpListener;

use rapace_http_over_rapace::HttpServiceClient;

#[cfg(unix)]
const STREAM_CONTROL_ENV: &str = "RAPACE_STREAM_CONTROL_FD";

#[cfg(unix)]
fn make_inheritable(stream: &std::os::unix::net::UnixStream) {
    use std::os::fd::AsRawFd;

    let fd = stream.as_raw_fd();
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags == -1 {
            panic!("fcntl(F_GETFD) failed: {}", std::io::Error::last_os_error());
        }
        if libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) == -1 {
            panic!("fcntl(F_SETFD) failed: {}", std::io::Error::last_os_error());
        }
    }
}

#[cfg(unix)]
async fn spawn_helper_stream(
    helper_path: &std::path::Path,
    extra_args: &[&str],
) -> (std::process::Child, tokio::net::TcpStream) {
    use async_send_fd::AsyncSendFd;
    use std::os::unix::{
        io::{AsRawFd, IntoRawFd},
        net::UnixStream as StdUnixStream,
    };
    use tokio::{
        io::AsyncReadExt,
        net::{TcpStream, UnixStream},
        time::timeout,
    };

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener
        .set_nonblocking(true)
        .expect("failed to configure listener");
    let addr = listener.local_addr().unwrap();
    let addr_str = addr.to_string();
    eprintln!("[test] Listening on TCP {}", addr_str);

    let (control_parent, control_child) = StdUnixStream::pair().unwrap();
    make_inheritable(&control_parent);
    make_inheritable(&control_child);
    control_parent
        .set_nonblocking(true)
        .expect("failed to configure control socket");
    control_child
        .set_nonblocking(true)
        .expect("failed to configure control socket");
    let mut control_parent = UnixStream::from_std(control_parent).unwrap();

    let mut cmd = Command::new(helper_path);
    cmd.args(extra_args)
        .arg(format!("--addr={}", addr_str))
        .env(STREAM_CONTROL_ENV, control_child.as_raw_fd().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut helper = cmd.spawn().expect("failed to spawn helper");
    drop(control_child);

    let raw_listener = listener.into_raw_fd();
    if let Err(e) = control_parent.send_fd(raw_listener).await {
        let _ = helper.kill();
        let _ = helper.wait();
        panic!("failed to transfer listener fd: {:?}", e);
    }

    let mut ack = [0u8; 1];
    if let Err(e) = control_parent.read_exact(&mut ack).await {
        let _ = helper.kill();
        let _ = helper.wait();
        panic!("failed to read listener ack: {:?}", e);
    }
    drop(control_parent);

    let stream = match timeout(Duration::from_secs(5), TcpStream::connect(addr)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            let _ = helper.kill();
            let _ = helper.wait();
            panic!("failed to connect to inherited listener: {:?}", e);
        }
        Err(_) => {
            let _ = helper.kill();
            let _ = helper.wait();
            panic!("TCP connect timed out");
        }
    };

    (helper, stream)
}

#[cfg(not(unix))]
async fn spawn_helper_stream(
    helper_path: &std::path::Path,
    extra_args: &[&str],
) -> (std::process::Child, tokio::net::TcpStream) {
    use tokio::time::timeout;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let addr_str = addr.to_string();
    eprintln!("[test] Listening on TCP {}", addr_str);

    let mut cmd = Command::new(helper_path);
    cmd.args(extra_args)
        .arg(format!("--addr={}", addr_str))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut helper = cmd.spawn().expect("failed to spawn helper");

    let stream = match timeout(Duration::from_secs(5), listener.accept()).await {
        Ok(Ok((stream, peer))) => {
            eprintln!("[test] Accepted connection from {:?}", peer);
            stream
        }
        Ok(Err(e)) => {
            let _ = helper.kill();
            let _ = helper.wait();
            panic!("Accept failed: {:?}", e);
        }
        Err(_) => {
            let _ = helper.kill();
            let _ = helper.wait();
            panic!("Accept timed out");
        }
    };

    (helper, stream)
}

/// Run the host side of the scenario with a stream transport.
async fn run_host_scenario_stream(transport: StreamTransport) -> Vec<(HttpRequest, HttpResponse)> {
    run_host_scenario(Transport::Stream(transport)).await
}

/// Run the host side of the scenario with any transport.
async fn run_host_scenario(transport: Transport) -> Vec<(HttpRequest, HttpResponse)> {
    // Host uses odd channel IDs (1, 3, 5, ...)
    // Note: The host doesn't have a dispatcher since it only calls the plugin
    let session = std::sync::Arc::new(RpcSession::with_channel_start(transport, 1));

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
    session.close();
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

#[tokio_test_lite::test]
async fn test_cross_process_tcp() {
    // Find or build the helper binary
    let helper_path = match find_helper_binary("http-plugin-helper") {
        Ok(path) => path,
        Err(e) => {
            eprintln!("[test] {}; attempting to build inline", e);
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

            find_helper_binary("http-plugin-helper")
                .expect("helper binary still not found after building")
        }
    };

    eprintln!("[test] Spawning helper: {:?}", helper_path);
    let (mut helper, stream) = spawn_helper_stream(&helper_path, &["--transport=stream"]).await;

    let transport = StreamTransport::new(stream);

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
#[tokio_test_lite::test]
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

    let transport = StreamTransport::new(stream);

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
#[tokio_test_lite::test]
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
    let transport = Transport::shm(session);

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
