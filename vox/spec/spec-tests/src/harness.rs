use std::future::Future;
#[cfg(unix)]
use std::io::Write as _;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::fd::{AsRawFd, IntoRawFd};

use roam::{Rx, Tx};
use roam_core::{DriverReplySink, acceptor, acceptor_transport, initiator, memory_link_pair};
use roam_shm::HostHub;
use roam_shm::ShmLink;
use roam_shm::bootstrap::{BootstrapStatus, decode_request, encode_request};
#[cfg(windows)]
use roam_shm::guest_link_from_names;
#[cfg(unix)]
use roam_shm::guest_link_from_raw;
use roam_shm::segment::{Segment, SegmentConfig};
use roam_shm::varslot::SizeClassConfig as RoamShmSizeClassConfig;
use roam_stream::StreamLink;
use roam_types::{RequestCall, SelfRef};
use shm_primitives::FileCleanup;
use shm_primitives::SizeClassConfig;
use spec_proto::{
    Canvas, Color, LookupError, MathError, Message, Person, Point, Rectangle, Shape, Testbed,
    TestbedClient, TestbedDispatcher,
};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;

const SUBJECT_WAIT_HEARTBEAT: Duration = Duration::from_millis(500);
/// Spawn a task that catches panics and makes them loud.
///
/// If the spawned future panics, the panic message is printed to stderr
/// immediately and then re-raised. This prevents the silent-task-panic
/// problem where tokio tasks panic and nobody notices, causing mysterious
/// timeouts in tests.
pub fn spawn_loud<F>(fut: F) -> moire::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    moire::task::spawn(async move {
        // Inner spawn so we can catch the panic via JoinError
        let inner = tokio::task::spawn(fut);
        match inner.await {
            Ok(v) => v,
            Err(e) if e.is_panic() => {
                let panic = e.into_panic();
                let msg = panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| format!("{panic:?}"));
                eprintln!("\n\n!!! SPAWNED TASK PANICKED !!!\n{msg}\n");
                std::panic::resume_unwind(panic);
            }
            Err(e) => {
                panic!("spawned task failed: {e}");
            }
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubjectLanguage {
    Rust,
    Swift,
    TypeScript,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubjectTestTransport {
    Tcp,
    Shm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubjectShmMode {
    GuestServer,
    HostServer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubjectSpec {
    pub language: SubjectLanguage,
    pub transport: SubjectTestTransport,
    pub shm_mode: SubjectShmMode,
}

impl SubjectSpec {
    pub const fn tcp(language: SubjectLanguage) -> Self {
        Self {
            language,
            transport: SubjectTestTransport::Tcp,
            shm_mode: SubjectShmMode::GuestServer,
        }
    }

    pub const fn shm_guest(language: SubjectLanguage) -> Self {
        Self {
            language,
            transport: SubjectTestTransport::Shm,
            shm_mode: SubjectShmMode::GuestServer,
        }
    }

    pub const fn shm_host(language: SubjectLanguage) -> Self {
        Self {
            language,
            transport: SubjectTestTransport::Shm,
            shm_mode: SubjectShmMode::HostServer,
        }
    }
}

struct NoopHandler;

impl roam_types::Handler<DriverReplySink> for NoopHandler {
    async fn handle(&self, _call: SelfRef<RequestCall<'static>>, _reply: DriverReplySink) {}
}

pub fn workspace_root() -> &'static std::path::Path {
    // `spec/spec-tests` → `spec` → workspace root
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

pub fn subject_cmd() -> String {
    match std::env::var("SUBJECT_CMD") {
        Ok(s) if !s.trim().is_empty() => s,
        _ => subject_cmd_for_language(SubjectLanguage::Rust),
    }
}

pub fn subject_cmd_for_language(language: SubjectLanguage) -> String {
    match language {
        SubjectLanguage::Rust => {
            let exe = format!("subject-rust{}", std::env::consts::EXE_SUFFIX);
            let debug = workspace_root().join("target").join("debug").join(&exe);
            if debug.exists() {
                debug.display().to_string()
            } else {
                workspace_root()
                    .join("target")
                    .join("release")
                    .join(&exe)
                    .display()
                    .to_string()
            }
        }
        SubjectLanguage::Swift => "./swift/subject/subject-swift.sh".to_string(),
        SubjectLanguage::TypeScript => "./typescript/subject/subject-ts.sh".to_string(),
    }
}

fn subject_transport() -> SubjectTestTransport {
    match std::env::var("SPEC_TRANSPORT")
        .ok()
        .unwrap_or_else(|| "tcp".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "shm" => SubjectTestTransport::Shm,
        _ => SubjectTestTransport::Tcp,
    }
}

fn shm_subject_mode() -> SubjectShmMode {
    let mode = std::env::var("SPEC_SHM_SUBJECT_MODE")
        .ok()
        .unwrap_or_else(|| "shm-server".to_string())
        .to_ascii_lowercase();
    if mode == "shm-host-server" {
        SubjectShmMode::HostServer
    } else {
        SubjectShmMode::GuestServer
    }
}

pub fn run_async<T>(f: impl Future<Output = T>) -> T {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(f)
}

#[derive(Clone)]
struct TestbedService;

impl Testbed for TestbedService {
    async fn echo(&self, message: String) -> String {
        message
    }

    async fn reverse(&self, message: String) -> String {
        message.chars().rev().collect()
    }

    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError> {
        if divisor == 0 {
            Err(MathError::DivisionByZero)
        } else {
            Ok(dividend / divisor)
        }
    }

    async fn lookup(&self, id: u32) -> Result<Person, LookupError> {
        match id {
            1 => Ok(Person {
                name: "Alice".to_string(),
                age: 30,
                email: Some("alice@example.com".to_string()),
            }),
            2 => Ok(Person {
                name: "Bob".to_string(),
                age: 25,
                email: None,
            }),
            3 => Ok(Person {
                name: "Charlie".to_string(),
                age: 35,
                email: Some("charlie@example.com".to_string()),
            }),
            _ => Err(LookupError::NotFound),
        }
    }

    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            total += *n as i64;
        }
        total
    }

    async fn generate(&self, count: u32, output: Tx<i32>) {
        for i in 0..count as i32 {
            if output.send(i).await.is_err() {
                break;
            }
        }
        output.close(Default::default()).await.ok();
    }

    async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
        while let Ok(Some(s)) = input.recv().await {
            let _ = output.send(s.clone()).await;
        }
        output.close(Default::default()).await.ok();
    }

    async fn echo_point(&self, point: Point) -> Point {
        point
    }

    async fn create_person(&self, name: String, age: u8, email: Option<String>) -> Person {
        Person { name, age, email }
    }

    async fn rectangle_area(&self, rect: Rectangle) -> f64 {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        width * height
    }

    async fn parse_color(&self, name: String) -> Option<Color> {
        match name.to_lowercase().as_str() {
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "blue" => Some(Color::Blue),
            _ => None,
        }
    }

    async fn shape_area(&self, shape: Shape) -> f64 {
        match shape {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Point => 0.0,
        }
    }

    async fn create_canvas(&self, name: String, shapes: Vec<Shape>, background: Color) -> Canvas {
        Canvas {
            name,
            shapes,
            background,
        }
    }

    async fn process_message(&self, msg: Message) -> Message {
        match msg {
            Message::Text(s) => Message::Text(format!("processed: {s}")),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(d) => Message::Data(d.into_iter().rev().collect()),
        }
    }

    async fn get_points(&self, count: u32) -> Vec<Point> {
        (0..count as i32)
            .map(|i| Point { x: i, y: i * 2 })
            .collect()
    }

    async fn swap_pair(&self, pair: (i32, String)) -> (String, i32) {
        (pair.1, pair.0)
    }
}

/// Spawn the subject binary, telling it to connect to `peer_addr`.
pub async fn spawn_subject(peer_addr: &str) -> Result<Child, String> {
    spawn_subject_cmd_with_env(&subject_cmd(), peer_addr, &[]).await
}

fn spawn_subject_log_pump<R>(reader: R, pid: u32, stream: &'static str)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => eprintln!("[subject:{pid}:{stream}] {line}"),
                Ok(None) => break,
                Err(err) => {
                    eprintln!("[subject:{pid}:{stream}] log read error: {err}");
                    break;
                }
            }
        }
    });
}

