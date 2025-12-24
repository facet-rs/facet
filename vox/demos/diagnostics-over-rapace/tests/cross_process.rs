//! Cross-process tests for Diagnostics over Rapace.
//!
//! These tests spawn a child process (the helper binary) to run the plugin
//! side (providing diagnostics), while the test runs the host side (requesting analysis).
//! This proves that streaming diagnostics work across real process boundaries.

use std::process::{Child, Command, Stdio};

/// Guard that kills and waits on a child process when dropped.
/// This prevents zombie processes by ensuring we always collect the exit status.
struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    /// Take ownership of the child, preventing automatic cleanup on drop.
    #[allow(dead_code)]
    fn take(&mut self) -> Option<Child> {
        self.0.take()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::StreamExt;
use rapace::helper_binary::find_helper_binary;
use rapace::transport::shm::{ShmSession, ShmSessionConfig};
use rapace::{RpcSession, StreamTransport, Transport};
#[cfg(not(unix))]
use tokio::net::TcpListener;

use rapace_diagnostics_over_rapace::{Diagnostic, DiagnosticsClient};

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

static TRACING_INIT: AtomicBool = AtomicBool::new(false);

fn init_tracing() {
    if TRACING_INIT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("rapace_core=debug".parse().unwrap())
                    .add_directive("rapace_diagnostics_over_rapace=debug".parse().unwrap()),
            )
            .with_writer(std::io::stderr)
            .init();
    }
}

/// Run the host side of the scenario with a stream transport.
async fn run_host_scenario_stream(transport: StreamTransport, source: &str) -> Vec<Diagnostic> {
    run_host_scenario(Transport::Stream(transport), source).await
}

/// Run the host side of the scenario with any transport.
async fn run_host_scenario(transport: Transport, source: &str) -> Vec<Diagnostic> {
    // Create RpcSession and client
    let session = std::sync::Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    tokio::spawn(async move { session_clone.run().await });
    let client = DiagnosticsClient::new(session.clone());

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut stream = client
            .analyze("test.rs".to_string(), source.as_bytes().to_vec())
            .await
            .expect("analyze call failed");

        let mut diagnostics = Vec::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(diag) => diagnostics.push(diag),
                Err(e) => panic!("Stream error: {:?}", e),
            }
        }
        diagnostics
    })
    .await
    .expect("Host scenario timed out");

    // Clean up
    session.close();

    result
}

const TEST_SOURCE: &str = r#"
// Test file for cross-process diagnostics
fn main() {
    // TODO: implement feature
    let x = 1;
    // FIXME: this is broken
    let y = 2;
    // NOTE: remember to test this
    let z = 3;
    // TODO: another todo item
}
"#;

/// Verify the diagnostics from the plugin helper.
fn verify_diagnostics(diagnostics: &[Diagnostic]) {
    eprintln!("[test] Received {} diagnostics", diagnostics.len());
    for (i, diag) in diagnostics.iter().enumerate() {
        eprintln!(
            "[test] Diagnostic {}: {} - {} (line {}, col {})",
            i, diag.code, diag.message, diag.line, diag.column
        );
    }

    // Should have 4 diagnostics: 2 TODOs, 1 FIXME, 1 NOTE
    assert_eq!(diagnostics.len(), 4, "Expected 4 diagnostics");

    // Verify order and content
    assert_eq!(diagnostics[0].code, "TODO001");
    assert_eq!(diagnostics[0].severity, "warning");
    assert_eq!(diagnostics[0].line, 4);

    assert_eq!(diagnostics[1].code, "FIXME001");
    assert_eq!(diagnostics[1].severity, "error");
    assert_eq!(diagnostics[1].line, 6);

    assert_eq!(diagnostics[2].code, "NOTE001");
    assert_eq!(diagnostics[2].severity, "info");
    assert_eq!(diagnostics[2].line, 8);

    assert_eq!(diagnostics[3].code, "TODO001");
    assert_eq!(diagnostics[3].severity, "warning");
    assert_eq!(diagnostics[3].line, 10);
}

#[tokio_test_lite::test]
async fn test_cross_process_tcp() {
    init_tracing();

    // Find or build the helper binary
    let helper_path = match find_helper_binary("diagnostics-plugin-helper") {
        Ok(path) => path,
        Err(e) => {
            eprintln!("[test] {}; attempting to build inline", e);
            let build_status = Command::new("cargo")
                .args([
                    "build",
                    "--bin",
                    "diagnostics-plugin-helper",
                    "-p",
                    "rapace-diagnostics-over-rapace",
                ])
                .status()
                .expect("failed to build helper");
            assert!(build_status.success(), "helper build failed");

            find_helper_binary("diagnostics-plugin-helper")
                .expect("helper binary still not found after building")
        }
    };

    eprintln!("[test] Spawning helper: {:?}", helper_path);
    let (mut helper, stream) = spawn_helper_stream(&helper_path, &["--transport=stream"]).await;

    let transport = StreamTransport::new(stream);

    // Run host scenario
    let diagnostics = run_host_scenario_stream(transport, TEST_SOURCE).await;

    // Kill the helper - it won't exit on its own because transport.close()
    // doesn't actually close the TCP socket (it only sets an atomic flag)
    let _ = helper.kill();

    // Verify diagnostics
    verify_diagnostics(&diagnostics);

    eprintln!("[test] Test passed!");
}

