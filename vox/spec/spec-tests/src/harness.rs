use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::SystemTime;

use spec_proto::{
    Canvas, Color, Config, GnarlyPayload, LookupError, MathError, Measurement, Message, Person,
    Point, Profile, Record, Rectangle, Shape, Status, Tag, TaggedPoint, Testbed, TestbedClient,
    TestbedDispatcher,
};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use vox::{Rx, Tx};
use vox_core::{
    DriverReplySink, SessionAcceptOutcome, SessionHandle, SessionRegistry, acceptor_conduit,
    acceptor_on, acceptor_transport, memory_link_pair,
};
use vox_stream::StreamLink;
use vox_types::{Backing, Link, LinkRx, LinkTx, RequestCall, SelfRef};
use vox_websocket::WsLink;

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
    Ws,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubjectSpec {
    pub language: SubjectLanguage,
    pub transport: SubjectTestTransport,
}

impl SubjectSpec {
    pub const fn tcp(language: SubjectLanguage) -> Self {
        Self {
            language,
            transport: SubjectTestTransport::Tcp,
        }
    }

    pub const fn ws(language: SubjectLanguage) -> Self {
        Self {
            language,
            transport: SubjectTestTransport::Ws,
        }
    }
}

#[derive(Clone)]
struct NoopHandler;

impl vox_types::Handler<DriverReplySink> for NoopHandler {
    async fn handle(
        &self,
        _call: SelfRef<RequestCall<'static>>,
        _reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
    }
}

struct BreakableLink {
    tx: moire::sync::mpsc::Sender<Option<Vec<u8>>>,
    rx: moire::sync::mpsc::Receiver<Option<Vec<u8>>>,
}

#[derive(Clone)]
struct BreakHandle {
    tx: moire::sync::mpsc::Sender<Option<Vec<u8>>>,
}

fn breakable_link_pair(buffer: usize) -> (BreakableLink, BreakHandle, BreakableLink, BreakHandle) {
    let (tx_a, rx_b) = moire::sync::mpsc::channel("breakable_link.a→b", buffer);
    let (tx_b, rx_a) = moire::sync::mpsc::channel("breakable_link.b→a", buffer);

    let a_handle = BreakHandle { tx: tx_b.clone() };
    let b_handle = BreakHandle { tx: tx_a.clone() };

    (
        BreakableLink { tx: tx_a, rx: rx_a },
        a_handle,
        BreakableLink { tx: tx_b, rx: rx_b },
        b_handle,
    )
}

impl BreakHandle {
    async fn close(&self) {
        let _ = self.tx.send(None).await;
    }
}

impl Link for BreakableLink {
    type Tx = BreakableLinkTx;
    type Rx = BreakableLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        (
            BreakableLinkTx { tx: self.tx },
            BreakableLinkRx { rx: self.rx },
        )
    }
}

#[derive(Clone)]
struct BreakableLinkTx {
    tx: moire::sync::mpsc::Sender<Option<Vec<u8>>>,
}

impl LinkTx for BreakableLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> std::io::Result<()> {
        self.tx.clone().send(Some(bytes)).await.map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "receiver dropped")
        })
    }

    async fn close(self) -> std::io::Result<()> {
        drop(self.tx);
        Ok(())
    }
}

struct BreakableLinkRx {
    rx: moire::sync::mpsc::Receiver<Option<Vec<u8>>>,
}

#[derive(Debug)]
struct BreakableLinkRxError;

impl std::fmt::Display for BreakableLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "breakable link rx error")
    }
}

impl std::error::Error for BreakableLinkRxError {}

impl LinkRx for BreakableLinkRx {
    type Error = BreakableLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        match self.rx.recv().await {
            Some(Some(bytes)) => Ok(Some(Backing::Boxed(bytes.into_boxed_slice()))),
            Some(None) | None => Ok(None),
        }
    }
}

async fn forward_link_frames<Rx, Tx>(rx: &mut Rx, tx: &Tx) -> Result<(), String>
where
    Rx: LinkRx,
    Rx::Error: std::fmt::Display,
    Tx: LinkTx,
{
    loop {
        let Some(frame) = rx.recv().await.map_err(|e| format!("recv frame: {e}"))? else {
            return Ok(());
        };
        tx.send(frame.as_bytes().to_vec())
            .await
            .map_err(|e| format!("send frame: {e}"))?;
    }
}

