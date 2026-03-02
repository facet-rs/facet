#![cfg(all(unix, target_os = "macos"))]

use std::path::{Path, PathBuf};
use std::process::Output;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use facet_postcard::{from_slice_borrowed, to_vec};
use roam_shm::HostHub;
use roam_shm::framing::{MmapRef, OwnedFrame, read_frame, write_inline, write_mmap_ref};
use roam_shm::segment::{Segment, SegmentConfig};
use roam_shm::varslot::SizeClassConfig;
use roam_types::{
    ChannelBody, ChannelClose, ChannelGrantCredit, ChannelId, ChannelMessage, ConnectionId, Link,
    LinkRx, LinkTx, LinkTxPermit, Message, MessagePayload, Metadata, MetadataEntry, MetadataFlags,
    MetadataValue, Payload, RequestBody, RequestId, RequestMessage, RequestResponse, WriteSlot,
};
use shm_primitives::{FileCleanup, MmapRegion};
use shm_primitives_async::{MmapAttachMessage, clear_cloexec, create_mmap_control_pair};
use spec_tests::harness::run_async;
use tokio::process::Command as TokioCommand;

fn boundary_sizes() -> &'static [usize] {
    &[55, 56, 57, 4091, 4092, 4093, 16 * 1024, 64 * 1024]
}

fn make_boundary_payload(index: usize, size: usize) -> Vec<u8> {
    (0..size)
        .map(|pos| {
            (index as u8)
                .wrapping_mul(31)
                .wrapping_add((pos as u8).wrapping_mul(17))
        })
        .collect()
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |sum, &byte| sum.wrapping_add(byte as u32))
}

fn encode_ack(len: usize, sum: u32) -> [u8; 8] {
    let len = len as u32;
    [
        len as u8,
        (len >> 8) as u8,
        (len >> 16) as u8,
        (len >> 24) as u8,
        sum as u8,
        (sum >> 8) as u8,
        (sum >> 16) as u8,
        (sum >> 24) as u8,
    ]
}

fn decode_ack(payload: &[u8]) -> Option<(u32, u32)> {
    if payload.len() != 8 {
        return None;
    }
    let len = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let sum = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
    Some((len, sum))
}

async fn wait_child_with_timeout(
    child: tokio::process::Child,
    timeout: Duration,
) -> Result<Output, String> {
    tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| "swift guest child timed out".to_string())?
        .map_err(|e| format!("wait_with_output failed: {e}"))
}

fn swift_runtime_package_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../swift/roam-runtime")
        .canonicalize()
        .expect("swift runtime package path")
}

fn swift_shm_guest_client_path() -> PathBuf {
    let pkg = swift_runtime_package_path();
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

    panic!("shm-guest-client binary not found; build swift/roam-runtime target first")
}

