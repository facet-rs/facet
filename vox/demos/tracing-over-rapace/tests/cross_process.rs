//! Cross-process tests for Tracing over Rapace.
//!
//! These tests spawn a child process (the helper binary) to run the plugin
//! side (emitting traces), while the test runs the host side (receiving traces).
//! This proves that tracing-over-rapace works across real process boundaries.

use std::process::{Command, Stdio};
use std::time::Duration;

use rapace::helper_binary::find_helper_binary;
use rapace::transport::shm::{ShmSession, ShmSessionConfig};
use rapace::{AnyTransport, RpcSession, StreamTransport};
#[cfg(not(unix))]
use tokio::net::TcpListener;

use rapace_tracing_over_rapace::{HostTracingSink, TraceRecord, create_tracing_sink_dispatcher};

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
async fn run_host_scenario_stream(transport: StreamTransport) -> Vec<TraceRecord> {
    run_host_scenario(AnyTransport::new(transport)).await
}

/// Run the host side of the scenario with any transport.
async fn run_host_scenario(transport: AnyTransport) -> Vec<TraceRecord> {
    // Create the tracing sink
    let tracing_sink = HostTracingSink::new();

    // Host uses odd channel IDs (1, 3, 5, ...)
    let session = std::sync::Arc::new(RpcSession::with_channel_start(transport, 1));
    session.set_dispatcher(create_tracing_sink_dispatcher(
        tracing_sink.clone(),
        session.buffer_pool().clone(),
    ));

    // Spawn the session runner
    let session_clone = session.clone();
    let session_handle = tokio::spawn(async move { session_clone.run().await });

    // Wait for plugin to send traces and close
    // The transport will close when the plugin exits
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Clean up
    session.close();
    session_handle.abort();

    tracing_sink.records()
}

/// Verify the trace records from the plugin helper (stream transports).
/// Stream transports are reliable and ordered.
fn verify_records(records: &[TraceRecord]) {
    eprintln!("[test] Received {} records", records.len());
    for (i, record) in records.iter().enumerate() {
        eprintln!("[test] Record {}: {:?}", i, record);
    }

    // Should have at least some records
    assert!(!records.is_empty(), "Should have some records");

    // Check for expected span names
    let has_outer_span = records
        .iter()
        .any(|r| matches!(r, TraceRecord::NewSpan { meta, .. } if meta.name == "outer_span"));
    assert!(has_outer_span, "Should have outer_span");

    let has_inner_span = records
        .iter()
        .any(|r| matches!(r, TraceRecord::NewSpan { meta, .. } if meta.name == "inner_span"));
    assert!(has_inner_span, "Should have inner_span");

    // Check for expected events
    let has_started_event = records.iter().any(|r| {
        matches!(
            r,
            TraceRecord::Event(e)
                if e.message.contains("cell started") || e.message.contains("plugin started")
        )
    });
    assert!(has_started_event, "Should have 'cell started' event");

    let has_final_event = records
        .iter()
        .any(|r| matches!(r, TraceRecord::Event(e) if e.message.contains("final event")));
    assert!(has_final_event, "Should have 'final event' event");

    // Check for enter/exit pairs
    let enter_count = records
        .iter()
        .filter(|r| matches!(r, TraceRecord::Enter { .. }))
        .count();
    let exit_count = records
        .iter()
        .filter(|r| matches!(r, TraceRecord::Exit { .. }))
        .count();
    assert_eq!(
        enter_count, exit_count,
        "Enter and exit counts should match"
    );
}