async fn bridge_links<A, B>(left: A, right: B) -> Result<(), String>
where
    A: Link + Send + 'static,
    B: Link + Send + 'static,
    A::Tx: Send + 'static,
    A::Rx: Send + 'static,
    B::Tx: Send + 'static,
    B::Rx: Send + 'static,
    <A::Rx as LinkRx>::Error: std::fmt::Display,
    <B::Rx as LinkRx>::Error: std::fmt::Display,
{
    let (left_tx, mut left_rx) = left.split();
    let (right_tx, mut right_rx) = right.split();

    let left_to_right = async {
        let result = forward_link_frames(&mut left_rx, &right_tx).await;
        let _ = right_tx.close().await;
        result
    };
    let right_to_left = async {
        let result = forward_link_frames(&mut right_rx, &left_tx).await;
        let _ = left_tx.close().await;
        result
    };

    let (a, b) = tokio::join!(left_to_right, right_to_left);
    match (a, b) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(err), _) | (_, Err(err)) => Err(err),
    }
}

#[derive(Clone, Default)]
struct CurrentBreakPair {
    inner: Arc<Mutex<Option<(BreakHandle, BreakHandle)>>>,
}

impl CurrentBreakPair {
    fn set(&self, left: BreakHandle, right: BreakHandle) {
        *self.inner.lock().expect("break pair mutex") = Some((left, right));
    }

    async fn break_current(&self) {
        let pair = self.inner.lock().expect("break pair mutex").clone();
        if let Some((left, right)) = pair {
            left.close().await;
            right.close().await;
        }
    }
}

pub struct ResumableSubjectHarness {
    pub client: TestbedClient,
    pub child: Child,
    active_breaks: CurrentBreakPair,
    accept_task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub struct SubjectConnectionBreaker {
    active_breaks: CurrentBreakPair,
}

impl SubjectConnectionBreaker {
    pub async fn break_current(&self) {
        self.active_breaks.break_current().await;
    }
}

impl ResumableSubjectHarness {
    pub async fn break_current(&self) {
        self.active_breaks.break_current().await;
    }

    pub fn breaker(&self) -> SubjectConnectionBreaker {
        SubjectConnectionBreaker {
            active_breaks: self.active_breaks.clone(),
        }
    }

    pub async fn cleanup(mut self) {
        self.accept_task.abort();
        let _ = self.child.kill().await;
    }
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
        "ws" => SubjectTestTransport::Ws,
        _ => SubjectTestTransport::Tcp,
    }
}

pub fn run_async<T>(f: impl Future<Output = T>) -> T {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(f)
}

struct RetryProbeState {
    active_breaks: CurrentBreakPair,
    break_after: usize,
    sent: AtomicUsize,
    broke: AtomicBool,
}

#[derive(Clone, Default)]
struct TestbedService {
    retry_probe: Option<Arc<RetryProbeState>>,
}

impl TestbedService {
    fn new() -> Self {
        Self::default()
    }

    fn with_retry_probe(active_breaks: CurrentBreakPair, break_after: usize) -> Self {
        Self {
            retry_probe: Some(Arc::new(RetryProbeState {
                active_breaks,
                break_after,
                sent: AtomicUsize::new(0),
                broke: AtomicBool::new(false),
            })),
        }
    }

    async fn stream_retry_probe_values(&self, count: u32, output: Tx<i32>) {
        for i in 0..count as i32 {
            if output.send(i).await.is_err() {
                eprintln!("[harness] stream_retry_probe_values send failed at {i}");
                break;
            }
            if let Some(state) = &self.retry_probe {
                let sent = state.sent.fetch_add(1, Ordering::SeqCst) + 1;
                if sent >= state.break_after && !state.broke.swap(true, Ordering::SeqCst) {
                    eprintln!("[harness] breaking active tcp attachment after {sent} items");
                    state.active_breaks.break_current().await;
                }
            }
        }
        eprintln!("[harness] stream_retry_probe_values closing output");
        output.close(Default::default()).await.ok();
    }
}

