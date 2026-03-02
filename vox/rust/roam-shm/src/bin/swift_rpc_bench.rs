#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("swift_rpc_bench is only supported on macOS");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::SystemTime;
    use std::time::{Duration, Instant};

    use facet_postcard::{from_slice_borrowed, to_vec};
    use roam_shm::framing::{OwnedFrame, read_frame, write_inline};
    use roam_shm::segment::{Segment, SegmentConfig};
    use roam_shm::varslot::SizeClassConfig;
    use roam_types::{
        ConnectionId, Message, MessagePayload, MethodId, Payload, RequestBody, RequestCall,
        RequestId, RequestMessage,
    };
    use shm_primitives::FileCleanup;
    use shm_primitives_async::clear_cloexec;

    fn swift_runtime_package_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../swift/roam-runtime")
            .canonicalize()
            .expect("swift runtime package path")
    }

    fn swift_shm_guest_client_path() -> std::path::PathBuf {
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

        panic!("shm-guest-client binary not found; build swift/roam-runtime target first");
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
        let one: [u8; 1] = [1];
        let rc = unsafe { libc::send(fd, one.as_ptr().cast(), 1, libc::MSG_DONTWAIT) };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            match err.raw_os_error() {
                Some(code)
                    if code == libc::EAGAIN
                        || code == libc::EWOULDBLOCK
                        || code == libc::EPIPE
                        || code == libc::ECONNRESET
                        || code == libc::ENOTCONN => {}
                _ => panic!("doorbell send failed: {err}"),
            }
        }
    }

    fn parse_args() -> (usize, usize) {
        let mut iterations: usize = 10_000;
        let mut payload_size: usize = 256;

        for arg in std::env::args().skip(1) {
            if let Some(v) = arg.strip_prefix("--iterations=") {
                iterations = v.parse().expect("invalid --iterations");
            } else if let Some(v) = arg.strip_prefix("--payload-size=") {
                payload_size = v.parse().expect("invalid --payload-size");
            }
        }
        (iterations, payload_size)
    }

    fn decode_message(bytes: &[u8]) -> Message<'_> {
        if let Ok(msg) = from_slice_borrowed::<Message<'_>>(bytes) {
            return msg;
        }

        for pad in 1..=3 {
            if bytes.len() <= pad {
                break;
            }
            let suffix = &bytes[bytes.len() - pad..];
            if suffix.iter().all(|&b| b == 0) {
                let trimmed = &bytes[..bytes.len() - pad];
                if let Ok(msg) = from_slice_borrowed::<Message<'_>>(trimmed) {
                    return msg;
                }
            } else {
                break;
            }
        }

        panic!("failed to decode Message payload from frame")
    }

    fn percentile(mut values_us: Vec<u128>, pct: f64) -> u128 {
        values_us.sort_unstable();
        if values_us.is_empty() {
            return 0;
        }
        let idx = ((values_us.len() - 1) as f64 * pct).round() as usize;
        values_us[idx]
    }

    let (iterations, payload_size) = parse_args();
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system time before epoch")
        .as_nanos();
    let shm_path = std::env::temp_dir().join(format!(
        "roam-swift-rpc-bench-{}-{}.shm",
        std::process::id(),
        stamp
    ));
    let classes = [SizeClassConfig {
        slot_size: 4096,
        slot_count: 32,
    }];
    let config = SegmentConfig {
        max_guests: 1,
        bipbuf_capacity: 64 * 1024,
        max_payload_size: 4096,
        inline_threshold: 256,
        heartbeat_interval: 0,
        size_classes: &classes,
    };
    let segment = Segment::create(Path::new(&shm_path), config, FileCleanup::Manual).unwrap();

    let peer_id = segment.reserve_peer().expect("reserve peer slot");
    let (host_fd, guest_fd) = make_socketpair();
    clear_cloexec(guest_fd).expect("clear close-on-exec");

    let child = Command::new(swift_shm_guest_client_path())
        .arg(format!("--hub-path={}", shm_path.display()))
        .arg(format!("--peer-id={}", peer_id.get()))
        .arg(format!("--doorbell-fd={guest_fd}"))
        .arg("--size-class=4096:32")
        .arg(format!("--iterations={iterations}"))
        .arg("--scenario=rpc-bench-echo")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn swift shm guest client");

    let g2h = segment.g2h_bipbuf(peer_id);
    let h2g = segment.h2g_bipbuf(peer_id);
    let (_g2h_tx, mut g2h_rx) = g2h.split();
    let (mut h2g_tx, _h2g_rx) = h2g.split();
    let mut latencies_us = Vec::with_capacity(iterations);
    let total_start = Instant::now();

    for i in 0..iterations {
        let req_id = (i as u64) + 1;
        let payload = vec![0xA5u8; payload_size];
        let msg = Message {
            connection_id: ConnectionId(2),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(req_id),
                body: RequestBody::Call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&payload),
                    channels: Vec::new(),
                    metadata: vec![],
                }),
            }),
        };
        let req_bytes = to_vec(&msg).expect("encode request");

        let t0 = Instant::now();
        write_inline(&mut h2g_tx, &req_bytes).expect("write request");
        ring_doorbell(host_fd);

        let deadline = Instant::now() + Duration::from_secs(5);
        let resp_bytes = loop {
            if Instant::now() > deadline {
                panic!("timed out waiting for response {req_id}");
            }
            if let Some(frame) = read_frame(&mut g2h_rx) {
                match frame {
                    OwnedFrame::Inline(bytes) => break bytes,
                    OwnedFrame::SlotRef(slot_ref) => {
                        let raw = unsafe { segment.var_pool().slot_data(&slot_ref) };
                        let len = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
                        let payload = raw[4..4 + len].to_vec();
                        let _ = segment.var_pool().free(slot_ref);
                        break payload;
                    }
                    OwnedFrame::MmapRef(_) => {
                        panic!("unexpected mmap-ref response frame")
                    }
                }
            }
            std::thread::sleep(Duration::from_micros(50));
        };

        let response = decode_message(&resp_bytes);
        match response.payload {
            MessagePayload::RequestMessage(RequestMessage {
                id,
                body: RequestBody::Response(_),
            }) => assert_eq!(id.0, req_id),
            _ => panic!("unexpected response payload"),
        }
        latencies_us.push(t0.elapsed().as_micros());
    }

    let total_elapsed = total_start.elapsed();
    let output = child.wait_with_output().expect("wait for swift guest");
    unsafe {
        libc::close(host_fd);
        libc::close(guest_fd);
    }
    if !output.status.success() {
        panic!(
            "swift guest failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = std::fs::remove_file(&shm_path);

    let throughput = iterations as f64 / total_elapsed.as_secs_f64();
    let p50 = percentile(latencies_us.clone(), 0.50);
    let p95 = percentile(latencies_us.clone(), 0.95);
    let p99 = percentile(latencies_us, 0.99);

    println!("swift-rpc-bench");
    println!("iterations: {iterations}");
    println!("payload_size: {payload_size} bytes");
    println!("elapsed: {:.3}s", total_elapsed.as_secs_f64());
    println!("throughput: {:.2} req/s", throughput);
    println!("latency_us: p50={p50} p95={p95} p99={p99}");
}
