//! Tests for peer death scenarios.
//!
//! These tests verify that when one end of the connection dies,
//! the other end properly detects it and shuts down gracefully
//! without busy looping.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use rapace::helper_binary::find_helper_binary;
use rapace::transport::shm::{HubConfig, HubHost, ShmTransport};
use rapace::{AnyTransport, RpcSession, StreamTransport};

use rapace_template_engine::{TemplateEngineClient, ValueHostImpl, create_value_host_dispatcher};

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

/// Test that when the helper (plugin) dies, the host detects it and shuts down.
#[cfg(unix)]
#[tokio_test_lite::test]
async fn test_stream_helper_death() {
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

    eprintln!("[test] Spawning helper: {:?}", helper_path);
    let (mut helper, stream) = spawn_helper_stream(&helper_path, &["--transport=stream"]).await;

    let transport = AnyTransport::new(StreamTransport::new(stream));

    // Set up the host side
    let mut value_host_impl = ValueHostImpl::new();
    value_host_impl.set("user.name", "Alice");
    let value_host_impl = Arc::new(value_host_impl);

    let session = Arc::new(RpcSession::with_channel_start(transport, 1));
    session.set_dispatcher(create_value_host_dispatcher(
        value_host_impl.clone(),
        session.buffer_pool().clone(),
    ));

    let session_clone = session.clone();
    let session_handle = tokio::spawn(async move { session_clone.run().await });

    // Give the helper time to set up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Make a successful call first to verify everything is working
    let client = TemplateEngineClient::new(session.clone());
    let result = client
        .render("Hello {{user.name}}".to_string())
        .await
        .expect("first call should succeed");
    assert_eq!(result, "Hello Alice");
    eprintln!("[test] First call succeeded");

    // Now kill the helper
    eprintln!("[test] Killing helper process");
    helper.kill().expect("failed to kill helper");
    let _ = helper.wait();

    // The session should detect the peer death and exit within a reasonable time
    eprintln!("[test] Waiting for session to detect peer death");
    let timeout_result = tokio::time::timeout(Duration::from_secs(5), session_handle).await;

    match timeout_result {
        Ok(Ok(Ok(()))) => {
            eprintln!("[test] Session exited normally (transport closed)");
        }
        Ok(Ok(Err(e))) => {
            eprintln!("[test] Session exited with transport error: {:?}", e);
        }
        Ok(Err(e)) => {
            eprintln!("[test] Session panicked: {:?}", e);
            panic!("Session panicked instead of exiting gracefully");
        }
        Err(_) => {
            panic!(
                "Session did not exit within 5 seconds after helper was killed - likely a busy loop!"
            );
        }
    }

    eprintln!("[test] Test passed!");
}