async fn stream_retry_probe_values(count: u32, output: Tx<i32>) {
    for i in 0..count as i32 {
        if output.send(i).await.is_err() {
            break;
        }
    }
    output.close(Default::default()).await.ok();
}

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
            dividend.checked_div(divisor).ok_or(MathError::Overflow)
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
            100..=199 => Err(LookupError::AccessDenied),
            _ => Err(LookupError::NotFound),
        }
    }

    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            let n = n.get();
            total += *n as i64;
        }
        total
    }

    async fn generate(&self, count: u32, output: Tx<i32>) {
        stream_retry_probe_values(count, output).await;
    }

    async fn generate_retry_non_idem(&self, count: u32, output: Tx<i32>) {
        self.stream_retry_probe_values(count, output).await;
    }

    async fn generate_retry_idem(&self, count: u32, output: Tx<i32>) {
        self.stream_retry_probe_values(count, output).await;
    }

    async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
        while let Ok(Some(s)) = input.recv().await {
            let s = s.get();
            let _ = output.send(s.clone()).await;
        }
        output.close(Default::default()).await.ok();
    }

    async fn post_reply_generate(&self, output: Tx<i32>) {
        spawn_loud(async move {
            moire::time::sleep(Duration::from_millis(10)).await;
            for i in 0..5 {
                if output.send(i).await.is_err() {
                    break;
                }
            }
            output.close(Default::default()).await.ok();
        });
    }

    async fn post_reply_sum(&self, mut input: Rx<i32>, result: Tx<i64>) {
        spawn_loud(async move {
            let mut total: i64 = 0;
            while let Ok(Some(n)) = input.recv().await {
                let n = n.get();
                total += *n as i64;
            }
            let _ = result.send(total).await;
            result.close(Default::default()).await.ok();
        });
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

    async fn echo_bytes(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }

    async fn echo_bool(&self, b: bool) -> bool {
        b
    }

    async fn echo_u64(&self, n: u64) -> u64 {
        n
    }

    async fn echo_option_string(&self, s: Option<String>) -> Option<String> {
        s
    }

    async fn sum_large(&self, mut numbers: Rx<i32>) -> i64 {
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            let n = n.get();
            total += *n as i64;
        }
        total
    }

    async fn generate_large(&self, count: u32, output: Tx<i32>) {
        stream_retry_probe_values(count, output).await;
    }

    async fn all_colors(&self) -> Vec<Color> {
        vec![Color::Red, Color::Green, Color::Blue]
    }

    async fn describe_point(&self, label: String, x: i32, y: i32, active: bool) -> TaggedPoint {
        TaggedPoint {
            label,
            x,
            y,
            active,
        }
    }

    async fn echo_shape(&self, shape: Shape) -> Shape {
        shape
    }

    async fn echo_status_v1(&self, status: Status) -> Status {
        status
    }

    async fn echo_tag_v1(&self, tag: Tag) -> Tag {
        tag
    }

    async fn echo_profile(&self, profile: Profile) -> Profile {
        profile
    }

    async fn echo_record(&self, record: Record) -> Record {
        record
    }

    async fn echo_status(&self, status: Status) -> Status {
        status
    }

    async fn echo_tag(&self, tag: Tag) -> Tag {
        tag
    }

    async fn echo_measurement(&self, m: Measurement) -> Measurement {
        m
    }

    async fn echo_config(&self, c: Config) -> Config {
        c
    }

    async fn echo_gnarly(&self, payload: GnarlyPayload) -> GnarlyPayload {
        payload
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
        .env("VOX_DLOG", "1")
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

    // If it crashes immediately (non-zero exit), surface that early.
    // A fast successful exit (code 0) is fine - the test just completed quickly.
    tokio::time::sleep(Duration::from_millis(10)).await;
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())?
        && !status.success()
    {
        eprintln!("[subject:{pid}] crashed immediately: {status}");
        return Err(format!("subject crashed immediately with {status}"));
    }

    Ok(child)
}

struct ResumableSubjectClientGuard {
    path: std::path::PathBuf,
}

impl Drop for ResumableSubjectClientGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.path);
    }
}