async fn spawn_subject_cmd_with_env(
    cmd: &str,
    peer_addr: &str,
    extra_env: &[(&str, &str)],
) -> Result<Child, String> {
    let extra_env_desc = if extra_env.is_empty() {
        "<none>".to_string()
    } else {
        extra_env
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    eprintln!("[subject:spawn] cmd={cmd:?} peer_addr={peer_addr:?} extra_env={extra_env_desc}");

    let mut command = if cmd.ends_with(".sh") {
        let mut c = Command::new("sh");
        c.arg("-lc").arg(cmd);
        c
    } else {
        Command::new(cmd)
    };
    command
        .current_dir(workspace_root())
        .env("PEER_ADDR", peer_addr)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        command.env(k, v);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to spawn subject: {e}"))?;
    let pid = child.id().unwrap_or_default();
    eprintln!("[subject:{pid}] spawned");

    if let Some(stdout) = child.stdout.take() {
        spawn_subject_log_pump(stdout, pid, "stdout");
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_subject_log_pump(stderr, pid, "stderr");
    }

    // If it exits immediately, surface that early.
    tokio::time::sleep(Duration::from_millis(10)).await;
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
        eprintln!("[subject:{pid}] exited immediately: {status}");
        return Err(format!("subject exited immediately with {status}"));
    }

    Ok(child)
}

