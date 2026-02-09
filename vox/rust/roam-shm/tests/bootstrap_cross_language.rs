#![cfg(all(unix, target_os = "macos"))]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use roam_shm::bootstrap::{SessionId, SessionPaths, unix};
use roam_shm::layout::{SegmentConfig, SizeClass};
use roam_shm::msg::ShmMsg;
use roam_shm::peer::PeerId;
use roam_shm::transport::{message_to_shm_msg, shm_msg_to_message};
use roam_shm::{AddPeerOptions, ShmHost, msg_type};
use roam_wire::{ConnectionId, Message};
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

fn swift_shm_guest_client_path() -> PathBuf {
    let pkg = swift_package_path();
    let candidates = [
        pkg.join(".build/debug/shm-guest-client"),
        pkg.join(".build/arm64-apple-macosx/debug/shm-guest-client"),
        pkg.join(".build/x86_64-apple-macosx/debug/shm-guest-client"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }

    panic!("shm-guest-client binary not found; ensure nextest setup built swift target");
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

#[tokio::test]
async fn rust_host_shm_to_swift_guest_data_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("xlang-shm-data.shm");

    let mut host = ShmHost::create(&path, SegmentConfig::default()).unwrap();
    let ticket = host.add_peer(AddPeerOptions::default()).unwrap();
    let peer_id = ticket.peer_id;
    let args = ticket.to_args();

    let client_bin = swift_shm_guest_client_path();
    let child = Command::new(client_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn swift shm guest client");

    // Parent closes its copy of guest doorbell fd; child keeps inherited one.
    drop(ticket);

    let mut received = Vec::new();
    for _ in 0..100 {
        let result = host.poll();
        received.extend(result.messages);
        if received.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert_eq!(received.len(), 2, "expected 2 messages from Swift guest");
    assert!(received.iter().all(|(pid, _)| *pid == peer_id));

    let inline = received
        .iter()
        .find(|(_, msg)| msg.id == 1)
        .expect("missing inline message");
    assert_eq!(inline.1.payload_bytes(), b"swift-inline");

    let slot_ref = received
        .iter()
        .find(|(_, msg)| msg.id == 2)
        .expect("missing slot-ref message");
    assert_eq!(slot_ref.1.payload_bytes().len(), 2048);
    for (i, b) in slot_ref.1.payload_bytes().iter().enumerate() {
        assert_eq!(*b, i as u8, "slot payload mismatch at byte {i}");
    }

    host.send(
        peer_id,
        &ShmMsg::new(msg_type::DATA, 101, 0, b"ack-inline".to_vec()),
    )
    .unwrap();
    host.send(
        peer_id,
        &ShmMsg::new(msg_type::DATA, 102, 0, b"ack-slot".to_vec()),
    )
    .unwrap();

    let output = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .expect("join wait_with_output task")
        .expect("wait for swift guest client");
    if !output.status.success() {
        panic!(
            "swift shm guest client failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
async fn rust_host_shm_growth_remap_to_swift_guest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("xlang-shm-remap.shm");

    let config = SegmentConfig {
        max_guests: 1,
        max_payload_size: 4096,
        var_slot_classes: vec![SizeClass::new(4096, 1)],
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    let ticket = host.add_peer(AddPeerOptions::default()).unwrap();
    let peer_id = ticket.peer_id;
    let mut args = ticket.to_args();
    args.push("--scenario=remap-recv".to_string());

    let client_bin = swift_shm_guest_client_path();
    let child = Command::new(client_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn swift remap guest client");
    drop(ticket);

    let payload = vec![0xAB; 3000];
    let mut first_sent = false;
    for _ in 0..100 {
        match host.send(
            peer_id,
            &ShmMsg::new(msg_type::DATA, 201, 0, payload.clone()),
        ) {
            Ok(()) => {
                first_sent = true;
                break;
            }
            Err(roam_shm::host::SendError::PeerNotAttached) => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(other) => panic!("unexpected error sending first message: {other:?}"),
        }
    }
    assert!(first_sent, "swift guest did not attach in time");

    let second_before_growth = host.send(
        peer_id,
        &ShmMsg::new(msg_type::DATA, 202, 0, payload.clone()),
    );
    assert!(
        matches!(
            second_before_growth,
            Err(roam_shm::host::SendError::SlotExhausted)
        ),
        "expected SlotExhausted before growth, got {second_before_growth:?}"
    );

    let extent_idx = host.grow_size_class(0).expect("grow size class 0");
    assert_eq!(extent_idx, 1);

    host.send(peer_id, &ShmMsg::new(msg_type::DATA, 202, 0, payload))
        .expect("send should succeed after growth");

    let output = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .expect("join wait_with_output task")
        .expect("wait for swift remap guest client");
    if !output.status.success() {
        panic!(
            "swift remap guest client failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
async fn rust_host_shm_growth_remap_for_swift_send_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("xlang-shm-remap-send.shm");

    let config = SegmentConfig {
        max_guests: 1,
        max_payload_size: 4096,
        var_slot_classes: vec![SizeClass::new(4096, 1)],
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    let ticket = host.add_peer(AddPeerOptions::default()).unwrap();
    let peer_id = ticket.peer_id;
    let mut args = ticket.to_args();
    args.push("--scenario=remap-send".to_string());

    let client_bin = swift_shm_guest_client_path();
    let child = Command::new(client_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn swift remap-send guest client");
    drop(ticket);

    // Wait for first large payload from guest (occupies the only slot in extent 0).
    let mut got_first = false;
    for _ in 0..120 {
        let result = host.poll();
        if result
            .messages
            .iter()
            .any(|(pid, msg)| *pid == peer_id && msg.id == 301 && msg.payload_bytes().len() == 3000)
        {
            got_first = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(got_first, "did not receive first remap-send payload");

    // Grow var-slot class so guest must remap to see new extent.
    let extent_idx = host.grow_size_class(0).expect("grow size class 0");
    assert_eq!(extent_idx, 1);

    // Trigger guest to attempt second large send.
    host.send(
        peer_id,
        &ShmMsg::new(msg_type::DATA, 401, 0, b"start-second-send".to_vec()),
    )
    .expect("send remap trigger");

    // Guest should now send second large payload successfully after remap.
    let mut got_second = false;
    for _ in 0..200 {
        let result = host.poll();
        if result
            .messages
            .iter()
            .any(|(pid, msg)| *pid == peer_id && msg.id == 302 && msg.payload_bytes().len() == 3000)
        {
            got_second = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !got_second {
        for _ in 0..50 {
            let result = host.poll();
            if result.messages.iter().any(|(pid, msg)| {
                *pid == peer_id && msg.id == 302 && msg.payload_bytes().len() == 3000
            }) {
                got_second = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    if got_second {
        host.send(
            peer_id,
            &ShmMsg::new(msg_type::DATA, 402, 0, b"send-remap-ack".to_vec()),
        )
        .expect("send remap-send ack");
    }

    let output = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .expect("join wait_with_output task")
        .expect("wait for swift remap-send guest client");
    if !output.status.success() {
        panic!(
            "swift remap-send guest client failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(
        got_second,
        "did not receive second remap-send payload\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn rust_host_shm_driver_interop_with_swift_guest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("xlang-shm-driver-interop.shm");

    let mut host = ShmHost::create(&path, SegmentConfig::default()).unwrap();
    let ticket = host.add_peer(AddPeerOptions::default()).unwrap();
    let peer_id = ticket.peer_id;
    let mut args = ticket.to_args();
    args.push("--scenario=driver-interop".to_string());

    let client_bin = swift_shm_guest_client_path();
    let child = Command::new(client_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn swift driver interop guest client");
    drop(ticket);

    let req1 = Message::Request {
        conn_id: ConnectionId::ROOT,
        request_id: 901,
        method_id: 1,
        metadata: vec![],
        channels: vec![],
        payload: b"hello".to_vec(),
    };

    let req2 = Message::Request {
        conn_id: ConnectionId::ROOT,
        request_id: 902,
        method_id: 2,
        metadata: vec![],
        channels: vec![77],
        payload: 77u64.to_le_bytes().to_vec(),
    };

    let mut sent1 = false;
    let mut sent2 = false;
    for _ in 0..100 {
        if !sent1 {
            match host.send(peer_id, &message_to_shm_msg(&req1).unwrap()) {
                Ok(()) => sent1 = true,
                Err(roam_shm::host::SendError::PeerNotAttached) => {}
                Err(other) => panic!("unexpected req1 send error: {other:?}"),
            }
        }
        if sent1 && !sent2 {
            match host.send(peer_id, &message_to_shm_msg(&req2).unwrap()) {
                Ok(()) => sent2 = true,
                Err(roam_shm::host::SendError::PeerNotAttached) => {}
                Err(other) => panic!("unexpected req2 send error: {other:?}"),
            }
        }
        if sent1 && sent2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(sent1 && sent2, "swift guest did not attach in time");

    let mut got_resp1 = false;
    let mut got_resp2 = false;
    let mut got_data = false;
    let mut got_close = false;
    let mut seen = Vec::new();

    for _ in 0..300 {
        let result = host.poll();
        for (pid, shm) in result.messages {
            assert_eq!(pid, peer_id);
            let msg = shm_msg_to_message(shm).expect("decode shm message");
            seen.push(format!("{msg:?}"));
            match msg {
                Message::Response {
                    conn_id,
                    request_id,
                    payload,
                    ..
                } => {
                    assert_eq!(conn_id, ConnectionId::ROOT);
                    if request_id == 901 {
                        assert_eq!(payload, b"swift-driver:hello");
                        got_resp1 = true;
                    } else if request_id == 902 {
                        assert_eq!(payload, b"channel-ok");
                        got_resp2 = true;
                    }
                }
                Message::Data {
                    conn_id,
                    channel_id,
                    payload,
                } => {
                    assert_eq!(conn_id, ConnectionId::ROOT);
                    assert_eq!(channel_id, 77);
                    assert_eq!(payload, b"swift-channel");
                    got_data = true;
                }
                Message::Close {
                    conn_id,
                    channel_id,
                } => {
                    assert_eq!(conn_id, ConnectionId::ROOT);
                    assert_eq!(channel_id, 77);
                    got_close = true;
                }
                other => panic!("unexpected message from driver interop: {other:?}"),
            }
        }
        if got_resp1 && got_resp2 && got_data && got_close {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    if !(got_resp1 && got_resp2 && got_data && got_close) {
        let output = tokio::task::spawn_blocking(move || child.wait_with_output())
            .await
            .expect("join wait_with_output task")
            .expect("wait for swift guest client");
        panic!(
            "missing expected messages; got_resp1={got_resp1} got_resp2={got_resp2} got_data={got_data} got_close={got_close}\nseen={seen:?}\nchild status={:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .expect("join wait_with_output task")
        .expect("wait for swift guest client");
    if !output.status.success() {
        panic!(
            "swift driver interop guest client failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