async fn acquire_resumable_subject_client_guard() -> Result<ResumableSubjectClientGuard, String> {
    let path = std::env::temp_dir().join("vox-spec-tests-resumable-subject-client.lock");
    loop {
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(ResumableSubjectClientGuard { path }),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if let Ok(metadata) = std::fs::metadata(&path)
                    && let Ok(modified) = metadata.modified()
                    && let Ok(age) = SystemTime::now().duration_since(modified)
                    && age > Duration::from_secs(10)
                {
                    let _ = std::fs::remove_dir_all(&path);
                    continue;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(err) => return Err(format!("create resumable subject lock: {err}")),
        }
    }
}

/// Listen on a random TCP port, upgrade incoming connection to WebSocket,
/// complete the vox handshake, and return a ready `TestbedClient`.
pub async fn accept_subject_ws(cmd: &str) -> Result<(TestbedClient, Child, SessionHandle), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?
        .port();
    let ws_url = format!("ws://127.0.0.1:{port}/");

    let child = spawn_subject_cmd_with_env(cmd, &ws_url, &[]).await?;

    // Use a timeout to catch subjects that fail to connect.
    let (tcp_stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .map_err(|_| "timed out waiting for WebSocket subject to connect".to_string())?
        .map_err(|e| format!("accept: {e}"))?;
    tcp_stream.set_nodelay(true).ok();

    let ws = WsLink::server(tcp_stream)
        .await
        .map_err(|e| format!("WebSocket upgrade: {e}"))?;

    let client = acceptor_on(ws)
        .on_connection(TestbedDispatcher::new(TestbedService::new()))
        .establish::<TestbedClient>()
        .await
        .map_err(|e| format!("handshake: {e}"))?;
    let sh = client.session.clone().unwrap();

    Ok((client, child, sh))
}

pub async fn accept_subject() -> Result<(TestbedClient, Child, SessionHandle), String> {
    let spec = SubjectSpec {
        language: SubjectLanguage::Rust,
        transport: subject_transport(),
    };
    accept_subject_spec(spec).await
}

pub async fn accept_subject_spec(
    spec: SubjectSpec,
) -> Result<(TestbedClient, Child, SessionHandle), String> {
    let cmd = subject_cmd_for_language(spec.language);
    match spec.transport {
        SubjectTestTransport::Tcp => accept_subject_tcp(&cmd).await,
        SubjectTestTransport::Ws => accept_subject_ws(&cmd).await,
    }
}

/// Accept a subject over TCP given a custom command string.
pub async fn accept_subject_cmd_tcp(
    cmd: &str,
) -> Result<(TestbedClient, Child, SessionHandle), String> {
    accept_subject_tcp(cmd).await
}

/// Spawn a subject, establish a connection, run a test closure, and clean up.
///
/// Monitors the child process in a background task — if the subject dies,
/// the session handle is dropped so pending calls fail immediately instead
/// of hanging until a timeout.
pub async fn with_subject<F, T>(spec: SubjectSpec, f: F) -> Result<T, String>
where
    F: AsyncFnOnce(&TestbedClient) -> Result<T, String>,
{
    let cmd = subject_cmd_for_language(spec.language);
    with_subject_cmd(spec, &cmd, f).await
}

/// Like [`with_subject`] but with a custom command string (e.g. for evolved TS subjects).
pub async fn with_subject_cmd<F, T>(spec: SubjectSpec, cmd: &str, f: F) -> Result<T, String>
where
    F: AsyncFnOnce(&TestbedClient) -> Result<T, String>,
{
    let (client, mut child, session_handle) = match spec.transport {
        SubjectTestTransport::Tcp => accept_subject_tcp(cmd).await?,
        SubjectTestTransport::Ws => accept_subject_ws(cmd).await?,
    };

    // Monitor the child process — if it dies, drop the session handle
    // so pending RPCs fail immediately.
    let child_pid = child.id().unwrap_or_default();
    let (child_died_tx, child_died_rx) = tokio::sync::oneshot::channel::<String>();
    let monitor = tokio::task::spawn(async move {
        let status = child.wait().await;
        let msg = match status {
            Ok(s) => format!("subject (pid={child_pid}) exited: {s}"),
            Err(e) => format!("subject (pid={child_pid}) wait error: {e}"),
        };
        eprintln!("[harness] {msg}");
        // Drop the session handle to close the session, which unblocks
        // any pending RPCs on the client.
        drop(session_handle);
        let _ = child_died_tx.send(msg);
    });

    let result = tokio::select! {
        result = f(&client) => result,
        Ok(msg) = child_died_rx => {
            Err(format!("subject died during test: {msg}"))
        }
    };

    // Clean up: abort the monitor and kill the child if still alive.
    monitor.abort();

    result
}

