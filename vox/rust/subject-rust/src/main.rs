//! Rust subject binary for the vox compliance suite.

use spec_proto::{
    Canvas, Color, Config, LookupError, MathError, Measurement, Message, Person, Point, Profile,
    Record, Rectangle, Shape, Status, Tag, TaggedPoint, Testbed, TestbedClient, TestbedDispatcher,
};
use tracing::{debug, error, info, instrument};
use vox::{Rx, Tx};
use vox_core::{TransportMode, initiator, initiator_on};
use vox_shm::bootstrap::{BootstrapStatus, encode_request};
use vox_shm::segment::Segment;
use vox_stream::tcp_link_source;

#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::time::Duration;
#[cfg(windows)]
use vox_shm::guest_link_from_names;
#[cfg(unix)]
use vox_shm::guest_link_from_raw;

#[derive(Clone)]
struct TestbedService;

async fn stream_retry_probe_values(count: u32, output: Tx<i32>) {
    for i in 0..count as i32 {
        debug!(i, "sending value");
        if let Err(e) = output.send(i).await {
            error!(i, ?e, "send failed");
            break;
        }
    }
    output.close(Default::default()).await.ok();
}

impl Testbed for TestbedService {
    #[instrument(skip(self))]
    async fn echo(&self, message: String) -> String {
        info!("echo called");
        message
    }

    #[instrument(skip(self))]
    async fn reverse(&self, message: String) -> String {
        info!("reverse called");
        message.chars().rev().collect()
    }

    #[instrument(skip(self))]
    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError> {
        info!("divide called");
        if divisor == 0 {
            Err(MathError::DivisionByZero)
        } else {
            dividend.checked_div(divisor).ok_or(MathError::Overflow)
        }
    }

    #[instrument(skip(self))]
    async fn lookup(&self, id: u32) -> Result<Person, LookupError> {
        info!("lookup called");
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

    #[instrument(skip(self, numbers))]
    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        info!("sum called");
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            debug!(n = *n, total, "received number");
            total += *n as i64;
        }
        info!(total, "sum complete");
        total
    }

    #[instrument(skip(self, output))]
    async fn generate(&self, count: u32, output: Tx<i32>) {
        info!(count, "generate called");
        stream_retry_probe_values(count, output).await;
    }

    #[instrument(skip(self, output))]
    async fn generate_retry_non_idem(&self, count: u32, output: Tx<i32>) {
        info!(count, "generate_retry_non_idem called");
        stream_retry_probe_values(count, output).await;
    }

    #[instrument(skip(self, output))]
    async fn generate_retry_idem(&self, count: u32, output: Tx<i32>) {
        info!(count, "generate_retry_idem called");
        stream_retry_probe_values(count, output).await;
    }

    #[instrument(skip(self, input, output))]
    async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
        info!("transform called");
        while let Ok(Some(s)) = input.recv().await {
            debug!(s = ?*s, "transforming");
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
}

fn requested_transport_mode() -> TransportMode {
    match std::env::var("SPEC_CONDUIT").ok().as_deref() {
        Some("stable") => TransportMode::Stable,
        _ => TransportMode::Bare,
    }
}

fn main() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    let mode = std::env::var("SUBJECT_MODE").unwrap_or_else(|_| "server".to_string());
    match mode.as_str() {
        "server" => rt.block_on(connect_and_serve()),
        "client" => rt.block_on(run_client()),
        "server-listen" => rt.block_on(listen_and_serve()),
        "shm-server" => rt.block_on(connect_and_serve_shm()),
        other => Err(format!("unknown SUBJECT_MODE: {other}")),
    }
}

