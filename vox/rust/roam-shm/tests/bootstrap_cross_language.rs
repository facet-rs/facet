#![cfg(all(unix, target_os = "macos"))]

use std::path::PathBuf;
use std::process::Command;

use roam_shm::bootstrap::{SessionId, SessionPaths, unix};
use roam_shm::peer::PeerId;
use shm_primitives::Doorbell;

fn swift_package_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../swift/roam-runtime")
        .canonicalize()
        .expect("swift package path")
}

fn swift_bootstrap_client_path() -> PathBuf {
    let pkg = swift_package_path();
    let candidates = [
        pkg.join(".build/debug/shm-bootstrap-client"),
        pkg.join(".build/arm64-apple-macosx/debug/shm-bootstrap-client"),
        pkg.join(".build/x86_64-apple-macosx/debug/shm-bootstrap-client"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }

    panic!("shm-bootstrap-client binary not found; ensure nextest setup built swift target");
}

#[tokio::test]
async fn rust_host_bootstrap_to_swift_client() {
    let tmp = tempfile::Builder::new()
        .prefix("rshm-xlang-")
        .tempdir_in("/tmp")
        .unwrap();
    let container_root = tmp.path().join("app-group");
    std::fs::create_dir_all(&container_root).unwrap();

    let sid = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let paths = SessionPaths::new(&container_root, sid.clone()).unwrap();
    let listener = unix::bind_control_socket(&paths).unwrap();

    let (_host_doorbell, guest_handle) = Doorbell::create_pair().unwrap();
    let peer_id = PeerId::new(1).unwrap();
    let hub_path = paths.shm_path();
    let hub_path_for_host = hub_path.clone();

    let host_task = tokio::spawn(async move {
        unix::accept_and_send_ticket(
            &listener,
            &sid,
            peer_id,
            &hub_path_for_host,
            guest_handle.as_raw_fd(),
        )
        .await
    });

    let client_bin = swift_bootstrap_client_path();
    let control_sock = paths.control_sock_path();
    let sid_arg = "123e4567-e89b-12d3-a456-426614174000";

    let output = tokio::task::spawn_blocking(move || {
        Command::new(client_bin)
            .args([control_sock.to_str().unwrap(), sid_arg])
            .output()
            .expect("run swift bootstrap client")
    })
    .await
    .unwrap();

    if !output.status.success() {
        panic!(
            "swift client failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("peer_id=1"), "missing peer_id in: {stdout}");
    assert!(
        stdout.contains(&format!("hub_path={}", hub_path.display())),
        "missing hub_path in: {stdout}"
    );

    host_task.await.unwrap().unwrap();
}