pub async fn accept_subject_with_transport(
    transport: SubjectTestTransport,
) -> Result<(TestbedClient, Child, SessionHandle), String> {
    accept_subject_spec(SubjectSpec {
        language: SubjectLanguage::Rust,
        transport,
    })
    .await
}

pub async fn accept_subject_spec_resumable(
    spec: SubjectSpec,
) -> Result<ResumableSubjectHarness, String> {
    let cmd = subject_cmd_for_language(spec.language);
    match spec.transport {
        SubjectTestTransport::Tcp => accept_subject_tcp_resumable(&cmd).await,
        SubjectTestTransport::Ws => {
            Err("resumable subject harness is only supported for TCP transport".to_string())
        }
    }
}

/// Spawn the subject in client mode, connect to a simple (non-resumable) Rust
/// server, let the subject run the named scenario, and verify it exits 0.
///
/// This is the non-resumable counterpart of `run_subject_client_scenario_resumable`.
/// Use it for scenarios that don't require session recovery.
/// Spawn a subject in `server-listen` mode, wait for it to announce its
/// bound address on stdout (`LISTEN_ADDR=127.0.0.1:PORT`), then return
/// the address string and the child process handle.
///
/// Spawns the process directly (without the normal log pump) so we can
/// read the `LISTEN_ADDR=` line from stdout before handing it off.
/// After reading the address, stderr is pumped to the test output as usual.
pub async fn spawn_server_subject(spec: SubjectSpec) -> Result<(String, Child), String> {
    if spec.transport != SubjectTestTransport::Tcp {
        return Err("server-listen mode is only supported for TCP transport".to_string());
    }

    let cmd = subject_cmd_for_language(spec.language);
    eprintln!(
        "[subject:spawn] cmd={cmd:?} peer_addr=<server-listen> extra_env=SUBJECT_MODE=server-listen LISTEN_PORT=0"
    );

    let mut command = if cmd.ends_with(".sh") {
        let mut c = Command::new("sh");
        c.arg("-lc").arg(cmd);
        c
    } else {
        Command::new(cmd)
    };
    command
        .current_dir(workspace_root())
        .env("PEER_ADDR", "unused")
        .env("SUBJECT_MODE", "server-listen")
        .env("LISTEN_PORT", "0")
        .env("VOX_DLOG", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped()) // we read this ourselves
        .stderr(Stdio::piped()); // pumped after addr is read

    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to spawn server subject: {e}"))?;
    let pid = child.id().unwrap_or_default();
    eprintln!("[subject:{pid}] spawned (server-listen)");

    // Read stdout until we see LISTEN_ADDR=.  We must do this before
    // handing stdout to the log pump, because the pump would consume it.
    let mut stdout = child.stdout.take().ok_or("no stdout from server subject")?;
    let addr = tokio::time::timeout(Duration::from_secs(10), async {
        use tokio::io::AsyncBufReadExt;
        let mut reader = tokio::io::BufReader::new(&mut stdout);
        let mut line = String::new();
        loop {
            line.clear();
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| format!("reading server subject stdout: {e}"))?;
            let trimmed = line.trim();
            if let Some(addr) = trimmed.strip_prefix("LISTEN_ADDR=") {
                return Ok::<String, String>(addr.to_string());
            }
            if line.is_empty() {
                return Err("server subject closed stdout without announcing address".to_string());
            }
            // Forward any other stdout lines as log output.
            eprintln!("[subject:{pid}:stdout] {trimmed}");
        }
    })
    .await
    .map_err(|_| "timed out waiting for server subject to announce listen address".to_string())??;

    // Hand the rest of stdout and all of stderr to the log pump.
    spawn_subject_log_pump(stdout, pid, "stdout");
    if let Some(stderr) = child.stderr.take() {
        spawn_subject_log_pump(stderr, pid, "stderr");
    }

    eprintln!("[subject:{pid}] server-listen ready at {addr}");
    Ok((addr, child))
}

