//! Test orchestrator for rapace spec conformance.
//!
//! This binary:
//! 1. Lists test cases from rapace-spec-peer
//! 2. For each test, spawns both peer and subject
//! 3. Proxies TCP traffic between them (recording for hex dump on failure)
//! 4. Relays stdout/stderr with [peer] and [subj] prefixes
//! 5. Reports pass/fail based on peer exit code

use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use libtest_mimic::{Arguments, Failed, Trial};
use owo_colors::OwoColorize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::sync::Mutex;

/// Test case from the spec tester
#[derive(facet::Facet)]
struct TestCase {
    name: String,
    rules: Vec<String>,
}

/// Recorded traffic for hex dump on failure
#[derive(Default)]
struct TrafficLog {
    /// (direction, bytes) - direction is "tester->subject" or "subject->tester"
    packets: Vec<(&'static str, Vec<u8>)>,
}

impl TrafficLog {
    fn record(&mut self, direction: &'static str, data: &[u8]) {
        self.packets.push((direction, data.to_vec()));
    }

    fn format_hex_dump(&self) -> String {
        let mut out = String::new();
        for (direction, data) in &self.packets {
            out.push_str(&format!("\n[{}] {} bytes:\n", direction, data.len()));
            out.push_str(&hexdump(data));
        }
        out
    }
}

fn hexdump(data: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        // Offset
        out.push_str(&format!("{:08x}  ", i * 16));

        // Hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            out.push_str(&format!("{:02x} ", byte));
            if j == 7 {
                out.push(' ');
            }
        }

        // Padding for incomplete lines
        for j in chunk.len()..16 {
            out.push_str("   ");
            if j == 7 {
                out.push(' ');
            }
        }

        out.push_str(" |");

        // ASCII
        for byte in chunk {
            if *byte >= 0x20 && *byte < 0x7f {
                out.push(*byte as char);
            } else {
                out.push('.');
            }
        }

        out.push_str("|\n");
    }
    out
}