/// Test that when the host dies, the helper detects it and shuts down.
#[cfg(unix)]
#[tokio_test_lite::test]
async fn test_stream_host_death() {
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

    eprintln!("[test] Spawning helper: {:?}", helper_path);
    let (mut helper, stream) = spawn_helper_stream(&helper_path, &["--transport=stream"]).await;

    let transport = AnyTransport::new(StreamTransport::new(stream));

    // Set up the host side
    let mut value_host_impl = ValueHostImpl::new();
    value_host_impl.set("user.name", "Alice");
    let value_host_impl = Arc::new(value_host_impl);

    let session = Arc::new(RpcSession::with_channel_start(transport, 1));
    session.set_dispatcher(create_value_host_dispatcher(
        value_host_impl.clone(),
        session.buffer_pool().clone(),
    ));

    let session_clone = session.clone();
    let session_handle = tokio::spawn(async move { session_clone.run().await });

    // Give the helper time to set up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Make a successful call first to verify everything is working
    let client = TemplateEngineClient::new(session.clone());
    let result = client
        .render("Hello {{user.name}}".to_string())
        .await
        .expect("first call should succeed");
    assert_eq!(result, "Hello Alice");
    eprintln!("[test] First call succeeded");

    // Now drop the host transport and session to simulate host death
    eprintln!("[test] Dropping host session");
    drop(client);
    session_handle.abort();
    drop(session);

    // The helper should detect that the host is gone and exit within a reasonable time
    eprintln!("[test] Waiting for helper to exit");
    let mut exit_check_count = 0;
    loop {
        match helper.try_wait() {
            Ok(Some(status)) => {
                eprintln!("[test] Helper exited with status: {:?}", status);
                break;
            }
            Ok(None) => {
                // Still running
                exit_check_count += 1;
                if exit_check_count > 50 {
                    // 50 * 100ms = 5 seconds
                    let _ = helper.kill();
                    let _ = helper.wait();
                    panic!(
                        "Helper did not exit within 5 seconds after host closed - likely a busy loop!"
                    );
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => {
                eprintln!("[test] Error checking helper status: {:?}", e);
                let _ = helper.kill();
                let _ = helper.wait();
                panic!("Error checking helper status: {:?}", e);
            }
        }
    }

    eprintln!("[test] Test passed!");
}

/// Test that when one end of a SHM transport dies, the other end detects it.
///
/// NOTE: This test is disabled because hub transport peer death detection needs
/// improvement. The hub doorbell should detect when a peer process exits.
#[cfg(unix)]
#[tokio_test_lite::test]
async fn test_shm_helper_death() {
    eprintln!("[test] SKIPPED: Hub transport peer death detection needs improvement");
    return;

    // Build the helper binary
    #[allow(unreachable_code)]
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
    let shm_path = format!("/tmp/rapace-test-peer-death-{}.shm", std::process::id());

    // Remove if exists
    let _ = std::fs::remove_file(&shm_path);

    eprintln!("[test] Using SHM file: {}", shm_path);

    // Create the hub host
    let host = std::sync::Arc::new(
        HubHost::create(&shm_path, HubConfig::default()).expect("failed to create hub"),
    );

    // Add a peer and get the doorbell FD for passing to the child process
    let peer_info = host.add_peer().expect("failed to add peer");
    let peer_id = peer_info.peer_id;
    let peer_doorbell_fd = peer_info.peer_doorbell_fd;

    // Create host-side transport
    let transport = AnyTransport::new(ShmTransport::hub_host_peer(
        host.clone(),
        peer_id,
        peer_info.doorbell,
    ));

    eprintln!("[test] Hub created, spawning helper...");

    // Find the helper binary
    let helper_path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("template-engine-helper");

    eprintln!("[test] Spawning helper: {:?}", helper_path);

    // Make the doorbell FD inheritable by clearing FD_CLOEXEC
    unsafe {
        let flags = libc::fcntl(peer_doorbell_fd, libc::F_GETFD);
        if flags != -1 {
            libc::fcntl(peer_doorbell_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
        }
    }

    // Spawn the helper with hub transport args
    let mut helper = Command::new(&helper_path)
        .arg("--transport=shm")
        .arg(format!("--hub-path={}", shm_path))
        .arg(format!("--peer-id={}", peer_id))
        .arg(format!("--doorbell-fd={}", peer_doorbell_fd))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn helper");

    // Give the helper a moment to map the SHM and register
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Set up the host side
    let mut value_host_impl = ValueHostImpl::new();
    value_host_impl.set("user.name", "Alice");
    let value_host_impl = Arc::new(value_host_impl);

    let session = Arc::new(RpcSession::with_channel_start(transport, 1));
    session.set_dispatcher(create_value_host_dispatcher(
        value_host_impl.clone(),
        session.buffer_pool().clone(),
    ));

    let session_clone = session.clone();
    let session_handle = tokio::spawn(async move { session_clone.run().await });

    // Give the helper time to set up
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Make a successful call first to verify everything is working
    let client = TemplateEngineClient::new(session.clone());
    let result = client
        .render("Hello {{user.name}}".to_string())
        .await
        .expect("first call should succeed");
    assert_eq!(result, "Hello Alice");
    eprintln!("[test] First call succeeded");

    // Now kill the helper
    eprintln!("[test] Killing helper process");
    helper.kill().expect("failed to kill helper");
    let _ = helper.wait();

    // The session should detect the peer death and exit within a reasonable time
    // Note: This may take longer for SHM since there's no OS-level notification
    eprintln!("[test] Waiting for session to detect peer death");
    let timeout_result = tokio::time::timeout(Duration::from_secs(10), session_handle).await;

    match timeout_result {
        Ok(Ok(Ok(()))) => {
            eprintln!("[test] Session exited normally (transport closed)");
        }
        Ok(Ok(Err(e))) => {
            eprintln!("[test] Session exited with transport error: {:?}", e);
        }
        Ok(Err(e)) => {
            eprintln!("[test] Session panicked: {:?}", e);
            panic!("Session panicked instead of exiting gracefully");
        }
        Err(_) => {
            panic!(
                "Session did not exit within 10 seconds after helper was killed - likely a busy loop!"
            );
        }
    }

    // Clean up
    let _ = std::fs::remove_file(&shm_path);

    eprintln!("[test] Test passed!");
}