fn sid_hex_32() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    format!("{nanos:032x}")
}

fn leaked_dirs() -> &'static Mutex<Vec<tempfile::TempDir>> {
    static DIRS: OnceLock<Mutex<Vec<tempfile::TempDir>>> = OnceLock::new();
    DIRS.get_or_init(|| Mutex::new(Vec::new()))
}

fn keep_tempdir_alive(dir: tempfile::TempDir) {
    leaked_dirs().lock().expect("tempdir mutex").push(dir);
}

/// Listen on a random TCP port, spawn the subject (which connects to us),
/// complete the roam handshake as acceptor, and return a ready `TestbedClient`.
pub async fn accept_subject() -> Result<(TestbedClient, Child), String> {
    let spec = SubjectSpec {
        language: SubjectLanguage::Rust,
        transport: subject_transport(),
        shm_mode: shm_subject_mode(),
    };
    accept_subject_spec(spec).await
}

pub async fn accept_subject_spec(spec: SubjectSpec) -> Result<(TestbedClient, Child), String> {
    let cmd = subject_cmd_for_language(spec.language);
    match spec.transport {
        SubjectTestTransport::Tcp => accept_subject_tcp(&cmd).await,
        SubjectTestTransport::Shm => match spec.shm_mode {
            SubjectShmMode::GuestServer => accept_subject_shm_subject_is_guest(&cmd).await,
            SubjectShmMode::HostServer => accept_subject_shm_subject_is_host(&cmd).await,
        },
    }
}