/// Run a cross-language scenario: spawn `server_spec` in server-listen mode,
/// then spawn `client_spec` as a client pointing at the server.
/// The harness orchestrates but is not in the data path — all traffic flows
/// directly between the two subjects.
pub fn run_cross_language_scenario(
    server_spec: SubjectSpec,
    client_spec: SubjectSpec,
    scenario: &str,
) {
    let scenario = scenario.to_string();
    let result: Result<(), String> = run_async(async move {
        if server_spec.transport != SubjectTestTransport::Tcp
            || client_spec.transport != SubjectTestTransport::Tcp
        {
            // Only TCP cross-language supported for now.
            return Ok(());
        }

        let (server_addr, mut server_child) = spawn_server_subject(server_spec).await?;

        let client_cmd = subject_cmd_for_language(client_spec.language);
        let mut client_child = spawn_subject_cmd_with_env(
            &client_cmd,
            &server_addr,
            &[("SUBJECT_MODE", "client"), ("CLIENT_SCENARIO", &scenario)],
        )
        .await?;

        let status = tokio::time::timeout(Duration::from_secs(15), client_child.wait())
            .await
            .map_err(|_| format!("cross-language scenario `{scenario}` timed out"))?
            .map_err(|e| format!("wait on client subject: {e}"))?;

        server_child.kill().await.ok();

        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "cross-language scenario `{scenario}` failed with status {status}"
            ))
        }
    });
    result.unwrap();
}

pub fn run_subject_client_scenario(spec: SubjectSpec, scenario: &str) {
    let scenario = scenario.to_string();
    let result: Result<(), String> = run_async(async move {
        match spec.transport {
            SubjectTestTransport::Tcp => {
                run_subject_client_scenario_tcp(spec.language, &scenario).await
            }
            SubjectTestTransport::Ws => {
                run_subject_client_scenario_ws(spec.language, &scenario).await
            }
        }
    });
    result.unwrap();
}

async fn run_subject_client_scenario_tcp(
    language: SubjectLanguage,
    scenario: &str,
) -> Result<(), String> {
    let cmd = subject_cmd_for_language(language);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    let mut child = spawn_subject_cmd_with_env(
        &cmd,
        &addr.to_string(),
        &[("SUBJECT_MODE", "client"), ("CLIENT_SCENARIO", scenario)],
    )
    .await?;

    let accept_task = tokio::spawn(async move {
        let (stream, _) = match listener.accept().await {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[harness] client-scenario accept error: {e}");
                return;
            }
        };
        stream.set_nodelay(true).ok();
        match acceptor_on(StreamLink::tcp(stream))
            .on_connection(TestbedDispatcher::new(TestbedService::new()))
            .establish::<TestbedClient>()
            .await
        {
            Ok(_client) => {
                std::future::pending::<()>().await;
            }
            Err(e) => {
                eprintln!("[harness] client-scenario handshake error: {e}");
            }
        }
    });

    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .map_err(|_| format!("subject client scenario `{scenario}` timed out"))?
        .map_err(|e| format!("wait on subject process: {e}"))?;

    accept_task.abort();

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "subject client scenario `{scenario}` failed with status {status}"
        ))
    }
}

async fn run_subject_client_scenario_ws(
    language: SubjectLanguage,
    scenario: &str,
) -> Result<(), String> {
    let cmd = subject_cmd_for_language(language);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?
        .port();
    let ws_url = format!("ws://127.0.0.1:{port}/");

    let mut child = spawn_subject_cmd_with_env(
        &cmd,
        &ws_url,
        &[("SUBJECT_MODE", "client"), ("CLIENT_SCENARIO", scenario)],
    )
    .await?;

    let accept_task = tokio::spawn(async move {
        let (tcp_stream, _) = match listener.accept().await {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[harness] ws client-scenario accept error: {e}");
                return;
            }
        };
        tcp_stream.set_nodelay(true).ok();
        let ws = match WsLink::server(tcp_stream).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("[harness] ws upgrade error: {e}");
                return;
            }
        };
        match acceptor_on(ws)
            .on_connection(TestbedDispatcher::new(TestbedService::new()))
            .establish::<TestbedClient>()
            .await
        {
            Ok(_client) => {
                std::future::pending::<()>().await;
            }
            Err(e) => {
                eprintln!("[harness] ws client-scenario handshake error: {e}");
            }
        }
    });

    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .map_err(|_| format!("subject client scenario (ws) `{scenario}` timed out"))?
        .map_err(|e| format!("wait on subject process: {e}"))?;

    accept_task.abort();

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "subject client scenario (ws) `{scenario}` failed with status {status}"
        ))
    }
}

