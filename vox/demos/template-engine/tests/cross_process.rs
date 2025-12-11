//! Cross-process tests for the Template Engine.
//!
//! These tests spawn a child process (the helper binary) to run the plugin
//! side of the RPC, while the test runs the host side. This proves that
//! the bidirectional RPC pattern works across real process boundaries.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use rapace::transport::shm::{ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{RpcSession, StreamTransport, Transport};
use rapace_testkit::helper_binary::find_helper_binary;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpListener;

use rapace_template_engine::{create_value_host_dispatcher, TemplateEngineClient, ValueHostImpl};

/// Find an available port for testing.
async fn find_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Run the host side of the scenario with a stream transport.
async fn run_host_scenario_stream<R, W>(transport: StreamTransport<R, W>) -> String
where
    R: tokio::io::AsyncRead + Unpin + Send + Sync + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
{
    let transport = Arc::new(transport);
    run_host_scenario(transport).await
}

/// Run the host side of the scenario with any transport.
async fn run_host_scenario<T: Transport + Send + Sync + 'static>(transport: Arc<T>) -> String {
    // Set up values
    let mut value_host_impl = ValueHostImpl::new();
    value_host_impl.set("user.name", "Alice");
    value_host_impl.set("site.title", "MySite");
    let value_host_impl = Arc::new(value_host_impl);

    // Host uses odd channel IDs (1, 3, 5, ...)
    let session = Arc::new(RpcSession::with_channel_start(transport.clone(), 1));
    session.set_dispatcher(create_value_host_dispatcher(value_host_impl.clone()));

    // Spawn the session runner
    let session_clone = session.clone();
    let session_handle = tokio::spawn(async move { session_clone.run().await });

    // Give the plugin a moment to set up
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send a render request using the generated client
    let client = TemplateEngineClient::new(session.clone());
    let template = "Hi {{user.name}} - {{site.title}}";

    eprintln!("[test] Sending render request: {}", template);

    let rendered = client
        .render(template.to_string())
        .await
        .expect("render failed");
    eprintln!("[test] Got rendered: {}", rendered);

    // Clean up
    let _ = transport.close().await;
    session_handle.abort();

    rendered
}

#[tokio::test]
async fn test_stream_transport_tcp() {
    // Find or build the helper binary
    let helper_path = match find_helper_binary("template-engine-helper") {
        Ok(path) => path,
        Err(e) => {
            eprintln!("[test] {}; attempting to build inline", e);
            let build_status = Command::new("cargo")
                .args([
                    "build",
                    "--bin",
                    "template-engine-helper",
                    "-p",
                    "rapace-template-engine",
                ])
                .status()
                .expect("failed to build helper");
            assert!(build_status.success(), "helper build failed");

            find_helper_binary("template-engine-helper")
                .expect("helper binary still not found after building")
        }
    };

    // Find an available port
    let port = find_available_port().await;
    let addr = format!("127.0.0.1:{}", port);

    eprintln!("[test] Using TCP address: {}", addr);

    // Start listening
    let listener = TcpListener::bind(&addr).await.unwrap();

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Spawn the helper (it will connect to us)
    let mut helper = Command::new(&helper_path)
        .arg(format!("--transport=stream"))
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
    let rendered = run_host_scenario_stream(transport).await;

    // Verify result
    assert_eq!(rendered, "Hi Alice - MySite");

    // Clean up helper
    let _ = helper.kill();
    let _ = helper.wait();

    eprintln!("[test] Test passed!");
}

#[cfg(unix)]
#[tokio::test]
async fn test_stream_transport_unix() {
    use tokio::net::UnixListener;

    // First, build the helper binary
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "template-engine-helper",
            "-p",
            "rapace-template-engine",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Create a temp socket path
    let socket_path = format!("/tmp/rapace-test-{}.sock", std::process::id());

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
        .join("template-engine-helper");

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
    let rendered = run_host_scenario_stream(transport).await;

    // Verify result
    assert_eq!(rendered, "Hi Alice - MySite");

    // Clean up
    let _ = helper.kill();
    let _ = helper.wait();
    let _ = std::fs::remove_file(&socket_path);

    eprintln!("[test] Test passed!");
}

#[cfg(unix)]
#[tokio::test]
async fn test_shm_transport() {
    // First, build the helper binary
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "template-engine-helper",
            "-p",
            "rapace-template-engine",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Create a temp SHM file path
    let shm_path = format!("/tmp/rapace-test-{}.shm", std::process::id());

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
        .join("template-engine-helper");

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
    let rendered = run_host_scenario(transport).await;

    // Verify result
    assert_eq!(rendered, "Hi Alice - MySite");

    // Clean up
    let _ = helper.kill();
    let _ = helper.wait();
    let _ = std::fs::remove_file(&shm_path);

    eprintln!("[test] Test passed!");
}