pub async fn accept_subject_with_transport(
    transport: SubjectTestTransport,
) -> Result<(TestbedClient, Child), String> {
    accept_subject_spec(SubjectSpec {
        language: SubjectLanguage::Rust,
        transport,
        shm_mode: shm_subject_mode(),
    })
    .await
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustTransport {
    Mem,
    Tcp,
    Shm,
}

pub async fn accept_rust_inproc(transport: RustTransport) -> Result<TestbedClient, String> {
    match transport {
        RustTransport::Mem => {
            let (a, b) = memory_link_pair(64 * 1024);
            accept_rust_inproc_with_conduits(a, b).await
        }
        RustTransport::Tcp => {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(|e| format!("bind: {e}"))?;
            let addr = listener
                .local_addr()
                .map_err(|e| format!("local_addr: {e}"))?;
            let connect_task =
                tokio::spawn(async move { tokio::net::TcpStream::connect(addr).await });
            let (server_stream, _) = listener
                .accept()
                .await
                .map_err(|e| format!("accept: {e}"))?;
            let client_stream = connect_task
                .await
                .map_err(|e| format!("connect task join: {e}"))?
                .map_err(|e| format!("connect: {e}"))?;
            server_stream.set_nodelay(true).unwrap();
            client_stream.set_nodelay(true).unwrap();
            accept_rust_inproc_with_conduits(
                StreamLink::tcp(client_stream),
                StreamLink::tcp(server_stream),
            )
            .await
        }
        RustTransport::Shm => {
            let classes = [RoamShmSizeClassConfig {
                slot_size: 4096,
                slot_count: 64,
            }];
            let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
            let path = dir.path().join("spec-test.shm");
            let segment = Arc::new(
                Segment::create(
                    &path,
                    SegmentConfig {
                        max_guests: 1,
                        bipbuf_capacity: 1 << 16,
                        max_payload_size: 1 << 20,
                        inline_threshold: 256,
                        heartbeat_interval: 0,
                        size_classes: &classes,
                    },
                    FileCleanup::Manual,
                )
                .map_err(|e| format!("shm segment create: {e}"))?,
            );
            let (a, b) = roam_shm::create_test_link_pair(segment)
                .await
                .map_err(|e| format!("shm create_test_link_pair: {e}"))?;
            // Keep the temp dir alive for the test duration by leaking it.
            std::mem::forget(dir);
            accept_rust_inproc_with_conduits(a, b).await
        }
    }
}

async fn accept_rust_inproc_with_conduits<L>(
    client_link: L,
    server_link: L,
) -> Result<TestbedClient, String>
where
    L: roam_types::Link + Send + 'static,
    L::Tx: Send + 'static,
    L::Rx: Send + 'static,
    <L::Rx as roam_types::LinkRx>::Error: std::error::Error + Send + Sync + 'static,
{
    let (server_ready_tx, server_ready_rx) = oneshot::channel::<Result<(), String>>();
    let _server_task = tokio::spawn(async move {
        let setup = acceptor(server_link)
            .establish::<TestbedClient>(TestbedDispatcher::new(TestbedService))
            .await
            .map_err(|e| format!("server handshake: {e}"));
        let (server_caller_guard, _sh) = match setup {
            Ok(parts) => parts,
            Err(err) => {
                let _ = server_ready_tx.send(Err(err));
                return;
            }
        };

        let _ = server_ready_tx.send(Ok(()));
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator(client_link)
        .establish::<TestbedClient>(NoopHandler)
        .await
        .map_err(|e| format!("client handshake: {e}"))?;

    server_ready_rx
        .await
        .map_err(|e| format!("server task join: {e}"))??;

    Ok(client)
}

async fn accept_subject_tcp(cmd: &str) -> Result<(TestbedClient, Child), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    let mut child = spawn_subject_cmd_with_env(cmd, &addr.to_string(), &[]).await?;
    let pid = child.id().unwrap_or_default();
    let wait_started = tokio::time::Instant::now();
    let wait_deadline = wait_started + Duration::from_secs(5);
    let mut heartbeat = tokio::time::interval(SUBJECT_WAIT_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;

    let (stream, _) = loop {
        tokio::select! {
            accepted = listener.accept() => {
                break accepted.map_err(|e| format!("accept: {e}"))?;
            }
            status = child.wait() => {
                let status = status.map_err(|e| format!("wait on subject process: {e}"))?;
                return Err(format!("subject exited before connecting: {status}"));
            }
            _ = tokio::time::sleep_until(wait_deadline) => {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("try_wait on subject process: {e}"))?
                {
                    return Err(format!("subject exited before connecting: {status}"));
                }
                return Err(format!(
                    "subject did not connect within 5s (pid={pid}, addr={addr}, elapsed={:?})",
                    wait_started.elapsed()
                ));
            }
            _ = heartbeat.tick() => {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("try_wait on subject process: {e}"))?
                {
                    return Err(format!("subject exited while waiting for tcp connect: {status}"));
                }
                eprintln!(
                    "[subject:{pid}] waiting for tcp connect to {addr} (elapsed={:?})",
                    wait_started.elapsed()
                );
            }
        }
    };
    stream.set_nodelay(true).unwrap();

    let (client, _sh) = acceptor_transport(StreamLink::tcp(stream))
        .establish::<TestbedClient>(NoopHandler)
        .await
        .map_err(|e| format!("handshake: {e}"))?;

    Ok((client, child))
}

async fn accept_subject_shm_subject_is_guest(cmd: &str) -> Result<(TestbedClient, Child), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let sid = sid_hex_32();
    let control_sock_path = dir.path().join("bootstrap.sock");
    let shm_path = dir.path().join("subject.shm");

    let size_classes = [SizeClassConfig {
        slot_size: 4096,
        slot_count: 8,
    }];
    let segment = Arc::new(
        Segment::create(
            &shm_path,
            SegmentConfig {
                max_guests: 1,
                bipbuf_capacity: 64 * 1024,
                max_payload_size: 1024 * 1024,
                inline_threshold: 256,
                heartbeat_interval: 0,
                size_classes: &size_classes,
            },
            FileCleanup::Manual,
        )
        .map_err(|e| format!("segment create: {e}"))?,
    );
    let hub = Arc::new(HostHub::new(Arc::clone(&segment)));

    // Bind the control listener.
    #[cfg(unix)]
    let listener = roam_local::LocalListener::bind(&control_sock_path)
        .map_err(|e| format!("bind {}: {e}", control_sock_path.display()))?;
    #[cfg(windows)]
    let mut listener = {
        let endpoint = roam_local::path_to_pipe_name(&control_sock_path);
        roam_local::LocalListener::bind(&endpoint).map_err(|e| format!("bind control pipe: {e}"))?
    };

    let hub_path_str = shm_path
        .to_str()
        .ok_or_else(|| format!("invalid shm path: {}", shm_path.display()))?;
    let hub_path_bytes = hub_path_str.as_bytes().to_vec();
    let prepared = hub
        .prepare_bootstrap_success(&hub_path_bytes)
        .map_err(|e| format!("prepare bootstrap success: {e}"))?;
    let mmap_tx_arg_env = prepared.guest_ticket.mmap_tx_arg();

    // Determine the control socket string and mmap env var for the subject.
    #[cfg(unix)]
    let control_sock = control_sock_path
        .to_str()
        .ok_or_else(|| format!("invalid socket path: {}", control_sock_path.display()))?
        .to_string();
    #[cfg(windows)]
    let control_sock = roam_local::path_to_pipe_name(&control_sock_path);

    let (peer_tx, peer_rx) = oneshot::channel();
    let sid_for_task = sid.clone();
    let segment_for_task = Arc::clone(&segment);
    tokio::spawn(async move {
        let result: Result<roam_shm::host::HostPeer, String> = async {
            let mut stream = listener
                .accept()
                .await
                .map_err(|e| format!("accept: {e}"))?;
            let mut request_buf = [0u8; 2048];
            let n = stream
                .read(&mut request_buf)
                .await
                .map_err(|e| format!("read bootstrap request: {e}"))?;
            if n == 0 {
                return Err("bootstrap request EOF".to_string());
            }
            let request = decode_request(&request_buf[..n])
                .map_err(|e| format!("decode bootstrap request: {e}"))?;
            let got_sid = String::from_utf8(request.sid.to_vec())
                .map_err(|e| format!("sid not utf-8: {e}"))?;
            if got_sid != sid_for_task {
                return Err(format!(
                    "sid mismatch: expected {sid_for_task}, got {got_sid}"
                ));
            }

            #[cfg(unix)]
            {
                prepared
                    .send_success_unix(stream.as_raw_fd(), &segment_for_task)
                    .map_err(|e| format!("send bootstrap success: {e}"))?;
            }
            #[cfg(windows)]
            {
                use roam_shm::bootstrap::{
                    BootstrapStatus, BootstrapSuccessNames, encode_response,
                };
                use tokio::io::AsyncWriteExt;
                let names = BootstrapSuccessNames {
                    segment_path: segment_for_task.path().to_str().unwrap().to_string(),
                    doorbell_name: prepared.guest_ticket.doorbell_arg(),
                    mmap_ctrl_name: prepared.guest_ticket.mmap_rx_arg(),
                };
                let payload = names.encode();
                let frame = encode_response(
                    BootstrapStatus::Success,
                    prepared.guest_ticket.peer_id.get() as u32,
                    &payload,
                )
                .map_err(|e| format!("encode bootstrap response: {e}"))?;
                stream
                    .write_all(&frame)
                    .await
                    .map_err(|e| format!("send bootstrap success: {e}"))?;
            }

            Ok(prepared.host_peer)
        }
        .await;
        let _ = peer_tx.send(result);
    });

    // Build the env vars for the subject process.
    #[cfg(unix)]
    let extra_env: Vec<(&str, &str)> = vec![
        ("SUBJECT_MODE", "shm-server"),
        ("SHM_CONTROL_SOCK", &control_sock),
        ("SHM_SESSION_ID", &sid),
        ("SHM_MMAP_TX_FD", &mmap_tx_arg_env),
    ];
    #[cfg(windows)]
    let extra_env: Vec<(&str, &str)> = vec![
        ("SUBJECT_MODE", "shm-server"),
        ("SHM_CONTROL_SOCK", &control_sock),
        ("SHM_SESSION_ID", &sid),
        ("SHM_MMAP_TX_PIPE", &mmap_tx_arg_env),
    ];

    let mut child = spawn_subject_cmd_with_env(cmd, "", &extra_env).await?;

    let mut peer_rx = peer_rx;
    let pid = child.id().unwrap_or_default();
    let wait_started = tokio::time::Instant::now();
    let wait_deadline = wait_started + Duration::from_secs(5);
    let mut heartbeat = tokio::time::interval(SUBJECT_WAIT_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;

    let host_peer = loop {
        tokio::select! {
            peer = &mut peer_rx => {
                break peer.map_err(|_| "bootstrap task dropped".to_string())??;
            }
            status = child.wait() => {
                let status = status.map_err(|e| format!("wait on subject process: {e}"))?;
                return Err(format!("subject exited before bootstrap request: {status}"));
            }
            _ = tokio::time::sleep_until(wait_deadline) => {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("try_wait on subject process: {e}"))?
                {
                    return Err(format!("subject exited before bootstrap request: {status}"));
                }
                return Err(format!(
                    "timed out waiting for bootstrap request (pid={pid}, socket={}, elapsed={:?})",
                    control_sock_path.display(),
                    wait_started.elapsed()
                ));
            }
            _ = heartbeat.tick() => {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("try_wait on subject process: {e}"))?
                {
                    return Err(format!("subject exited while waiting for bootstrap request: {status}"));
                }
                eprintln!(
                    "[subject:{pid}] waiting for bootstrap request on {} (elapsed={:?})",
                    control_sock_path.display(),
                    wait_started.elapsed()
                );
            }
        }
    };

    eprintln!("[harness] into_link...");
    let link = host_peer
        .into_link()
        .map_err(|e| format!("host peer to link: {e}"))?;
    eprintln!("[harness] into_link ok");
    #[cfg(windows)]
    {
        eprintln!("[harness] accept_doorbell...");
        link.accept_doorbell()
            .await
            .map_err(|e| format!("accept doorbell: {e}"))?;
        eprintln!("[harness] accept_doorbell ok");
    }
    eprintln!("[harness] handshake...");
    let (client, _sh) = acceptor(link)
        .establish::<TestbedClient>(NoopHandler)
        .await
        .map_err(|e| format!("handshake: {e}"))?;
    eprintln!("[harness] handshake ok");

    keep_tempdir_alive(dir);
    Ok((client, child))
}