/// Bind a TCP listener, announce the address to stdout (for the harness to read),
/// accept one connection, and serve the Testbed service on it.
///
/// Used by cross-language harness tests where another subject acts as the client.
async fn listen_and_serve() -> Result<(), String> {
    use tokio::net::TcpListener;
    use vox_core::acceptor_on;

    let listen_port: u16 = std::env::var("LISTEN_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let listener = TcpListener::bind(("127.0.0.1", listen_port))
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    // Signal readiness — the harness reads this line from stdout.
    println!("LISTEN_ADDR=127.0.0.1:{}", addr.port());
    info!("server-listen mode: bound to {addr}");

    let (stream, _) = listener
        .accept()
        .await
        .map_err(|e| format!("accept: {e}"))?;
    stream.set_nodelay(true).ok();

    let _client = acceptor_on(vox_stream::StreamLink::tcp(stream))
        .on_connection(TestbedDispatcher::new(TestbedService).establish::<TestbedClient>())
        .await
        .map_err(|e| format!("handshake: {e}"))?;

    std::future::pending::<()>().await;
    Ok(())
}

async fn connect_and_serve() -> Result<(), String> {
    let addr = std::env::var("PEER_ADDR").map_err(|_| "PEER_ADDR env var not set".to_string())?;
    info!("connecting to {addr}");

    let root_caller_guard = initiator(tcp_link_source(addr), requested_transport_mode())
        .on_connection(TestbedDispatcher::new(TestbedService).establish::<TestbedClient>())
        .await
        .map_err(|e| format!("handshake failed: {e}"))?;

    let _root_caller_guard = root_caller_guard;
    std::future::pending::<()>().await;
    Ok(())
}

async fn run_client() -> Result<(), String> {
    const ITEM_COUNT: u32 = 40;

    let addr = std::env::var("PEER_ADDR").map_err(|_| "PEER_ADDR env var not set".to_string())?;
    let scenario = std::env::var("CLIENT_SCENARIO").unwrap_or_else(|_| "echo".to_string());
    info!("client mode: connecting to {addr}, scenario={scenario}");

    let client = initiator(tcp_link_source(addr), requested_transport_mode())
        .on_connection(TestbedDispatcher::new(TestbedService).establish::<TestbedClient>())
        .await
        .map_err(|e| format!("handshake failed: {e}"))?;

    match scenario.as_str() {
        "echo" => {
            let result = client
                .echo("hello from client".to_string())
                .await
                .map_err(|e| format!("echo failed: {e:?}"))?;
            info!("echo result: {result}");
        }
        "reverse" => {
            let result = client
                .reverse("hello".to_string())
                .await
                .map_err(|e| format!("reverse failed: {e:?}"))?;
            if result != "olleh" {
                return Err(format!("reverse: expected 'olleh', got {result:?}"));
            }
            info!("reverse result: {result}");
        }
        "divide_success" => {
            let result = client
                .divide(10, 3)
                .await
                .map_err(|e| format!("divide_success failed: {e:?}"))?;
            if result != 3 {
                return Err(format!("divide_success: expected 3, got {result}"));
            }
            info!("divide_success result: {result}");
        }
        "divide_zero" => {
            match client.divide(10, 0).await {
                Err(vox::VoxError::User(MathError::DivisionByZero)) => {}
                other => {
                    return Err(format!(
                        "divide_zero: expected DivisionByZero, got {other:?}"
                    ));
                }
            }
            info!("divide_zero: got expected DivisionByZero error");
        }
        "divide_overflow" => {
            match client.divide(i64::MIN, -1).await {
                Err(vox::VoxError::User(MathError::Overflow)) => {}
                other => return Err(format!("divide_overflow: expected Overflow, got {other:?}")),
            }
            info!("divide_overflow: got expected Overflow error");
        }
        "lookup_found" => {
            let p = client
                .lookup(1)
                .await
                .map_err(|e| format!("lookup_found failed: {e:?}"))?;
            if p.name != "Alice" {
                return Err(format!("lookup_found: expected Alice, got {p:?}"));
            }
            info!("lookup_found: {p:?}");
        }
        "lookup_found_no_email" => {
            let p = client
                .lookup(2)
                .await
                .map_err(|e| format!("lookup_found_no_email failed: {e:?}"))?;
            if p.name != "Bob" || p.email.is_some() {
                return Err(format!(
                    "lookup_found_no_email: expected Bob with no email, got {p:?}"
                ));
            }
            info!("lookup_found_no_email: {p:?}");
        }
        "lookup_not_found" => {
            match client.lookup(999).await {
                Err(vox::VoxError::User(spec_proto::LookupError::NotFound)) => {}
                other => {
                    return Err(format!(
                        "lookup_not_found: expected NotFound, got {other:?}"
                    ));
                }
            }
            info!("lookup_not_found: got expected NotFound error");
        }
        "lookup_access_denied" => {
            match client.lookup(100).await {
                Err(vox::VoxError::User(spec_proto::LookupError::AccessDenied)) => {}
                other => {
                    return Err(format!(
                        "lookup_access_denied: expected AccessDenied, got {other:?}"
                    ));
                }
            }
            info!("lookup_access_denied: got expected AccessDenied error");
        }
        "echo_point" => {
            let pt = spec_proto::Point { x: 42, y: -7 };
            let result = client
                .echo_point(pt.clone())
                .await
                .map_err(|e| format!("echo_point failed: {e:?}"))?;
            if result != pt {
                return Err(format!("echo_point: expected {pt:?}, got {result:?}"));
            }
            info!("echo_point OK");
        }
        "create_person" => {
            let p = client
                .create_person("Dave".to_string(), 40, Some("dave@example.com".to_string()))
                .await
                .map_err(|e| format!("create_person failed: {e:?}"))?;
            if p.name != "Dave" || p.age != 40 || p.email.as_deref() != Some("dave@example.com") {
                return Err(format!("create_person: unexpected {p:?}"));
            }
            info!("create_person OK: {p:?}");
        }
        "rectangle_area" => {
            use spec_proto::{Point, Rectangle};
            let area = client
                .rectangle_area(Rectangle {
                    top_left: Point { x: 0, y: 10 },
                    bottom_right: Point { x: 5, y: 0 },
                    label: None,
                })
                .await
                .map_err(|e| format!("rectangle_area failed: {e:?}"))?;
            if (area - 50.0_f64).abs() > 1e-9 {
                return Err(format!("rectangle_area: expected 50.0, got {area}"));
            }
            info!("rectangle_area: {area}");
        }
        "parse_color" => {
            for (name, expected) in [
                ("red", Color::Red),
                ("green", Color::Green),
                ("blue", Color::Blue),
            ] {
                match client.parse_color(name.to_string()).await {
                    Ok(Some(c)) if c == expected => {}
                    other => return Err(format!("parse_color {name}: unexpected {other:?}")),
                }
            }
            match client.parse_color("purple".to_string()).await {
                Ok(None) => {}
                other => return Err(format!("parse_color purple: expected None, got {other:?}")),
            }
            info!("parse_color: all variants OK");
        }
        "get_points" => {
            let pts = client
                .get_points(5)
                .await
                .map_err(|e| format!("get_points failed: {e:?}"))?;
            if pts.len() != 5 {
                return Err(format!("get_points: expected 5, got {}", pts.len()));
            }
            info!("get_points: {} points", pts.len());
        }
        "swap_pair" => {
            let (s, n) = client
                .swap_pair((99, "hello".to_string()))
                .await
                .map_err(|e| format!("swap_pair failed: {e:?}"))?;
            if s != "hello" || n != 99 {
                return Err(format!(
                    "swap_pair: expected ('hello', 99), got ({s:?}, {n})"
                ));
            }
            info!("swap_pair OK");
        }
        "echo_bytes" => {
            let data = vec![1u8, 2, 3, 255, 0, 128];
            let result = client
                .echo_bytes(data.clone())
                .await
                .map_err(|e| format!("echo_bytes failed: {e:?}"))?;
            if result != data {
                return Err(format!("echo_bytes: expected {data:?}, got {result:?}"));
            }
            info!("echo_bytes OK");
        }
        "echo_bool" => {
            for b in [true, false] {
                let result = client
                    .echo_bool(b)
                    .await
                    .map_err(|e| format!("echo_bool({b}) failed: {e:?}"))?;
                if result != b {
                    return Err(format!("echo_bool: expected {b}, got {result}"));
                }
            }
            info!("echo_bool OK");
        }
        "echo_u64" => {
            for n in [0u64, 1, u64::MAX, 1_000_000_000_000] {
                let result = client
                    .echo_u64(n)
                    .await
                    .map_err(|e| format!("echo_u64({n}) failed: {e:?}"))?;
                if result != n {
                    return Err(format!("echo_u64: expected {n}, got {result}"));
                }
            }
            info!("echo_u64 OK");
        }
        "echo_option_string" => {
            match client.echo_option_string(Some("hello".to_string())).await {
                Ok(Some(s)) if s == "hello" => {}
                other => return Err(format!("echo_option_string Some: got {other:?}")),
            }
            match client.echo_option_string(None).await {
                Ok(None) => {}
                other => return Err(format!("echo_option_string None: got {other:?}")),
            }
            info!("echo_option_string OK");
        }
        "describe_point" => {
            let result = client
                .describe_point("origin".to_string(), 0, 0, true)
                .await
                .map_err(|e| format!("describe_point failed: {e:?}"))?;
            if result.label != "origin" || result.x != 0 || result.y != 0 || !result.active {
                return Err(format!("describe_point: unexpected {result:?}"));
            }
            info!("describe_point OK: {result:?}");
        }
        "all_colors" => {
            let colors = client
                .all_colors()
                .await
                .map_err(|e| format!("all_colors failed: {e:?}"))?;
            if colors.len() != 3 {
                return Err(format!("all_colors: expected 3, got {}", colors.len()));
            }
            if colors[0] != Color::Red || colors[1] != Color::Green || colors[2] != Color::Blue {
                return Err(format!("all_colors: unexpected order {colors:?}"));
            }
            info!("all_colors OK");
        }
        "echo_shape" => {
            for shape in [
                spec_proto::Shape::Point,
                #[allow(clippy::approx_constant)]
                spec_proto::Shape::Circle { radius: 3.14 },
                spec_proto::Shape::Rectangle {
                    width: 2.0,
                    height: 5.0,
                },
            ] {
                let result = client
                    .echo_shape(shape.clone())
                    .await
                    .map_err(|e| format!("echo_shape failed: {e:?}"))?;
                if result != shape {
                    return Err(format!("echo_shape: expected {shape:?}, got {result:?}"));
                }
            }
            info!("echo_shape OK (all 3 variants)");
        }
        "pipelining" => {
            let mut handles = Vec::new();
            for i in 0..10usize {
                let client = client.clone();
                let msg = format!("msg{i}");
                handles.push(tokio::spawn(async move {
                    client
                        .echo(msg.clone())
                        .await
                        .map_err(|e| format!("pipelining echo {i}: {e:?}"))
                        .and_then(|r| {
                            if r == msg {
                                Ok(r)
                            } else {
                                Err(format!("pipelining: expected {msg}, got {r}"))
                            }
                        })
                }));
            }
            for h in handles {
                h.await.map_err(|e| format!("pipelining join: {e}"))??;
            }
            info!("pipelining OK (10 concurrent echo calls)");
        }
        "sum_large" => {
            let (tx, rx) = vox::channel::<i32>();
            let n: i32 = 100;
            let send_task = tokio::spawn(async move {
                for i in 0..n {
                    tx.send(i).await.ok();
                }
                tx.close(Default::default()).await.ok();
            });
            let result = client
                .sum_large(rx)
                .await
                .map_err(|e| format!("sum_large failed: {e:?}"))?;
            send_task.await.ok();
            let expected: i64 = (0..n as i64).sum();
            if result != expected {
                return Err(format!("sum_large: expected {expected}, got {result}"));
            }
            info!("sum_large OK: {result}");
        }
        "generate_large" => {
            let (tx, mut rx) = vox::channel::<i32>();
            let n: u32 = 100;
            let recv_task = tokio::spawn(async move {
                let mut received = Vec::new();
                while let Ok(Some(v)) = rx.recv().await {
                    received.push(*v);
                }
                received
            });
            client
                .generate_large(n, tx)
                .await
                .map_err(|e| format!("generate_large failed: {e:?}"))?;
            let received = recv_task.await.map_err(|e| format!("recv task: {e}"))?;
            if received.len() != n as usize {
                return Err(format!(
                    "generate_large: expected {n} items, got {}",
                    received.len()
                ));
            }
            let expected: Vec<i32> = (0..n as i32).collect();
            if received != expected {
                return Err(format!(
                    "generate_large: expected sequential, got {received:?}"
                ));
            }
            info!("generate_large OK: {} items", received.len());
        }
        "sum_client_to_server" => {
            let (tx, rx) = vox::channel::<i32>();
            let send_task = tokio::spawn(async move {
                for n in [1i32, 2, 3, 4, 5] {
                    tx.send(n).await.unwrap();
                }
                tx.close(Default::default()).await.unwrap();
            });
            let result = client
                .sum(rx)
                .await
                .map_err(|e| format!("sum_client_to_server failed: {e:?}"))?;
            send_task.await.ok();
            if result != 15 {
                return Err(format!("sum_client_to_server: expected 15, got {result}"));
            }
            info!("sum_client_to_server OK: {result}");
        }
        "transform_bidi" => {
            let (input_tx, input_rx) = vox::channel::<String>();
            let (output_tx, mut output_rx) = vox::channel::<String>();
            let messages = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
            let msgs_clone = messages.clone();
            let send_task = tokio::spawn(async move {
                for msg in msgs_clone {
                    input_tx.send(msg).await.unwrap();
                }
                input_tx.close(Default::default()).await.unwrap();
            });
            let recv_task = tokio::spawn(async move {
                let mut received = Vec::new();
                while let Ok(Some(s)) = output_rx.recv().await {
                    received.push(s.clone());
                }
                received
            });
            client
                .transform(input_rx, output_tx)
                .await
                .map_err(|e| format!("transform_bidi failed: {e:?}"))?;
            send_task.await.ok();
            let received = recv_task.await.map_err(|e| format!("recv: {e}"))?;
            if received != messages {
                return Err(format!(
                    "transform_bidi: expected {messages:?}, got {received:?}"
                ));
            }
            info!("transform_bidi OK");
        }
        "shape_area" => {
            use spec_proto::Shape;
            let area = client
                .shape_area(Shape::Rectangle {
                    width: 3.0,
                    height: 4.0,
                })
                .await
                .map_err(|e| format!("shape_area failed: {e:?}"))?;
            if (area - 12.0_f64).abs() > 1e-9 {
                return Err(format!("shape_area: expected 12.0, got {area}"));
            }
            info!("shape_area result: {area}");
        }
        "create_canvas" => {
            use spec_proto::{Color, Shape};
            let canvas = client
                .create_canvas(
                    "enum-canvas".to_string(),
                    vec![Shape::Point, Shape::Circle { radius: 2.5 }],
                    Color::Green,
                )
                .await
                .map_err(|e| format!("create_canvas failed: {e:?}"))?;
            if canvas.name != "enum-canvas" {
                return Err(format!(
                    "create_canvas: expected name 'enum-canvas', got {:?}",
                    canvas.name
                ));
            }
            if canvas.background != Color::Green {
                return Err(format!(
                    "create_canvas: expected Green background, got {:?}",
                    canvas.background
                ));
            }
            if canvas.shapes.len() != 2 {
                return Err(format!(
                    "create_canvas: expected 2 shapes, got {}",
                    canvas.shapes.len()
                ));
            }
            info!("create_canvas result OK");
        }
        "process_message" => {
            use spec_proto::Message;
            let result = client
                .process_message(Message::Data(vec![1, 2, 3, 4]))
                .await
                .map_err(|e| format!("process_message failed: {e:?}"))?;
            match &result {
                Message::Data(bytes) if bytes == &vec![4, 3, 2, 1] => {}
                other => {
                    return Err(format!("process_message: unexpected result {other:?}"));
                }
            }
            info!("process_message result OK");
        }
        "channel_retry_non_idem" => {
            let (tx, mut rx) = vox::channel::<i32>();
            let call = tokio::spawn({
                let client = client.clone();
                async move {
                    info!("starting channel_retry_non_idem call");
                    client.generate_retry_non_idem(ITEM_COUNT, tx).await
                }
            });
            let recv = tokio::spawn(async move {
                let mut received = Vec::new();
                loop {
                    match rx.recv().await {
                        Ok(Some(n)) => {
                            info!(value = *n, "channel_retry_idem recv");
                            received.push(*n);
                        }
                        Ok(None) => {
                            info!("channel_retry_idem recv reached close");
                            break;
                        }
                        Err(err) => {
                            info!("channel_retry_idem recv error: {err}");
                            break;
                        }
                    }
                }
                received
            });

            let result = tokio::time::timeout(Duration::from_secs(5), call)
                .await
                .map_err(|_| "timed out waiting for non-idem call".to_string())?
                .map_err(|e| format!("non-idem call task: {e}"))?;
            let received = tokio::time::timeout(Duration::from_secs(5), recv)
                .await
                .map_err(|_| "timed out draining non-idem channel".to_string())?
                .map_err(|e| format!("non-idem recv task: {e}"))?;

            if !matches!(result, Err(vox::VoxError::Indeterminate)) {
                return Err(format!(
                    "expected non-idem channel retry to fail with Indeterminate, got {result:?}"
                ));
            }

            let expected: Vec<i32> = (0..received.len() as i32).collect();
            if received != expected {
                return Err(format!(
                    "expected sequential non-idem prefix {expected:?}, got {received:?}"
                ));
            }
        }
        "channel_retry_idem" => {
            let (tx, mut rx) = vox::channel::<i32>();
            let call = tokio::spawn({
                let client = client.clone();
                async move {
                    info!("starting channel_retry_idem call");
                    client.generate_retry_idem(ITEM_COUNT, tx).await
                }
            });
            let recv = tokio::spawn(async move {
                let mut received = Vec::new();
                while let Ok(Some(n)) = rx.recv().await {
                    received.push(*n);
                }
                received
            });

            tokio::time::timeout(Duration::from_secs(5), call)
                .await
                .map_err(|_| "timed out waiting for idem call".to_string())?
                .map_err(|e| format!("idem call task: {e}"))?
                .map_err(|e| format!("idem retry call failed: {e:?}"))?;

            let received = tokio::time::timeout(Duration::from_secs(5), recv)
                .await
                .map_err(|_| "timed out draining idem channel".to_string())?
                .map_err(|e| format!("idem recv task: {e}"))?;

            let restart = received
                .iter()
                .enumerate()
                .skip(1)
                .find_map(|(idx, value)| (*value == 0).then_some(idx))
                .ok_or_else(|| format!("expected retry stream restart, got {received:?}"))?;

            let expected_prefix: Vec<i32> = (0..restart as i32).collect();
            if received[..restart] != expected_prefix {
                return Err(format!(
                    "expected first attempt prefix {expected_prefix:?}, got {:?}",
                    &received[..restart]
                ));
            }

            let expected_rerun: Vec<i32> = (0..ITEM_COUNT as i32).collect();
            if received[restart..] != expected_rerun {
                return Err(format!(
                    "expected rerun suffix {expected_rerun:?}, got {:?}",
                    &received[restart..]
                ));
            }
        }
        other => return Err(format!("unknown CLIENT_SCENARIO: {other}")),
    }

    Ok(())
}

async fn connect_and_serve_shm() -> Result<(), String> {
    let control_sock = std::env::var("SHM_CONTROL_SOCK")
        .map_err(|_| "SHM_CONTROL_SOCK env var not set".to_string())?;

    let request = encode_request();

    // Connect to the control socket, send the bootstrap request, and receive the
    // bootstrap response. The response carries fds on Unix and names on Windows.
    #[cfg(unix)]
    let link = {
        let mut stream = std::os::unix::net::UnixStream::connect(&control_sock)
            .map_err(|e| format!("connect bootstrap socket: {e}"))?;
        std::io::Write::write_all(&mut stream, &request)
            .map_err(|e| format!("send bootstrap request: {e}"))?;

        let received = shm_primitives::bootstrap::recv_response_unix(stream.as_raw_fd())
            .map_err(|e| format!("recv bootstrap response: {e}"))?;
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
        let segment = std::sync::Arc::new(
            Segment::attach(std::path::Path::new(hub_path))
                .map_err(|e| format!("attach segment at {hub_path}: {e}"))?,
        );
        let peer_id = shm_primitives::PeerId::new(received.response.peer_id as u8)
            .ok_or_else(|| format!("invalid peer id {}", received.response.peer_id))?;

        use std::os::fd::IntoRawFd;
        let doorbell_fd = fds.doorbell_fd.into_raw_fd();
        let mmap_rx_fd = fds.mmap_rx_fd.into_raw_fd();
        let mmap_tx_fd = fds.mmap_tx_fd.into_raw_fd();

        unsafe { guest_link_from_raw(segment, peer_id, doorbell_fd, mmap_rx_fd, mmap_tx_fd, true) }
            .map_err(|e| format!("guest_link_from_raw: {e}"))?
    };

    #[cfg(windows)]
    let link = {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use vox_shm::bootstrap::{
            BOOTSTRAP_RESPONSE_HEADER_LEN, BootstrapSuccessNames, decode_response,
        };

        let mmap_tx_pipe = std::env::var("SHM_MMAP_TX_PIPE")
            .map_err(|_| "SHM_MMAP_TX_PIPE env var not set".to_string())?;

        // On Windows, SHM_CONTROL_SOCK is a named pipe path.
        let mut stream = vox_local::connect(&control_sock)
            .await
            .map_err(|e| format!("connect bootstrap pipe: {e}"))?;
        stream
            .write_all(&request)
            .await
            .map_err(|e| format!("send bootstrap request: {e}"))?;

        // Read the bootstrap response header.
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
        let segment = std::sync::Arc::new(
            Segment::attach(std::path::Path::new(&names.segment_path))
                .map_err(|e| format!("attach segment at {}: {e}", names.segment_path))?,
        );
        let peer_id = shm_primitives::PeerId::new(response_ref.peer_id as u8)
            .ok_or_else(|| format!("invalid peer id {}", response_ref.peer_id))?;

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

    let root_caller_guard = initiator_on(link, requested_transport_mode())
        .on_connection(TestbedDispatcher::new(TestbedService).establish::<TestbedClient>())
        .await
        .map_err(|e| format!("handshake failed: {e}"))?;

    let _root_caller_guard = root_caller_guard;
    // Session and driver are spawned internally by establish(); wait forever
    // so the spawned tasks can continue serving requests.
    std::future::pending::<()>().await;
    Ok(())
}