/// Verify trace records for SHM transport (relaxed assertions).
/// SHM uses polling and fire-and-forget messages may be lost or reordered.
fn verify_records_shm(records: &[TraceRecord]) {
    eprintln!("[test] Received {} records", records.len());
    for (i, record) in records.iter().enumerate() {
        eprintln!("[test] Record {}: {:?}", i, record);
    }

    // Should have at least some records
    assert!(
        records.len() >= 5,
        "Should have at least 5 records, got {}",
        records.len()
    );

    // SHM is unreliable - messages can be lost or reordered.
    // We just verify we got SOME trace activity, without requiring specific message types.
    // This is intentionally lenient to avoid flakiness in CI environments.

    // Check for any trace activity (spans, events, or span lifecycle events)
    let has_any_span = records
        .iter()
        .any(|r| matches!(r, TraceRecord::NewSpan { .. }));
    let has_any_event = records.iter().any(|r| matches!(r, TraceRecord::Event(_)));
    let has_span_lifecycle = records
        .iter()
        .any(|r| matches!(r, TraceRecord::Enter { .. } | TraceRecord::Exit { .. }));

    assert!(
        has_any_span || has_any_event || has_span_lifecycle,
        "Should have at least some trace activity (spans, events, or span lifecycle), but got none"
    );
}

#[tokio_test_lite::test]
async fn test_cross_process_tcp() {
    // Find or build the helper binary
    let helper_path = match find_helper_binary("tracing-plugin-helper") {
        Ok(path) => path,
        Err(e) => {
            eprintln!("[test] {}; attempting to build inline", e);
            let build_status = Command::new("cargo")
                .args([
                    "build",
                    "--bin",
                    "tracing-plugin-helper",
                    "-p",
                    "rapace-tracing-over-rapace",
                ])
                .status()
                .expect("failed to build helper");
            assert!(build_status.success(), "helper build failed");

            find_helper_binary("tracing-plugin-helper")
                .expect("helper binary still not found after building")
        }
    };

    eprintln!("[test] Spawning helper: {:?}", helper_path);
    let (mut helper, stream) = spawn_helper_stream(&helper_path, &["--transport=stream"]).await;

    let transport = StreamTransport::new(stream);

    // Run host scenario
    let records = run_host_scenario_stream(transport).await;

    // Wait for helper to exit
    let _ = helper.wait();

    // Verify records
    verify_records(&records);

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
            "tracing-plugin-helper",
            "-p",
            "rapace-tracing-over-rapace",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Create temp socket path
    let socket_path = format!("/tmp/rapace-tracing-test-{}.sock", std::process::id());
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
        .join("tracing-plugin-helper");

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Spawn helper
    let mut helper = Command::new(&helper_path)
        .arg("--transport=stream")
        .arg(format!("--addr={}", socket_path))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn helper");

    // Accept connection
    let stream = match tokio::time::timeout(Duration::from_secs(5), listener.accept()).await {
        Ok(Ok((stream, _))) => {
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

    // Run host scenario
    let records = run_host_scenario_stream(transport).await;

    // Cleanup
    let _ = helper.wait();
    let _ = std::fs::remove_file(&socket_path);

    // Verify
    verify_records(&records);

    eprintln!("[test] Test passed!");
}

#[cfg(unix)]
#[tokio_test_lite::test]
async fn test_cross_process_shm() {
    // Build helper
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--bin",
            "tracing-plugin-helper",
            "-p",
            "rapace-tracing-over-rapace",
        ])
        .status()
        .expect("failed to build helper");
    assert!(build_status.success(), "helper build failed");

    // Create temp SHM path
    let shm_path = format!("/tmp/rapace-tracing-test-{}.shm", std::process::id());
    let _ = std::fs::remove_file(&shm_path);

    eprintln!("[test] Using SHM file: {}", shm_path);

    // Create SHM session (host is Peer A)
    let session = ShmSession::create_file(&shm_path, ShmSessionConfig::default())
        .expect("failed to create SHM file");
    let transport = AnyTransport::shm(session);

    eprintln!("[test] SHM file created, spawning helper...");

    // Find helper binary
    let helper_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tracing-plugin-helper");

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Spawn helper
    let mut helper = Command::new(&helper_path)
        .arg("--transport=shm")
        .arg(format!("--addr={}", shm_path))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn helper");

    // Give helper time to map SHM, emit traces, and let spawned async tasks complete
    // SHM needs significant time due to polling nature
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Run host scenario
    let records = run_host_scenario(transport).await;

    // Cleanup
    let _ = helper.wait();
    let _ = std::fs::remove_file(&shm_path);

    // Verify - use relaxed assertions for SHM (fire-and-forget may lose messages)
    verify_records_shm(&records);

    eprintln!("[test] Test passed!");
}