pub async fn run_subject_client_scenario_resumable(
    spec: SubjectSpec,
    scenario: &str,
    break_after: usize,
) -> Result<(), String> {
    let _guard = acquire_resumable_subject_client_guard().await?;

    if spec.transport != SubjectTestTransport::Tcp {
        return Err("resumable subject client scenarios are only supported for TCP".to_string());
    }

    let cmd = subject_cmd_for_language(spec.language);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    let mut child = spawn_subject_cmd_with_env(
        &cmd,
        &addr.to_string(),
        &[("SUBJECT_MODE", "client"), ("CLIENT_SCENARIO", scenario)],
    )
    .await?;

    let registry = SessionRegistry::default();
    let active_breaks = CurrentBreakPair::default();
    let (first_accept_tx, first_accept_rx) = oneshot::channel::<Result<(), String>>();
    let service = TestbedService::with_retry_probe(active_breaks.clone(), break_after);

    let accept_task = tokio::spawn(async move {
        let mut first_accept_tx = Some(first_accept_tx);
        let mut retained_clients = Vec::new();
        let mut retained_handles = Vec::new();
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(err) => {
                    if let Some(tx) = first_accept_tx.take() {
                        let _ = tx.send(Err(format!("accept: {err}")));
                    }
                    break;
                }
            };
            eprintln!("[harness] accepted subject client tcp connection");
            stream.set_nodelay(true).ok();

            let (bridge_link, bridge_break, session_link, session_break) = breakable_link_pair(64);
            active_breaks.set(bridge_break, session_break);

            tokio::spawn(async move {
                let _ = bridge_links(StreamLink::tcp(stream), bridge_link).await;
            });

            match acceptor_on(session_link)
                .session_registry(registry.clone())
                .metadata(vec![vox_types::MetadataEntry::str(
                    "vox-service",
                    "Testbed",
                )])
                .on_connection(TestbedDispatcher::new(service.clone()))
                .establish_or_resume::<TestbedClient>()
                .await
            {
                Ok(SessionAcceptOutcome::Established(client)) => {
                    eprintln!("[harness] established subject client session");
                    if let Some(sh) = client.session.clone() {
                        retained_handles.push(sh);
                    }
                    retained_clients.push(client);
                    if let Some(tx) = first_accept_tx.take() {
                        let _ = tx.send(Ok(()));
                    }
                }
                Ok(SessionAcceptOutcome::Resumed) => {
                    eprintln!("[harness] resumed subject client session");
                }
                Err(err) => {
                    eprintln!("[harness] subject client handshake error: {err}");
                    if let Some(tx) = first_accept_tx.take() {
                        let _ = tx.send(Err(format!("handshake: {err}")));
                    }
                    break;
                }
            }
        }
    });

    first_accept_rx
        .await
        .map_err(|e| format!("accept task join: {e}"))??;

    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .map_err(|_| format!("subject client scenario `{scenario}` timed out"))?
        .map_err(|e| format!("wait on subject process: {e}"))?;

    accept_task.abort();

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "subject client scenario `{scenario}` failed with status {status}"
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustTransport {
    Mem,
    Tcp,
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
    }
}