#[cfg(unix)]
#[tokio_test_lite::test]
async fn test_cross_process_unix() {
    use tokio::net::UnixListener;

    // Build helper
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "diagnostics-plugin-helper",
            "-p",
            "rapace-diagnostics-over-rapace",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Create temp socket path
    let socket_path = format!("/tmp/rapace-diag-test-{}.sock", std::process::id());
    let _ = std::fs::remove_file(&socket_path);

    eprintln!("[test] Using Unix socket: {}", socket_path);

    // Start listening
    let listener = UnixListener::bind(&socket_path).unwrap();

    // Find helper binary
    let helper_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("diagnostics-plugin-helper");

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Spawn helper (wrapped in guard to ensure cleanup on panic)
    let _helper = ChildGuard::new(
        Command::new(&helper_path)
            .arg("--transport=stream")
            .arg(format!("--addr={}", socket_path))
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("failed to spawn helper"),
    );

    // Accept connection
    let stream = match tokio::time::timeout(Duration::from_secs(5), listener.accept()).await {
        Ok(Ok((stream, _))) => {
            eprintln!("[test] Accepted connection");
            stream
        }
        Ok(Err(e)) => {
            let _ = std::fs::remove_file(&socket_path);
            panic!("Accept failed: {:?}", e);
        }
        Err(_) => {
            let _ = std::fs::remove_file(&socket_path);
            panic!("Accept timed out");
        }
    };

    let transport = StreamTransport::new(stream);

    // Run host scenario
    let diagnostics = run_host_scenario_stream(transport, TEST_SOURCE).await;

    // Cleanup (helper is killed by ChildGuard on drop)
    let _ = std::fs::remove_file(&socket_path);

    // Verify
    verify_diagnostics(&diagnostics);

    eprintln!("[test] Test passed!");
}

#[cfg(unix)]
#[tokio_test_lite::test]
#[ignore = "SHM cross-process streaming has timing issues - in-process SHM test passes"]
async fn test_cross_process_shm() {
    // Build helper
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "diagnostics-plugin-helper",
            "-p",
            "rapace-diagnostics-over-rapace",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Create temp SHM path
    let shm_path = format!("/tmp/rapace-diag-test-{}.shm", std::process::id());
    let _ = std::fs::remove_file(&shm_path);

    eprintln!("[test] Using SHM file: {}", shm_path);

    // Create SHM session (host is Peer A)
    let session = ShmSession::create_file(&shm_path, ShmSessionConfig::default())
        .expect("failed to create SHM file");
    let transport = Transport::shm(session);

    eprintln!("[test] SHM file created, spawning helper...");

    // Find helper binary
    let helper_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("diagnostics-plugin-helper");

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Spawn helper (wrapped in guard to ensure cleanup on panic)
    let _helper = ChildGuard::new(
        Command::new(&helper_path)
            .arg("--transport=shm")
            .arg(format!("--addr={}", shm_path))
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("failed to spawn helper"),
    );

    // Give helper time to map SHM
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Run host scenario
    let diagnostics = run_host_scenario(transport, TEST_SOURCE).await;

    // Cleanup (helper is killed by ChildGuard on drop)
    let _ = std::fs::remove_file(&shm_path);

    // Verify
    verify_diagnostics(&diagnostics);

    eprintln!("[test] Test passed!");
}

#[tokio_test_lite::test]
async fn test_cross_process_large_file_tcp() {
    // Test with a large source file to exercise streaming properly
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "diagnostics-plugin-helper",
            "-p",
            "rapace-diagnostics-over-rapace",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Generate a large source file
    let mut large_source = String::new();
    for i in 0..50 {
        large_source.push_str(&format!("// Line {} - TODO: task {}\n", i + 1, i));
    }

    eprintln!(
        "[test] Testing large file ({} bytes) over TCP",
        large_source.len()
    );

    let helper_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("diagnostics-plugin-helper");
    let (mut helper, stream) = spawn_helper_stream(&helper_path, &["--transport=stream"]).await;

    let transport = StreamTransport::new(stream);

    let diagnostics = run_host_scenario_stream(transport, &large_source).await;

    // Kill helper since transport.close() doesn't actually close the socket
    let _ = helper.kill();

    // Should have 50 TODOs
    assert_eq!(
        diagnostics.len(),
        50,
        "Expected 50 diagnostics for large file"
    );

    // Verify order
    for (i, diag) in diagnostics.iter().enumerate() {
        assert_eq!(diag.line, (i + 1) as u32, "Diagnostic {} has wrong line", i);
        assert_eq!(diag.code, "TODO001");
    }

    eprintln!("[test] Large file test passed!");
}