fn read_guest_payloads(
    segment: &Segment,
    peer_id: shm_primitives::PeerId,
    expected_count: usize,
    deadline: Instant,
) -> Vec<Vec<u8>> {
    let g2h = segment.g2h_bipbuf(peer_id);
    let (_tx, mut rx) = g2h.split();
    let mut payloads = Vec::new();

    while Instant::now() < deadline && payloads.len() < expected_count {
        if let Some(frame) = read_frame(&mut rx) {
            match frame {
                OwnedFrame::Inline(bytes) => payloads.push(bytes),
                OwnedFrame::SlotRef(slot_ref) => {
                    let raw = unsafe { segment.var_pool().slot_data(&slot_ref) };
                    let len = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
                    payloads.push(raw[4..4 + len].to_vec());
                }
                OwnedFrame::MmapRef(_) => panic!("unexpected mmap-ref frame"),
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }

    payloads
}

fn send_host_ack(segment: &Segment, peer_id: shm_primitives::PeerId, payload: &[u8]) {
    let h2g = segment.h2g_bipbuf(peer_id);
    let (mut tx, _rx) = h2g.split();
    write_inline(&mut tx, payload).expect("write ack frame");
}

fn send_host_message(segment: &Segment, peer_id: shm_primitives::PeerId, message: &Message<'_>) {
    let h2g = segment.h2g_bipbuf(peer_id);
    let (mut tx, _rx) = h2g.split();
    let payload = to_vec(message).expect("encode host message");
    write_inline(&mut tx, &payload).expect("write host message");
}

fn sample_metadata<'a>() -> Metadata<'a> {
    vec![MetadataEntry {
        key: "trace-id",
        value: MetadataValue::String("rust-trace"),
        flags: MetadataFlags::NONE,
    }]
}

fn make_socketpair() -> (i32, i32) {
    let mut fds = [0i32; 2];
    let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
    assert_eq!(
        rc,
        0,
        "socketpair failed: {}",
        std::io::Error::last_os_error()
    );
    (fds[0], fds[1])
}

fn ring_doorbell(fd: i32) {
    let byte = [1u8];
    let rc = unsafe { libc::send(fd, byte.as_ptr().cast(), 1, libc::MSG_DONTWAIT) };
    assert!(
        rc >= 0,
        "doorbell send failed: {}",
        std::io::Error::last_os_error()
    );
}

pub fn run_data_path_case() {
    let dir = tempfile::tempdir().unwrap();
    let shm_path = dir.path().join("xlang-shm-data-path.shm");
    let class = [SizeClassConfig {
        slot_size: 4096,
        slot_count: 2,
    }];
    let config = SegmentConfig {
        max_guests: 1,
        bipbuf_capacity: 64 * 1024,
        max_payload_size: 4096,
        inline_threshold: 64,
        heartbeat_interval: 0,
        size_classes: &class,
    };
    let segment = Segment::create(Path::new(&shm_path), config, FileCleanup::Manual).unwrap();

    let peer_id = segment.reserve_peer().expect("reserve peer slot");
    let (host_fd, guest_fd) = make_socketpair();
    clear_cloexec(guest_fd).expect("clear close-on-exec");

    let child = Command::new(swift_shm_guest_client_path())
        .arg(format!("--hub-path={}", shm_path.display()))
        .arg(format!("--peer-id={}", peer_id.get()))
        .arg(format!("--doorbell-fd={guest_fd}"))
        .arg("--scenario=data-path")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn swift shm guest client");

    let payloads = read_guest_payloads(
        &segment,
        peer_id,
        2,
        Instant::now() + Duration::from_secs(5),
    );
    if payloads.len() < 2 {
        let output = child
            .wait_with_output()
            .expect("wait for swift guest process");
        panic!(
            "expected two payload frames from Swift guest, got {}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            payloads.len(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        payloads.len(),
        2,
        "expected two payload frames from Swift guest"
    );
    assert_eq!(payloads[0], b"swift-inline");
    assert_eq!(payloads[1].len(), 2048);
    for (idx, byte) in payloads[1].iter().enumerate() {
        assert_eq!(*byte, idx as u8, "slot payload mismatch at byte {idx}");
    }

    send_host_ack(&segment, peer_id, b"ack-inline");
    send_host_ack(&segment, peer_id, b"ack-slot");

    let output = child
        .wait_with_output()
        .expect("wait for swift guest process");
    unsafe {
        libc::close(host_fd);
        libc::close(guest_fd);
    }
    if !output.status.success() {
        panic!(
            "swift shm guest failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

pub fn run_message_v7_case() {
    let dir = tempfile::tempdir().unwrap();
    let shm_path = dir.path().join("xlang-shm-message-v7.shm");
    let class = [SizeClassConfig {
        slot_size: 4096,
        slot_count: 2,
    }];
    let config = SegmentConfig {
        max_guests: 1,
        bipbuf_capacity: 64 * 1024,
        max_payload_size: 4096,
        inline_threshold: 64,
        heartbeat_interval: 0,
        size_classes: &class,
    };
    let segment = Segment::create(Path::new(&shm_path), config, FileCleanup::Manual).unwrap();

    let peer_id = segment.reserve_peer().expect("reserve peer slot");
    let (host_fd, guest_fd) = make_socketpair();
    clear_cloexec(guest_fd).expect("clear close-on-exec");

    let child = Command::new(swift_shm_guest_client_path())
        .arg(format!("--hub-path={}", shm_path.display()))
        .arg(format!("--peer-id={}", peer_id.get()))
        .arg(format!("--doorbell-fd={guest_fd}"))
        .arg("--scenario=message-v7")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn swift shm guest client");

    let payloads = read_guest_payloads(
        &segment,
        peer_id,
        3,
        Instant::now() + Duration::from_secs(5),
    );
    if payloads.len() < 3 {
        let mut child = child;
        let _ = child.kill();
        let output = child
            .wait_with_output()
            .expect("wait for swift guest process");
        panic!(
            "expected three MessageV7 frames from Swift guest, got {}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            payloads.len(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let req = from_slice_borrowed::<Message<'_>>(&payloads[0]).expect("decode request message");
    assert_eq!(req.connection_id, ConnectionId(2));
    match req.payload {
        MessagePayload::RequestMessage(RequestMessage {
            id: RequestId(11),
            body: RequestBody::Call(call),
        }) => {
            assert_eq!(call.channels, vec![ChannelId(3), ChannelId(5)]);
            let payload = match call.args {
                Payload::Incoming(bytes) => bytes,
                _ => panic!("expected incoming bytes payload"),
            };
            assert_eq!(payload, b"swift-request");
        }
        _ => panic!("unexpected first message payload"),
    }

    let close = from_slice_borrowed::<Message<'_>>(&payloads[1]).expect("decode channel close");
    assert_eq!(close.connection_id, ConnectionId(2));
    match close.payload {
        MessagePayload::ChannelMessage(ChannelMessage {
            id: ChannelId(3),
            body: ChannelBody::Close(ChannelClose { metadata }),
        }) => {
            assert_eq!(metadata.len(), 1);
            assert_eq!(metadata[0].key, "reason");
        }
        _ => panic!("unexpected second message payload"),
    }

    let proto = from_slice_borrowed::<Message<'_>>(&payloads[2]).expect("decode protocol error");
    match proto.payload {
        MessagePayload::ProtocolError(err) => {
            assert_eq!(err.description, "swift protocol violation")
        }
        _ => panic!("unexpected third message payload"),
    }

    let ret: u32 = 42;
    let response = Message {
        connection_id: ConnectionId(2),
        payload: MessagePayload::RequestMessage(RequestMessage {
            id: RequestId(11),
            body: RequestBody::Response(RequestResponse {
                ret: Payload::outgoing(&ret),
                channels: vec![ChannelId(7)],
                metadata: sample_metadata(),
            }),
        }),
    };
    send_host_message(&segment, peer_id, &response);

    let credit = Message {
        connection_id: ConnectionId(2),
        payload: MessagePayload::ChannelMessage(ChannelMessage {
            id: ChannelId(3),
            body: ChannelBody::GrantCredit(ChannelGrantCredit { additional: 4096 }),
        }),
    };
    send_host_message(&segment, peer_id, &credit);

    let output = child
        .wait_with_output()
        .expect("wait for swift guest process");
    unsafe {
        libc::close(host_fd);
        libc::close(guest_fd);
    }
    if !output.status.success() {
        panic!(
            "swift shm guest failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

pub fn run_mmap_ref_receive_case() {
    let dir = tempfile::tempdir().unwrap();
    let shm_path = dir.path().join("xlang-shm-mmap-recv.shm");
    let class = [SizeClassConfig {
        slot_size: 64,
        slot_count: 2,
    }];
    let config = SegmentConfig {
        max_guests: 1,
        bipbuf_capacity: 64 * 1024,
        max_payload_size: 4096,
        inline_threshold: 64,
        heartbeat_interval: 0,
        size_classes: &class,
    };
    let segment = Segment::create(Path::new(&shm_path), config, FileCleanup::Manual).unwrap();

    let peer_id = segment.reserve_peer().expect("reserve peer slot");
    let (host_fd, guest_fd) = make_socketpair();
    clear_cloexec(guest_fd).expect("clear close-on-exec");

    let (mmap_tx, mmap_handle) = create_mmap_control_pair().expect("create mmap control pair");
    clear_cloexec(mmap_handle.as_raw_fd()).expect("clear close-on-exec for mmap control fd");

    let child = Command::new(swift_shm_guest_client_path())
        .arg(format!("--hub-path={}", shm_path.display()))
        .arg(format!("--peer-id={}", peer_id.get()))
        .arg(format!("--doorbell-fd={guest_fd}"))
        .arg(format!("--mmap-control-fd={}", mmap_handle.as_raw_fd()))
        .arg("--scenario=mmap-recv")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn swift shm guest client");

    let mmap_path = dir.path().join("xlang-mmap-payload.shm");
    let mapping =
        MmapRegion::create(&mmap_path, 4096, FileCleanup::Manual).expect("create mmap payload");
    let payload: Vec<u8> = (0..512).map(|i| i as u8).collect();
    unsafe {
        let data = mapping.region().as_ptr().add(128);
        std::ptr::copy_nonoverlapping(payload.as_ptr(), data, payload.len());
    }
    let attach_msg = MmapAttachMessage {
        map_id: 7,
        map_generation: 1,
        mapping_length: mapping.len() as u64,
    };
    mmap_tx
        .send(mapping.as_raw_fd(), &attach_msg)
        .expect("send mmap attach message");

    {
        let h2g = segment.h2g_bipbuf(peer_id);
        let (mut tx, _rx) = h2g.split();
        let mmap_ref = MmapRef {
            map_id: 7,
            map_generation: 1,
            map_offset: 128,
            payload_len: payload.len() as u32,
        };
        write_mmap_ref(&mut tx, &mmap_ref).expect("write mmap-ref frame");
    }
    ring_doorbell(host_fd);

    let payloads = read_guest_payloads(
        &segment,
        peer_id,
        1,
        Instant::now() + Duration::from_secs(5),
    );
    if payloads.is_empty() {
        let mut child = child;
        let _ = child.kill();
        let output = child
            .wait_with_output()
            .expect("wait for swift guest process");
        panic!(
            "expected mmap ack from Swift guest, got {}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            payloads.len(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(payloads[0], b"mmap-recv-ok");

    let output = child
        .wait_with_output()
        .expect("wait for swift guest process");
    unsafe {
        libc::close(host_fd);
        libc::close(guest_fd);
    }
    if !output.status.success() {
        panic!(
            "swift shm guest failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

pub fn run_boundary_cutover_rust_to_swift_case() {
    run_async(async {
        let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
        let shm_path = dir.path().join("xlang-shm-boundary-rust-to-swift.shm");
        let class = [SizeClassConfig {
            slot_size: 4096,
            slot_count: 32,
        }];
        let config = SegmentConfig {
            max_guests: 1,
            bipbuf_capacity: 64 * 1024,
            max_payload_size: 1024 * 1024,
            inline_threshold: 64,
            heartbeat_interval: 0,
            size_classes: &class,
        };
        let segment = std::sync::Arc::new(
            Segment::create(Path::new(&shm_path), config, FileCleanup::Manual)
                .map_err(|e| format!("segment create: {e}"))?,
        );
        let hub = HostHub::new(segment);
        let prepared = hub
            .prepare_peer()
            .map_err(|e| format!("prepare peer: {e}"))?;
        let (host_peer, ticket) = prepared.into_parts();

        let doorbell_fd = ticket.doorbell.into_raw_fd();
        let mmap_rx_fd = ticket.mmap_rx.into_raw_fd();
        // Unused by this scenario; close parent copy.
        unsafe { libc::close(ticket.mmap_tx_fd) };

        let sizes_arg = boundary_sizes()
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let child = TokioCommand::new(swift_shm_guest_client_path())
            .arg(format!("--hub-path={}", shm_path.display()))
            .arg(format!("--peer-id={}", ticket.peer_id.get()))
            .arg(format!("--doorbell-fd={doorbell_fd}"))
            .arg(format!("--mmap-control-fd={mmap_rx_fd}"))
            .arg("--scenario=boundary-recv-ack")
            .arg(format!("--sizes={sizes_arg}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn swift guest: {e}"))?;

        let host_link = host_peer
            .into_link()
            .map_err(|e| format!("host into_link: {e}"))?;
        let (tx, mut rx) = host_link.split();

        for (index, &size) in boundary_sizes().iter().enumerate() {
            let payload = make_boundary_payload(index, size);
            let permit = tokio::time::timeout(Duration::from_secs(2), tx.reserve())
                .await
                .map_err(|_| format!("reserve timeout at size={size}"))?
                .map_err(|e| format!("reserve failed at size={size}: {e}"))?;
            let mut slot = permit
                .alloc(payload.len())
                .map_err(|e| format!("alloc failed at size={size}: {e}"))?;
            slot.as_mut_slice().copy_from_slice(&payload);
            slot.commit();

            let backing = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .map_err(|_| format!("recv ack timeout at size={size}"))?
                .map_err(|e| format!("recv ack failed at size={size}: {e}"))?
                .ok_or_else(|| format!("recv ack EOF at size={size}"))?;
            let (ack_len, ack_sum) = decode_ack(backing.as_bytes())
                .ok_or_else(|| format!("invalid ack payload at size={size}"))?;
            if ack_len != size as u32 || ack_sum != checksum(&payload) {
                return Err(format!(
                    "ack mismatch at size={size}: got len={ack_len} sum={ack_sum}"
                ));
            }
        }

        let tx_stats = tx.stats();
        if tx_stats.inline_sends != 3
            || tx_stats.slot_ref_sends != 2
            || tx_stats.mmap_ref_sends != 3
        {
            return Err(format!(
                "unexpected rust->swift framing stats: inline={} slot={} mmap={}",
                tx_stats.inline_sends, tx_stats.slot_ref_sends, tx_stats.mmap_ref_sends
            ));
        }

        let output = wait_child_with_timeout(child, Duration::from_secs(5)).await?;
        if !output.status.success() {
            return Err(format!(
                "swift guest failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok::<_, String>(())
    })
    .unwrap();
}

pub fn run_boundary_cutover_swift_to_rust_case() {
    run_async(async {
        let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
        let shm_path = dir.path().join("xlang-shm-boundary-swift-to-rust.shm");
        let class = [SizeClassConfig {
            slot_size: 4096,
            slot_count: 32,
        }];
        let config = SegmentConfig {
            max_guests: 1,
            bipbuf_capacity: 64 * 1024,
            max_payload_size: 1024 * 1024,
            inline_threshold: 64,
            heartbeat_interval: 0,
            size_classes: &class,
        };
        let segment = std::sync::Arc::new(
            Segment::create(Path::new(&shm_path), config, FileCleanup::Manual)
                .map_err(|e| format!("segment create: {e}"))?,
        );
        let hub = HostHub::new(segment);
        let prepared = hub
            .prepare_peer()
            .map_err(|e| format!("prepare peer: {e}"))?;
        let (host_peer, ticket) = prepared.into_parts();

        let doorbell_fd = ticket.doorbell.into_raw_fd();
        // Unused by this scenario; close parent copy.
        unsafe { libc::close(ticket.mmap_rx.into_raw_fd()) };

        let sizes_arg = boundary_sizes()
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let child = TokioCommand::new(swift_shm_guest_client_path())
            .arg(format!("--hub-path={}", shm_path.display()))
            .arg(format!("--peer-id={}", ticket.peer_id.get()))
            .arg(format!("--doorbell-fd={doorbell_fd}"))
            .arg(format!("--mmap-control-fd={}", ticket.mmap_tx_fd))
            .arg("--scenario=boundary-send-await-ack")
            .arg(format!("--sizes={sizes_arg}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn swift guest: {e}"))?;

        let host_link = host_peer
            .into_link()
            .map_err(|e| format!("host into_link: {e}"))?;
        let (tx, mut rx) = host_link.split();

        for (index, &size) in boundary_sizes().iter().enumerate() {
            let expected = make_boundary_payload(index, size);
            let backing = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .map_err(|_| format!("recv payload timeout at size={size}"))?
                .map_err(|e| format!("recv payload failed at size={size}: {e}"))?
                .ok_or_else(|| format!("recv payload EOF at size={size}"))?;
            if backing.as_bytes() != expected.as_slice() {
                return Err(format!("payload mismatch at size={size}"));
            }

            let ack = encode_ack(size, checksum(&expected));
            let permit = tokio::time::timeout(Duration::from_secs(2), tx.reserve())
                .await
                .map_err(|_| format!("reserve ack timeout at size={size}"))?
                .map_err(|e| format!("reserve ack failed at size={size}: {e}"))?;
            let mut slot = permit
                .alloc(ack.len())
                .map_err(|e| format!("alloc ack failed at size={size}: {e}"))?;
            slot.as_mut_slice().copy_from_slice(&ack);
            slot.commit();
        }

        let rx_stats = rx.stats();
        if rx_stats.inline_recvs != 2
            || rx_stats.slot_ref_recvs != 3
            || rx_stats.mmap_ref_recvs != 3
        {
            return Err(format!(
                "unexpected swift->rust framing stats: inline={} slot={} mmap={}",
                rx_stats.inline_recvs, rx_stats.slot_ref_recvs, rx_stats.mmap_ref_recvs
            ));
        }

        let output = wait_child_with_timeout(child, Duration::from_secs(5)).await?;
        if !output.status.success() {
            return Err(format!(
                "swift guest failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok::<_, String>(())
    })
    .unwrap();
}

pub fn run_fault_mmap_control_breakage_case() {
    run_async(async {
        let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
        let shm_path = dir.path().join("xlang-shm-fault-mmap-control-breakage.shm");
        let class = [SizeClassConfig {
            slot_size: 4096,
            slot_count: 4,
        }];
        let config = SegmentConfig {
            max_guests: 1,
            bipbuf_capacity: 64 * 1024,
            max_payload_size: 1024 * 1024,
            inline_threshold: 64,
            heartbeat_interval: 0,
            size_classes: &class,
        };
        let segment = std::sync::Arc::new(
            Segment::create(Path::new(&shm_path), config, FileCleanup::Manual)
                .map_err(|e| format!("segment create: {e}"))?,
        );
        let hub = HostHub::new(segment);
        let prepared = hub
            .prepare_peer()
            .map_err(|e| format!("prepare peer: {e}"))?;
        let (host_peer, ticket) = prepared.into_parts();
        drop(host_peer);

        let doorbell_fd = ticket.doorbell.into_raw_fd();
        // Unused by this scenario; close parent copy.
        unsafe { libc::close(ticket.mmap_rx.into_raw_fd()) };

        let child = TokioCommand::new(swift_shm_guest_client_path())
            .arg(format!("--hub-path={}", shm_path.display()))
            .arg(format!("--peer-id={}", ticket.peer_id.get()))
            .arg(format!("--doorbell-fd={doorbell_fd}"))
            .arg(format!("--mmap-control-fd={}", ticket.mmap_tx_fd))
            .arg("--scenario=fault-mmap-send-control-breakage")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn swift guest: {e}"))?;

        let output = wait_child_with_timeout(child, Duration::from_secs(3)).await?;
        if !output.status.success() {
            return Err(format!(
                "swift guest failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok::<_, String>(())
    })
    .unwrap();
}

pub fn run_fault_host_goodbye_wake_case() {
    run_async(async {
        let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
        let shm_path = dir.path().join("xlang-shm-fault-host-goodbye-wake.shm");
        let class = [SizeClassConfig {
            slot_size: 4096,
            slot_count: 4,
        }];
        let config = SegmentConfig {
            max_guests: 1,
            bipbuf_capacity: 64 * 1024,
            max_payload_size: 1024 * 1024,
            inline_threshold: 64,
            heartbeat_interval: 0,
            size_classes: &class,
        };
        let segment = Segment::create(Path::new(&shm_path), config, FileCleanup::Manual)
            .map_err(|e| format!("segment create: {e}"))?;
        let peer_id = segment
            .reserve_peer()
            .ok_or("reserve peer failed".to_string())?;
        let (host_fd, guest_fd) = make_socketpair();
        clear_cloexec(guest_fd).map_err(|e| format!("clear_cloexec guest doorbell: {e}"))?;

        let child = TokioCommand::new(swift_shm_guest_client_path())
            .arg(format!("--hub-path={}", shm_path.display()))
            .arg(format!("--peer-id={}", peer_id.get()))
            .arg(format!("--doorbell-fd={guest_fd}"))
            .arg("--scenario=fault-host-goodbye-wake")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn swift guest: {e}"))?;

        tokio::time::sleep(Duration::from_millis(50)).await;
        segment.set_host_goodbye();
        ring_doorbell(host_fd);

        let output = wait_child_with_timeout(child, Duration::from_secs(3)).await?;
        unsafe {
            libc::close(host_fd);
            libc::close(guest_fd);
        }
        if !output.status.success() {
            return Err(format!(
                "swift guest failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok::<_, String>(())
    })
    .unwrap();
}