fn main() {
    let args = Arguments::from_args();

    // Find binaries - they're in target/debug or target/release,
    // while we might be in target/debug/deps
    let self_exe = std::env::current_exe().expect("failed to get current exe");
    let mut bin_dir = self_exe.parent().expect("exe has no parent").to_path_buf();

    // If we're in deps/, go up one level
    if bin_dir.ends_with("deps") {
        bin_dir = bin_dir.parent().expect("deps has no parent").to_path_buf();
    }

    let peer_bin = bin_dir.join("rapace-spec-peer");
    let subject_bin = bin_dir.join("rapace-spec-subject");

    // Get test list from peer
    let output = match std::process::Command::new(&peer_bin)
        .args(["--list", "--format", "json"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Warning: spec-peer not found ({e}), returning empty test list");
            eprintln!("Run `cargo build -p rapace-spec-peer -p rapace-spec-subject` first");
            libtest_mimic::run(&args, vec![]).exit();
        }
    };

    if !output.status.success() {
        eprintln!("spec-peer --list failed:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let tests: Vec<TestCase> =
        facet_json::from_slice(&output.stdout).expect("failed to parse test list");

    eprintln!("Found {} test cases", tests.len());

    // Create trials
    let trials: Vec<Trial> = tests
        .into_iter()
        .map(|test| {
            let name = test.name.clone();
            let peer = peer_bin.clone();
            let subject = subject_bin.clone();

            Trial::test(name.clone(), move || run_test(&peer, &subject, &name))
        })
        .collect();

    libtest_mimic::run(&args, trials).exit();
}

fn run_test(peer: &std::path::Path, subject: &std::path::Path, case: &str) -> Result<(), Failed> {
    // Create a new tokio runtime for this test
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create runtime: {}", e))?;

    rt.block_on(run_test_async(peer, subject, case))
}

/// Format elapsed time as uptime-style string (e.g., "0.00123s", "1.23456s")
fn format_uptime(start: Instant) -> String {
    let elapsed = start.elapsed();
    format!("{:>8.5}s", elapsed.as_secs_f64())
}

async fn run_test_async(
    peer: &std::path::Path,
    subject: &std::path::Path,
    case: &str,
) -> Result<(), Failed> {
    let start = Instant::now();

    // Bind two TCP listeners on ephemeral ports
    let peer_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind peer listener: {}", e))?;
    let subject_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind subject listener: {}", e))?;

    let peer_addr = peer_listener
        .local_addr()
        .map_err(|e| format!("failed to get peer addr: {}", e))?;
    let subject_addr = subject_listener
        .local_addr()
        .map_err(|e| format!("failed to get subject addr: {}", e))?;

    eprintln!(
        "{} {} Starting test '{}': peer={}, subject={}",
        format_uptime(start),
        "[harn]".cyan(),
        case,
        peer_addr,
        subject_addr
    );
    eprintln!(
        "{} {} Spawning peer: {} --case {} (PEER_ADDR={})",
        format_uptime(start),
        "[harn]".cyan(),
        peer.display(),
        case,
        peer_addr
    );
    eprintln!(
        "{} {} Spawning subject: {} --case {} (PEER_ADDR={})",
        format_uptime(start),
        "[harn]".cyan(),
        subject.display(),
        case,
        subject_addr
    );

    // Spawn peer process
    let mut peer_proc = Command::new(peer)
        .args(["--case", case])
        .env("PEER_ADDR", peer_addr.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn peer: {}", e))?;

    // Spawn subject process
    let mut subject_proc = Command::new(subject)
        .args(["--case", case])
        .env("PEER_ADDR", subject_addr.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn subject: {}", e))?;

    // Take stdout/stderr handles
    let peer_stdout = peer_proc.stdout.take().unwrap();
    let peer_stderr = peer_proc.stderr.take().unwrap();
    let subject_stdout = subject_proc.stdout.take().unwrap();
    let subject_stderr = subject_proc.stderr.take().unwrap();

    // Spawn tasks to relay output with prefixes
    let relay_peer_stdout = tokio::spawn(relay_lines(peer_stdout, Source::Peer, start));
    let relay_peer_stderr = tokio::spawn(relay_lines(peer_stderr, Source::Peer, start));
    let relay_subject_stdout = tokio::spawn(relay_lines(subject_stdout, Source::Subject, start));
    let relay_subject_stderr = tokio::spawn(relay_lines(subject_stderr, Source::Subject, start));

    // Accept connections from both (with timeout)
    let accept_timeout = std::time::Duration::from_secs(10);

    let peer_conn = tokio::time::timeout(accept_timeout, peer_listener.accept())
        .await
        .map_err(|_| "peer didn't connect within timeout")?
        .map_err(|e| format!("failed to accept peer connection: {}", e))?
        .0;

    eprintln!(
        "{} {} Peer connected",
        format_uptime(start),
        "[harn]".cyan()
    );

    let subject_conn = tokio::time::timeout(accept_timeout, subject_listener.accept())
        .await
        .map_err(|_| "subject didn't connect within timeout")?
        .map_err(|e| format!("failed to accept subject connection: {}", e))?
        .0;

    eprintln!(
        "{} {} Subject connected, starting proxy",
        format_uptime(start),
        "[harn]".cyan()
    );

    // Set up traffic logging
    let traffic_log = Arc::new(Mutex::new(TrafficLog::default()));

    // Proxy traffic between peer and subject
    let proxy_result = proxy_connections(peer_conn, subject_conn, traffic_log.clone()).await;

    eprintln!(
        "{} {} Proxy completed: {:?}",
        format_uptime(start),
        "[harn]".cyan(),
        proxy_result
    );

    // Wait for processes to exit
    let peer_status = peer_proc
        .wait()
        .await
        .map_err(|e| format!("failed to wait for peer: {}", e))?;
    let subject_status = subject_proc
        .wait()
        .await
        .map_err(|e| format!("failed to wait for subject: {}", e))?;

    eprintln!(
        "{} {} Processes exited: peer={:?}, subject={:?}",
        format_uptime(start),
        "[harn]".cyan(),
        peer_status.code(),
        subject_status.code()
    );

    // Wait for relay tasks to complete
    let _ = relay_peer_stdout.await;
    let _ = relay_peer_stderr.await;
    let _ = relay_subject_stdout.await;
    let _ = relay_subject_stderr.await;

    // Check results
    if peer_status.success() {
        Ok(())
    } else {
        let log = traffic_log.lock().await;
        let hex_dump = if log.packets.is_empty() {
            "\n(no traffic recorded)".to_string()
        } else {
            log.format_hex_dump()
        };

        Err(Failed::from(format!(
            "peer exited {:?}, subject exited {:?}\nproxy result: {:?}\n\nTraffic dump:{}",
            peer_status.code(),
            subject_status.code(),
            proxy_result,
            hex_dump
        )))
    }
}

#[derive(Clone, Copy)]
enum Source {
    Peer,
    Subject,
}

impl Source {
    fn format(&self) -> String {
        match self {
            Source::Peer => format!("{}", "[peer]".green()),
            Source::Subject => format!("{}", "[subj]".yellow()),
        }
    }
}

/// Relay lines from a reader to stderr with a prefix and uptime
async fn relay_lines<R: tokio::io::AsyncRead + Unpin>(reader: R, source: Source, start: Instant) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        eprintln!("{} {} {}", format_uptime(start), source.format(), line);
    }
}

async fn proxy_connections(
    peer_conn: TcpStream,
    subject_conn: TcpStream,
    traffic_log: Arc<Mutex<TrafficLog>>,
) -> Result<(), String> {
    let (peer_read, peer_write) = peer_conn.into_split();
    let (subject_read, subject_write) = subject_conn.into_split();

    let log1 = traffic_log.clone();
    let log2 = traffic_log.clone();

    // peer -> subject
    let p2s = tokio::spawn(async move {
        proxy_one_direction(peer_read, subject_write, "peer->subject", log1).await
    });

    // subject -> peer
    let s2p = tokio::spawn(async move {
        proxy_one_direction(subject_read, peer_write, "subject->peer", log2).await
    });

    // Wait for both to complete
    let (r1, r2) = tokio::join!(p2s, s2p);

    r1.map_err(|e| format!("p2s task panicked: {}", e))?;
    r2.map_err(|e| format!("s2p task panicked: {}", e))?;

    Ok(())
}

async fn proxy_one_direction(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    direction: &'static str,
    traffic_log: Arc<Mutex<TrafficLog>>,
) {
    let mut buf = vec![0u8; 65536];

    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                // Record traffic
                {
                    let mut log = traffic_log.lock().await;
                    log.record(direction, &buf[..n]);
                }

                // Forward data
                if writer.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // Shutdown write side to signal EOF to the other end
    let _ = writer.shutdown().await;
}
