//! Browser tests for vox Rust/Wasm client.
//!
//! This crate only compiles for wasm32 target. Build with:
//! ```
//! wasm-pack build --target web rust/wasm-browser-tests
//! ```

#![cfg(target_arch = "wasm32")]

use spec_proto::{Color, LookupError, MathError, Message, Point, Rectangle, Shape, TestbedClient};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn error(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format!($($t)*)))
}

macro_rules! console_error {
    ($($t:tt)*) => (error(&format!($($t)*)))
}

#[wasm_bindgen]
pub struct TestResults {
    results: Vec<TestResult>,
}

struct TestResult {
    name: String,
    passed: bool,
    error: Option<String>,
}

#[wasm_bindgen]
impl TestResults {
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.results.len()
    }

    pub fn get_name(&self, index: usize) -> Option<String> {
        self.results.get(index).map(|r| r.name.clone())
    }

    pub fn get_passed(&self, index: usize) -> bool {
        self.results.get(index).is_some_and(|r| r.passed)
    }

    pub fn get_error(&self, index: usize) -> Option<String> {
        self.results
            .get(index)
            .and_then(|r| r.error.as_ref().cloned())
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }
}

/// Run all tests against a WebSocket server at the given URL.
#[wasm_bindgen]
pub async fn run_tests(ws_url: &str) -> TestResults {
    let mut results = Vec::new();

    console_log!("Connecting to {ws_url} via vox::connect_lane...");

    // First-party high-level connect: parses the `ws://`/`wss://` scheme, opens
    // a browser WebSocket, performs the vox handshake, and opens a typed lane.
    let client: TestbedClient = match vox::connect_lane(ws_url).await {
        Ok(client) => client,
        Err(e) => {
            console_error!("connect_lane failed: {e:?}");
            results.push(TestResult {
                name: "connect".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
            return TestResults { results };
        }
    };

    console_log!("Connected and lane opened.");

    // Run echo tests
    run_echo_tests(&client, &mut results).await;

    // Run complex type tests
    run_complex_tests(&client, &mut results).await;

    // Run fallible tests
    run_fallible_tests(&client, &mut results).await;

    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    console_log!("Tests complete: {passed}/{total} passed");

    TestResults { results }
}

async fn run_echo_tests(client: &TestbedClient, results: &mut Vec<TestResult>) {
    // Test: echo
    console_log!("Testing echo...");
    match client.echo("Hello from Rust Wasm!".into()).await {
        Ok(result) if result == "Hello from Rust Wasm!" => {
            results.push(TestResult {
                name: "echo".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "echo".into(),
                passed: false,
                error: Some(format!(
                    "expected 'Hello from Rust Wasm!', got '{}'",
                    result
                )),
            });
        }
        Err(e) => {
            console_error!("echo failed: {e:?}");
            results.push(TestResult {
                name: "echo".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: reverse
    console_log!("Testing reverse...");
    match client.reverse("Hello".into()).await {
        Ok(result) if result == "olleH" => {
            results.push(TestResult {
                name: "reverse".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "reverse".into(),
                passed: false,
                error: Some(format!("expected 'olleH', got '{}'", result)),
            });
        }
        Err(e) => {
            console_error!("reverse failed: {e:?}");
            results.push(TestResult {
                name: "reverse".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }
}

async fn run_complex_tests(client: &TestbedClient, results: &mut Vec<TestResult>) {
    // Test: echo_point
    console_log!("Testing echo_point...");
    let point = Point { x: 42, y: -17 };
    match client.echo_point(point.clone()).await {
        Ok(result) if result == point => {
            results.push(TestResult {
                name: "echo_point".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "echo_point".into(),
                passed: false,
                error: Some(format!("expected {point:?}, got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "echo_point".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: create_person
    console_log!("Testing create_person...");
    match client
        .create_person("Alice".into(), 30, Some("alice@example.com".into()))
        .await
    {
        Ok(result)
            if result.name == "Alice"
                && result.age == 30
                && result.email.as_deref() == Some("alice@example.com") =>
        {
            results.push(TestResult {
                name: "create_person".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "create_person".into(),
                passed: false,
                error: Some(format!("unexpected person: {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "create_person".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: rectangle_area
    console_log!("Testing rectangle_area...");
    let rect = Rectangle {
        top_left: Point { x: 0, y: 0 },
        bottom_right: Point { x: 10, y: 5 },
        label: None,
    };
    match client.rectangle_area(rect).await {
        Ok(result) if (result - 50.0).abs() < 0.001 => {
            results.push(TestResult {
                name: "rectangle_area".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "rectangle_area".into(),
                passed: false,
                error: Some(format!("expected 50.0, got {}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "rectangle_area".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: parse_color
    console_log!("Testing parse_color...");
    match client.parse_color("red".into()).await {
        Ok(Some(Color::Red)) => {
            results.push(TestResult {
                name: "parse_color".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "parse_color".into(),
                passed: false,
                error: Some(format!("expected Some(Red), got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "parse_color".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: shape_area (Circle)
    console_log!("Testing shape_area (Circle)...");
    match client.shape_area(Shape::Circle { radius: 2.0 }).await {
        Ok(result) if (result - std::f64::consts::PI * 4.0).abs() < 0.001 => {
            results.push(TestResult {
                name: "shape_area_circle".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "shape_area_circle".into(),
                passed: false,
                error: Some(format!(
                    "expected {}, got {}",
                    std::f64::consts::PI * 4.0,
                    result
                )),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "shape_area_circle".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: shape_area (Rectangle)
    console_log!("Testing shape_area (Rectangle)...");
    match client
        .shape_area(Shape::Rectangle {
            width: 3.0,
            height: 4.0,
        })
        .await
    {
        Ok(result) if (result - 12.0).abs() < 0.001 => {
            results.push(TestResult {
                name: "shape_area_rectangle".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "shape_area_rectangle".into(),
                passed: false,
                error: Some(format!("expected 12.0, got {}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "shape_area_rectangle".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: get_points
    console_log!("Testing get_points...");
    match client.get_points(3).await {
        Ok(result)
            if result.len() == 3
                && result[0] == Point { x: 0, y: 0 }
                && result[1] == Point { x: 1, y: 2 }
                && result[2] == Point { x: 2, y: 4 } =>
        {
            results.push(TestResult {
                name: "get_points".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "get_points".into(),
                passed: false,
                error: Some(format!("unexpected points: {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "get_points".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: swap_pair
    console_log!("Testing swap_pair...");
    match client.swap_pair((42, "hello".into())).await {
        Ok(result) if result.0 == "hello" && result.1 == 42 => {
            results.push(TestResult {
                name: "swap_pair".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "swap_pair".into(),
                passed: false,
                error: Some(format!("expected (\"hello\", 42), got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "swap_pair".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: process_message (Text)
    console_log!("Testing process_message (Text)...");
    match client.process_message(Message::Text("hello".into())).await {
        Ok(result) if matches!(&result, Message::Text(s) if s == "processed: hello") => {
            results.push(TestResult {
                name: "process_message_text".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "process_message_text".into(),
                passed: false,
                error: Some(format!(
                    "expected Text(\"processed: hello\"), got {:?}",
                    result
                )),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "process_message_text".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: process_message (Number)
    console_log!("Testing process_message (Number)...");
    match client.process_message(Message::Number(21)).await {
        Ok(result) if matches!(&result, Message::Number(n) if *n == 42) => {
            results.push(TestResult {
                name: "process_message_number".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "process_message_number".into(),
                passed: false,
                error: Some(format!("expected Number(42), got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "process_message_number".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }
}

async fn run_fallible_tests(client: &TestbedClient, results: &mut Vec<TestResult>) {
    // Test: divide (success)
    console_log!("Testing divide (success)...");
    match client.divide(10, 2).await {
        Ok(5) => {
            results.push(TestResult {
                name: "divide_success".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "divide_success".into(),
                passed: false,
                error: Some(format!("expected 5, got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "divide_success".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: divide (error - division by zero)
    console_log!("Testing divide (error)...");
    match client.divide(10, 0).await {
        Err(vox_types::VoxError::User(error)) if *error == MathError::DivisionByZero => {
            results.push(TestResult {
                name: "divide_error".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "divide_error".into(),
                passed: false,
                error: Some(format!(
                    "expected DivisionByZero error, got Ok({:?})",
                    result
                )),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "divide_error".into(),
                passed: false,
                error: Some(format!("expected DivisionByZero, got {e:?}")),
            });
        }
    }

    // Test: lookup (success)
    console_log!("Testing lookup (success)...");
    match client.lookup(1).await {
        Ok(result)
            if result.name == "Alice"
                && result.age == 30
                && result.email.as_deref() == Some("alice@example.com") =>
        {
            results.push(TestResult {
                name: "lookup_success".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "lookup_success".into(),
                passed: false,
                error: Some(format!("unexpected person: {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "lookup_success".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: lookup (error - not found)
    console_log!("Testing lookup (error)...");
    match client.lookup(999).await {
        Err(vox_types::VoxError::User(error)) if *error == LookupError::NotFound => {
            results.push(TestResult {
                name: "lookup_error".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "lookup_error".into(),
                passed: false,
                error: Some(format!("expected NotFound error, got Ok({:?})", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "lookup_error".into(),
                passed: false,
                error: Some(format!("expected NotFound, got {e:?}")),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Stress / soak test
// ---------------------------------------------------------------------------
//
// Opens many concurrent WebSocket connections to the *same* server, and on
// each one hammers a rotating mix of request/response calls, error paths, and
// both stream directions until a deadline. Every call is wrapped in a
// per-call timeout so a wedged connection is reported as "stuck" rather than
// hanging the whole run.

use std::time::Duration;

use futures_util::future::{FutureExt, LocalBoxFuture, join, join_all};
use vox_rt::time::{Instant, timeout};

/// How long a single RPC may take before we consider the connection stuck.
const PER_CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// Number of distinct operations the workers rotate through.
const OP_COUNT: u64 = 10;

/// Aggregated result of a stress run, surfaced to JS.
#[wasm_bindgen]
pub struct StressSummary {
    connections: u32,
    connected: u32,
    total_requests: u64,
    total_errors: u64,
    stuck: u64,
    elapsed_ms: f64,
    first_error: Option<String>,
}

#[wasm_bindgen]
impl StressSummary {
    #[wasm_bindgen(getter)]
    pub fn connections(&self) -> u32 {
        self.connections
    }

    /// Number of workers that successfully established a connection + lane.
    #[wasm_bindgen(getter)]
    pub fn connected(&self) -> u32 {
        self.connected
    }

    #[wasm_bindgen(getter)]
    pub fn total_requests(&self) -> f64 {
        self.total_requests as f64
    }

    #[wasm_bindgen(getter)]
    pub fn total_errors(&self) -> f64 {
        self.total_errors as f64
    }

    /// Number of calls that exceeded [`PER_CALL_TIMEOUT`] (i.e. got stuck).
    #[wasm_bindgen(getter)]
    pub fn stuck(&self) -> f64 {
        self.stuck as f64
    }

    #[wasm_bindgen(getter)]
    pub fn elapsed_ms(&self) -> f64 {
        self.elapsed_ms
    }

    pub fn get_first_error(&self) -> Option<String> {
        self.first_error.clone()
    }

    /// Everything connected, and no errors or stuck calls occurred.
    pub fn all_ok(&self) -> bool {
        self.connected == self.connections
            && self.total_errors == 0
            && self.stuck == 0
            && self.total_requests > 0
    }
}

#[derive(Default)]
struct WorkerStats {
    connected: bool,
    requests: u64,
    errors: u64,
    stuck: u64,
    first_error: Option<String>,
}

impl WorkerStats {
    fn note_error(&mut self, msg: String) {
        self.errors += 1;
        if self.first_error.is_none() {
            self.first_error = Some(msg);
        }
    }
}

/// Run the soak test: `connections` concurrent clients against `ws_url`, each
/// looping for `duration_ms`.
#[wasm_bindgen]
pub async fn run_stress(ws_url: &str, connections: u32, duration_ms: f64) -> StressSummary {
    let duration = Duration::from_millis(duration_ms as u64);
    console_log!(
        "Stress: {connections} connections against {ws_url} for {duration_ms}ms via vox::connect_lane"
    );

    let started = Instant::now();
    let workers: Vec<LocalBoxFuture<'_, WorkerStats>> = (0..connections)
        .map(|id| stress_worker(ws_url.to_string(), id, duration).boxed_local())
        .collect();
    let stats = join_all(workers).await;
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

    let mut summary = StressSummary {
        connections,
        connected: 0,
        total_requests: 0,
        total_errors: 0,
        stuck: 0,
        elapsed_ms,
        first_error: None,
    };
    for st in stats {
        if st.connected {
            summary.connected += 1;
        }
        summary.total_requests += st.requests;
        summary.total_errors += st.errors;
        summary.stuck += st.stuck;
        if summary.first_error.is_none() {
            summary.first_error = st.first_error;
        }
    }

    console_log!(
        "Stress done: connected={}/{}, requests={}, errors={}, stuck={}, elapsed={:.0}ms",
        summary.connected,
        summary.connections,
        summary.total_requests,
        summary.total_errors,
        summary.stuck,
        summary.elapsed_ms
    );
    summary
}

async fn stress_worker(ws_url: String, id: u32, duration: Duration) -> WorkerStats {
    let mut stats = WorkerStats::default();

    let client: TestbedClient = match vox::connect_lane(&ws_url).await {
        Ok(client) => client,
        Err(e) => {
            stats.note_error(format!("worker {id} connect: {e:?}"));
            return stats;
        }
    };
    stats.connected = true;

    let deadline = Instant::now() + duration;
    let mut i: u64 = 0;
    while Instant::now() < deadline {
        match timeout(PER_CALL_TIMEOUT, run_one_op(&client, i)).await {
            Ok(Ok(())) => stats.requests += 1,
            Ok(Err(msg)) => stats.note_error(format!("worker {id} {msg}")),
            Err(_) => {
                stats.stuck += 1;
                if stats.first_error.is_none() {
                    stats.first_error = Some(format!(
                        "worker {id} op {} stuck > {PER_CALL_TIMEOUT:?}",
                        i % OP_COUNT
                    ));
                }
                // A stuck connection won't recover; stop hammering it.
                break;
            }
        }
        i += 1;
    }

    stats
}

/// Execute one operation from the rotating mix and validate its result.
async fn run_one_op(client: &TestbedClient, i: u64) -> Result<(), String> {
    match i % OP_COUNT {
        0 => match client.echo("stress".into()).await {
            Ok(s) if s == "stress" => Ok(()),
            Ok(s) => Err(format!("echo got {s:?}")),
            Err(e) => Err(format!("echo: {e:?}")),
        },
        1 => match client.reverse("abcdef".into()).await {
            Ok(s) if s == "fedcba" => Ok(()),
            Ok(s) => Err(format!("reverse got {s:?}")),
            Err(e) => Err(format!("reverse: {e:?}")),
        },
        2 => match client.divide(84, 2).await {
            Ok(42) => Ok(()),
            Ok(v) => Err(format!("divide got {v}")),
            Err(e) => Err(format!("divide: {e:?}")),
        },
        3 => match client.divide(1, 0).await {
            Err(vox_types::VoxError::User(e)) if matches!(*e, MathError::DivisionByZero) => Ok(()),
            other => Err(format!("divide-by-zero expected error, got {other:?}")),
        },
        4 => match client.lookup(1).await {
            Ok(p) if p.name == "Alice" => Ok(()),
            Ok(p) => Err(format!("lookup got {p:?}")),
            Err(e) => Err(format!("lookup: {e:?}")),
        },
        5 => match client.lookup(999).await {
            Err(vox_types::VoxError::User(e)) if matches!(*e, LookupError::NotFound) => Ok(()),
            other => Err(format!("lookup-missing expected NotFound, got {other:?}")),
        },
        6 => {
            let point = Point { x: 3, y: -4 };
            match client.echo_point(point.clone()).await {
                Ok(p) if p == point => Ok(()),
                Ok(p) => Err(format!("echo_point got {p:?}")),
                Err(e) => Err(format!("echo_point: {e:?}")),
            }
        }
        7 => match client.get_points(3).await {
            Ok(pts) if pts.len() == 3 && pts[2] == (Point { x: 2, y: 4 }) => Ok(()),
            Ok(pts) => Err(format!("get_points got {pts:?}")),
            Err(e) => Err(format!("get_points: {e:?}")),
        },
        8 => stress_sum(client).await,
        _ => stress_generate(client).await,
    }
}

/// Client-to-server stream: send 0..10 and expect the server to sum them (45).
async fn stress_sum(client: &TestbedClient) -> Result<(), String> {
    let (tx, rx) = vox::channel::<i32>();
    let produce = async move {
        for n in 0..10 {
            let _ = tx.send(n).await;
        }
        let _ = tx.close(Default::default()).await;
    };
    let (_, resp) = join(produce, client.sum(rx)).await;
    match resp {
        Ok(45) => Ok(()),
        Ok(v) => Err(format!("sum got {v}")),
        Err(e) => Err(format!("sum: {e:?}")),
    }
}

/// Server-to-client stream: ask for 8 values and expect 0..8.
async fn stress_generate(client: &TestbedClient) -> Result<(), String> {
    let (tx, mut rx) = vox::channel::<i32>();
    let drain = async move {
        let mut received = Vec::new();
        while let Ok(Some(n)) = rx.recv().await {
            received.push(*n.get());
        }
        received
    };
    let (call, received) = join(client.generate(8, tx), drain).await;
    call.map_err(|e| format!("generate: {e:?}"))?;
    let expected: Vec<i32> = (0..8).collect();
    if received == expected {
        Ok(())
    } else {
        Err(format!("generate got {received:?}"))
    }
}