async fn accept_subject_shm_subject_is_host(cmd: &str) -> Result<(TestbedClient, Child), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let sid = sid_hex_32();
    let control_sock_path = dir.path().join("bootstrap.sock");
    let shm_path = dir.path().join("subject.shm");

    #[cfg(unix)]
    let control_sock = control_sock_path
        .to_str()
        .ok_or_else(|| format!("invalid socket path: {}", control_sock_path.display()))?
        .to_string();
    #[cfg(windows)]
    let control_sock = roam_local::path_to_pipe_name(&control_sock_path);

    let shm_path_str = shm_path
        .to_str()
        .ok_or_else(|| format!("invalid shm path: {}", shm_path.display()))?
        .to_string();

    let mut child = spawn_subject_cmd_with_env(
        cmd,
        "",
        &[
            ("SUBJECT_MODE", "shm-host-server"),
            ("SHM_CONTROL_SOCK", &control_sock),
            ("SHM_SESSION_ID", &sid),
            ("SHM_HUB_PATH", &shm_path_str),
        ],
    )
    .await?;
    let pid = child.id().unwrap_or_default();

    let setup_result: Result<TestbedClient, String> = async {
        eprintln!(
            "[subject:{pid}] waiting for subject-host bootstrap socket {}",
            control_sock_path.display()
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let connect_started = tokio::time::Instant::now();
        let mut heartbeat = tokio::time::interval(SUBJECT_WAIT_HEARTBEAT);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.tick().await;

        // Connect to the subject's bootstrap socket.
        #[cfg(unix)]
        let mut stream = {
            use std::os::unix::net::UnixStream as StdUnixStream;
            loop {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("try_wait on subject process: {e}"))?
                {
                    return Err(format!(
                        "subject exited before bootstrap handshake: {status}"
                    ));
                }
                match StdUnixStream::connect(&control_sock_path) {
                    Ok(stream) => {
                        eprintln!(
                            "[subject:{pid}] connected to bootstrap socket {}",
                            control_sock_path.display()
                        );
                        break stream;
                    }
                    Err(e) => {
                        if tokio::time::Instant::now() >= deadline {
                            return Err(format!(
                                "connect bootstrap socket {} failed after {:?}: {e}",
                                control_sock_path.display(),
                                connect_started.elapsed()
                            ));
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
                            _ = heartbeat.tick() => {
                                if let Some(status) = child
                                    .try_wait()
                                    .map_err(|e| format!("try_wait on subject process: {e}"))?
                                {
                                    return Err(format!(
                                        "subject exited while waiting for bootstrap socket: {status}"
                                    ));
                                }
                                eprintln!(
                                    "[subject:{pid}] waiting for bootstrap socket {} (elapsed={:?}, latest_error={e})",
                                    control_sock_path.display(),
                                    connect_started.elapsed()
                                );
                            }
                        }
                    }
                }
            }
        };

        #[cfg(windows)]
        let mut stream = {
            let pipe_name = roam_local::path_to_pipe_name(&control_sock_path);
            loop {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("try_wait on subject process: {e}"))?
                {
                    return Err(format!(
                        "subject exited before bootstrap handshake: {status}"
                    ));
                }
                match roam_local::connect(&pipe_name).await {
                    Ok(client) => {
                        eprintln!(
                            "[subject:{pid}] connected to bootstrap pipe {}",
                            control_sock_path.display()
                        );
                        break client;
                    }
                    Err(e) => {
                        if tokio::time::Instant::now() >= deadline {
                            return Err(format!(
                                "connect bootstrap pipe {} failed after {:?}: {e}",
                                control_sock_path.display(),
                                connect_started.elapsed()
                            ));
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
                            _ = heartbeat.tick() => {
                                if let Some(status) = child
                                    .try_wait()
                                    .map_err(|e| format!("try_wait on subject process: {e}"))?
                                {
                                    return Err(format!(
                                        "subject exited while waiting for bootstrap pipe: {status}"
                                    ));
                                }
                                eprintln!(
                                    "[subject:{pid}] waiting for bootstrap pipe {} (elapsed={:?}, latest_error={e})",
                                    control_sock_path.display(),
                                    connect_started.elapsed()
                                );
                            }
                        }
                    }
                }
            }
        };

        let request = encode_request(sid.as_bytes()).map_err(|e| format!("encode request: {e}"))?;

        // Send the bootstrap request and receive the response.
        #[cfg(unix)]
        let link: ShmLink = {
            stream
                .write_all(&request)
                .map_err(|e| format!("send bootstrap request: {e}"))?;
            eprintln!("[subject:{pid}] sent bootstrap request");

            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .map_err(|e| format!("set bootstrap socket read timeout: {e}"))?;

            let recv_fd = stream.as_raw_fd();
            let received = tokio::task::spawn_blocking(move || {
                shm_primitives::bootstrap::recv_response_unix(recv_fd)
            })
            .await
            .map_err(|e| format!("bootstrap recv task join: {e}"))?
            .map_err(|e| format!("recv bootstrap response: {e}"))?;
            eprintln!("[subject:{pid}] received bootstrap response");
            if received.response.status != BootstrapStatus::Success {
                return Err(format!(
                    "bootstrap failed: status={:?}, payload={}",
                    received.response.status,
                    String::from_utf8_lossy(&received.response.payload)
                ));
            }

            let fds = received
                .fds
                .ok_or_else(|| "missing bootstrap success fds".to_string())?;
            let hub_path = std::str::from_utf8(&received.response.payload)
                .map_err(|e| format!("bootstrap payload is not utf-8 path: {e}"))?;
            let segment = Arc::new(
                Segment::attach(std::path::Path::new(hub_path))
                    .map_err(|e| format!("attach segment at {hub_path}: {e}"))?,
            );
            let peer_id = shm_primitives::PeerId::new(received.response.peer_id as u8)
                .ok_or_else(|| format!("invalid peer id {}", received.response.peer_id))?;

            let doorbell_fd = fds.doorbell_fd.into_raw_fd();
            let mmap_rx_owned = fds.mmap_control_fd;
            let mmap_tx_owned = mmap_rx_owned
                .try_clone()
                .map_err(|e| format!("clone mmap control fd: {e}"))?;
            let mmap_rx_fd = mmap_rx_owned.into_raw_fd();
            let mmap_tx_fd = mmap_tx_owned.into_raw_fd();

            unsafe {
                guest_link_from_raw(segment, peer_id, doorbell_fd, mmap_rx_fd, mmap_tx_fd, true)
            }
            .map_err(|e| format!("guest_link_from_raw: {e}"))?
        };

        #[cfg(windows)]
        let link: ShmLink = {
            use roam_shm::bootstrap::{
                BootstrapSuccessNames, BOOTSTRAP_RESPONSE_HEADER_LEN, decode_response,
            };
            use tokio::io::AsyncWriteExt;

            stream
                .write_all(&request)
                .await
                .map_err(|e| format!("send bootstrap request: {e}"))?;
            eprintln!("[subject:{pid}] sent bootstrap request");

            // Read bootstrap response header.
            let mut header = [0u8; BOOTSTRAP_RESPONSE_HEADER_LEN];
            stream
                .read_exact(&mut header)
                .await
                .map_err(|e| format!("read bootstrap response header: {e}"))?;

            // Parse payload length from header (bytes 9-10 = payload_len as u16 LE).
            let payload_len = u16::from_le_bytes([header[9], header[10]]) as usize;
            let mut payload = vec![0u8; payload_len];
            if payload_len > 0 {
                stream
                    .read_exact(&mut payload)
                    .await
                    .map_err(|e| format!("read bootstrap response payload: {e}"))?;
            }
            eprintln!("[subject:{pid}] received bootstrap response");

            // Combine into full frame for decode_response.
            let mut frame = Vec::with_capacity(BOOTSTRAP_RESPONSE_HEADER_LEN + payload_len);
            frame.extend_from_slice(&header);
            frame.extend_from_slice(&payload);
            let response_ref =
                decode_response(&frame).map_err(|e| format!("decode bootstrap response: {e}"))?;

            if response_ref.status != BootstrapStatus::Success {
                return Err(format!(
                    "bootstrap failed: status={:?}, payload={}",
                    response_ref.status,
                    String::from_utf8_lossy(response_ref.payload)
                ));
            }

            let names = BootstrapSuccessNames::decode(response_ref.payload)
                .map_err(|e| format!("decode bootstrap names: {e}"))?;
            let segment = Arc::new(
                Segment::attach(std::path::Path::new(&names.segment_path))
                    .map_err(|e| format!("attach segment at {}: {e}", names.segment_path))?,
            );
            let peer_id = shm_primitives::PeerId::new(response_ref.peer_id as u8)
                .ok_or_else(|| format!("invalid peer id {}", response_ref.peer_id))?;

            // On Windows there are no inherited fds. The subject told us
            // the mmap_tx pipe in the env; read it from the harness env.
            // For SHM-host mode the *subject* is the host that sent us pipe names,
            // and we read the mmap_tx_pipe from the env.
            let mmap_tx_pipe = std::env::var("SHM_MMAP_TX_PIPE")
                .unwrap_or_default();



            guest_link_from_names(
                segment,
                peer_id,
                &names.doorbell_name,
                &names.mmap_ctrl_name,
                &mmap_tx_pipe,
                true,
            )
            .map_err(|e| format!("guest_link_from_names: {e}"))?
        };

        let (client, _sh) = initiator(link)
            .establish::<TestbedClient>(NoopHandler)
            .await
            .map_err(|e| format!("handshake: {e}"))?;

        Ok::<_, String>(client)
    }
    .await;

    match setup_result {
        Ok(client) => {
            keep_tempdir_alive(dir);
            Ok((client, child))
        }
        Err(e) => {
            let status_note = match child.try_wait() {
                Ok(Some(status)) => format!("subject exited: {status}"),
                Ok(None) => "subject still running".to_string(),
                Err(wait_err) => format!("subject status unavailable: {wait_err}"),
            };
            child.kill().await.ok();
            Err(format!("{e}; {status_note}"))
        }
    }
}