async fn accept_rust_inproc_with_conduits<L>(
    client_link: L,
    server_link: L,
) -> Result<TestbedClient, String>
where
    L: vox_types::Link + Send + 'static,
    L::Tx: Send + 'static,
    L::Rx: Send + 'static,
    <L::Rx as vox_types::LinkRx>::Error: std::error::Error + Send + Sync + 'static,
{
    let (server_ready_tx, server_ready_rx) = oneshot::channel::<Result<(), String>>();
    let _server_task = tokio::spawn(async move {
        let (tx, mut rx) = vox_types::Link::split(server_link);
        let handshake_result = vox_core::handshake_as_acceptor(
            &tx,
            &mut rx,
            vox_types::ConnectionSettings {
                parity: vox_types::Parity::Even,
                max_concurrent_requests: 64,
            },
            true,
            false,
            None,
            vec![vox_types::MetadataEntry::str("vox-service", "Noop")],
        )
        .await
        .map_err(|e| format!("server CBOR handshake: {e}"));
        let handshake_result = match handshake_result {
            Ok(r) => r,
            Err(err) => {
                let _ = server_ready_tx.send(Err(err));
                return;
            }
        };
        let server_conduit =
            vox_core::BareConduit::<vox_types::MessageFamily, _>::new(vox_types::SplitLink {
                tx,
                rx,
            });
        let setup = acceptor_conduit(server_conduit, handshake_result)
            .on_connection(TestbedDispatcher::new(TestbedService::new()))
            .establish::<TestbedClient>()
            .await
            .map_err(|e| format!("server handshake: {e}"));
        let server_caller_guard = match setup {
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

    let (client_tx, mut client_rx) = vox_types::Link::split(client_link);
    let client_handshake = vox_core::handshake_as_initiator(
        &client_tx,
        &mut client_rx,
        vox_types::ConnectionSettings {
            parity: vox_types::Parity::Odd,
            max_concurrent_requests: 64,
        },
        true,
        None,
        vec![vox_types::MetadataEntry::str("vox-service", "Noop")],
    )
    .await
    .map_err(|e| format!("client CBOR handshake: {e}"))?;
    let client_conduit =
        vox_core::BareConduit::<vox_types::MessageFamily, _>::new(vox_types::SplitLink {
            tx: client_tx,
            rx: client_rx,
        });
    let client = vox_core::initiator_conduit(client_conduit, client_handshake)
        .on_connection(NoopHandler)
        .establish::<TestbedClient>()
        .await
        .map_err(|e| format!("client handshake: {e}"))?;

    server_ready_rx
        .await
        .map_err(|e| format!("server task join: {e}"))??;

    Ok(client)
}

async fn accept_subject_tcp(cmd: &str) -> Result<(TestbedClient, Child, SessionHandle), String> {
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

    let client = acceptor_transport(StreamLink::tcp(stream))
        .on_connection(NoopHandler)
        .establish::<TestbedClient>()
        .await
        .map_err(|e| format!("handshake: {e}"))?;
    let sh = client.session.clone().unwrap();

    Ok((client, child, sh))
}

async fn accept_subject_tcp_resumable(cmd: &str) -> Result<ResumableSubjectHarness, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    let child = spawn_subject_cmd_with_env(cmd, &addr.to_string(), &[]).await?;
    let registry = SessionRegistry::default();
    let active_breaks = CurrentBreakPair::default();
    let (first_client_tx, first_client_rx) = oneshot::channel::<Result<TestbedClient, String>>();

    let accept_task = {
        let active_breaks = active_breaks.clone();
        tokio::spawn(async move {
            let mut first_client_tx = Some(first_client_tx);
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(accepted) => accepted,
                    Err(err) => {
                        if let Some(tx) = first_client_tx.take() {
                            let _ = tx.send(Err(format!("accept: {err}")));
                        }
                        break;
                    }
                };
                stream.set_nodelay(true).ok();

                let (bridge_link, bridge_break, session_link, session_break) =
                    breakable_link_pair(64);
                active_breaks.set(bridge_break, session_break);

                tokio::spawn(async move {
                    let _ = bridge_links(StreamLink::tcp(stream), bridge_link).await;
                });

                match acceptor_on(session_link)
                    .session_registry(registry.clone())
                    .metadata(vec![vox_types::MetadataEntry::str(
                        "vox-service",
                        "Testbed",
                    )])
                    .on_connection(NoopHandler)
                    .establish_or_resume::<TestbedClient>()
                    .await
                {
                    Ok(SessionAcceptOutcome::Established(client)) => {
                        if let Some(tx) = first_client_tx.take() {
                            let _ = tx.send(Ok(client));
                        }
                    }
                    Ok(SessionAcceptOutcome::Resumed) => {}
                    Err(err) => {
                        if let Some(tx) = first_client_tx.take() {
                            let _ = tx.send(Err(format!("handshake: {err}")));
                            break;
                        }
                    }
                }
            }
        })
    };

    let client = first_client_rx
        .await
        .map_err(|e| format!("accept task join: {e}"))??;

    Ok(ResumableSubjectHarness {
        client,
        child,
        active_breaks,
        accept_task,
    })
}
