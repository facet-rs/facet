//! Rust subject binary for the roam compliance suite.
//!
//! This demonstrates the minimal code needed to implement a roam service
//! using the roam-stream transport library.

use roam::session::{Rx, Tx};
use roam_stream::{Connector, HandshakeConfig, LengthPrefixedFramed, connect, initiate_framed};
use tokio::net::TcpStream;
use tracing::{debug, error, info, instrument};

// Re-export types from spec_proto
pub use spec_proto::{
    Canvas, Color, LookupError, MathError, Message, Person, Point, Rectangle, Shape,
};

// Re-export generated service items from spec-proto as a `testbed` module
mod testbed {
    pub use spec_proto::{Testbed, TestbedClient, TestbedDispatcher};
}

// Service implementation using generated Testbed trait
#[derive(Clone)]
struct TestbedService;

impl testbed::Testbed for TestbedService {
    // ========================================================================
    // Unary methods
    // ========================================================================

    #[instrument(skip(self, _cx))]
    async fn echo(&self, _cx: &roam::session::Context, message: String) -> String {
        info!("echo called");
        message
    }

    #[instrument(skip(self, _cx))]
    async fn reverse(&self, _cx: &roam::session::Context, message: String) -> String {
        info!("reverse called");
        message.chars().rev().collect()
    }

    // ========================================================================
    // Fallible methods (for testing User(E) error path)
    // ========================================================================

    #[instrument(skip(self, _cx))]
    async fn divide(
        &self,
        _cx: &roam::session::Context,
        dividend: i64,
        divisor: i64,
    ) -> Result<i64, MathError> {
        info!("divide called");
        if divisor == 0 {
            Err(MathError::DivisionByZero)
        } else {
            Ok(dividend / divisor)
        }
    }

    #[instrument(skip(self, _cx))]
    async fn lookup(&self, _cx: &roam::session::Context, id: u32) -> Result<Person, LookupError> {
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
            _ => Err(LookupError::NotFound),
        }
    }

    // ========================================================================
    // Streaming methods
    // ========================================================================

    #[instrument(skip(self, _cx, numbers))]
    async fn sum(&self, _cx: &roam::session::Context, mut numbers: Rx<i32>) -> i64 {
        info!("sum called, starting to receive numbers");
        let mut total: i64 = 0;
        while let Some(n) = numbers.recv().await.ok().flatten() {
            debug!(n, total, "received number");
            total += n as i64;
        }
        info!(total, "sum complete");
        total
    }

    #[instrument(skip(self, _cx, output))]
    async fn generate(&self, _cx: &roam::session::Context, count: u32, output: Tx<i32>) {
        info!(count, "generate called");
        for i in 0..count as i32 {
            debug!(i, "sending value");
            match output.send(&i).await {
                Ok(()) => debug!(i, "sent OK"),
                Err(e) => error!(i, ?e, "send failed"),
            }
        }
        info!("generate complete");
    }

    #[instrument(skip(self, _cx, input, output))]
    async fn transform(
        &self,
        _cx: &roam::session::Context,
        mut input: Rx<String>,
        output: Tx<String>,
    ) {
        info!("transform called");
        while let Some(s) = input.recv().await.ok().flatten() {
            debug!(?s, "transforming");
            let _ = output.send(&s).await;
        }
        info!("transform complete");
    }

    // ========================================================================
    // Complex type methods
    // ========================================================================

    async fn echo_point(&self, _cx: &roam::session::Context, point: Point) -> Point {
        point
    }

    async fn create_person(
        &self,
        _cx: &roam::session::Context,
        name: String,
        age: u8,
        email: Option<String>,
    ) -> Person {
        Person { name, age, email }
    }

    async fn rectangle_area(&self, _cx: &roam::session::Context, rect: Rectangle) -> f64 {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        width * height
    }

    async fn parse_color(&self, _cx: &roam::session::Context, name: String) -> Option<Color> {
        match name.to_lowercase().as_str() {
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "blue" => Some(Color::Blue),
            _ => None,
        }
    }

    async fn shape_area(&self, _cx: &roam::session::Context, shape: Shape) -> f64 {
        match shape {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Point => 0.0,
        }
    }

    async fn create_canvas(
        &self,
        _cx: &roam::session::Context,
        name: String,
        shapes: Vec<Shape>,
        background: Color,
    ) -> Canvas {
        Canvas {
            name,
            shapes,
            background,
        }
    }

    async fn process_message(&self, _cx: &roam::session::Context, msg: Message) -> Message {
        // Echo the message back with some transformation
        match msg {
            Message::Text(s) => Message::Text(format!("processed: {s}")),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(d) => Message::Data(d.into_iter().rev().collect()),
        }
    }

    async fn get_points(&self, _cx: &roam::session::Context, count: u32) -> Vec<Point> {
        (0..count as i32)
            .map(|i| Point { x: i, y: i * 2 })
            .collect()
    }

    async fn swap_pair(&self, _cx: &roam::session::Context, pair: (i32, String)) -> (String, i32) {
        (pair.1, pair.0)
    }
}

fn main() -> Result<(), String> {
    // Initialize tracing subscriber (respects RUST_LOG env var)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let mode = std::env::var("SUBJECT_MODE").unwrap_or_else(|_| "server".to_string());
    info!("subject-rust starting in {mode} mode");

    // Manual runtime (avoid tokio-macros / syn).
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    match mode.as_str() {
        "server" => rt.block_on(run_server()),
        "client" => rt.block_on(run_client()),
        _ => Err(format!("unknown SUBJECT_MODE: {mode}")),
    }
}

/// Connector that connects to the peer specified by PEER_ADDR.
struct PeerConnector {
    addr: String,
}

impl Connector for PeerConnector {
    type Transport = TcpStream;

    async fn connect(&self) -> std::io::Result<TcpStream> {
        TcpStream::connect(&self.addr).await
    }
}

async fn run_server() -> Result<(), String> {
    let addr = std::env::var("PEER_ADDR").map_err(|_| "PEER_ADDR env var not set".to_string())?;
    let accept_connections = std::env::var("ACCEPT_CONNECTIONS").is_ok();

    info!("connecting to {addr}");

    if accept_connections {
        // Use lower-level API to accept incoming virtual connections
        run_server_with_incoming_connections(&addr).await
    } else {
        // Use connect() with our dispatcher - automatic reconnection
        run_server_simple(&addr).await
    }
}

/// Simple server mode - uses auto-reconnecting client, doesn't accept incoming connections.
async fn run_server_simple(addr: &str) -> Result<(), String> {
    let connector = PeerConnector {
        addr: addr.to_string(),
    };
    let dispatcher = testbed::TestbedDispatcher::new(TestbedService);
    let client = connect(connector, HandshakeConfig::default(), dispatcher);

    // Get handle to verify connection works (this triggers the connection)
    let handle = client.handle().await.map_err(|e| format!("{e}"))?;
    info!("connected");
    let _ = handle;

    // Keep the connection alive until peer disconnects
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if client.handle().await.is_err() {
            info!("connection closed");
            break;
        }
    }

    Ok(())
}

/// Server mode that accepts incoming virtual connections.
async fn run_server_with_incoming_connections(addr: &str) -> Result<(), String> {
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("connect failed: {e}"))?;
    let framed = LengthPrefixedFramed::new(stream);

    let dispatcher = testbed::TestbedDispatcher::new(TestbedService);
    let (handle, mut incoming, driver) =
        initiate_framed(framed, HandshakeConfig::default(), dispatcher)
            .await
            .map_err(|e| format!("handshake failed: {e}"))?;

    info!("connected, accepting incoming connections");
    let _ = handle;

    // Spawn task to handle incoming virtual connections
    peeps::spawn_tracked!("subject_incoming_connections", async move {
        while let Some(incoming_conn) = incoming.recv().await {
            info!("received incoming connection request");
            // Accept all incoming connections with empty metadata
            match incoming_conn.accept(vec![], None).await {
                Ok(_conn_handle) => {
                    info!("accepted virtual connection");
                }
                Err(e) => {
                    error!("failed to accept virtual connection: {e}");
                }
            }
        }
        info!("incoming connections channel closed");
    });

    // Run the driver until connection closes
    driver
        .run()
        .await
        .map_err(|e| format!("driver error: {e}"))?;

    Ok(())
}

async fn run_client() -> Result<(), String> {
    let addr = std::env::var("PEER_ADDR").map_err(|_| "PEER_ADDR not set".to_string())?;
    info!("connecting to {addr}");

    // Use connect() for automatic reconnection
    let connector = PeerConnector { addr };
    let dispatcher = testbed::TestbedDispatcher::new(TestbedService);
    let client = connect(connector, HandshakeConfig::default(), dispatcher);

    // Create the typed client
    let service = testbed::TestbedClient::new(client);

    // Run the client test scenario specified by CLIENT_SCENARIO env var
    let scenario = std::env::var("CLIENT_SCENARIO").unwrap_or_else(|_| "echo".to_string());
    info!("running client scenario: {scenario}");

    match scenario.as_str() {
        "echo" => {
            let result = service.echo("hello from client".to_string()).await;
            info!("echo result: {result:?}");
        }
        "sum" => {
            // Client-to-server streaming: send numbers, get sum back
            let (tx, rx) = roam::channel::<i32>();

            // Spawn task to send numbers
            peeps::spawn_tracked!("subject_sum_sender", async move {
                for i in 1..=5 {
                    debug!("sending {i}");
                    if let Err(e) = tx.send(&i).await {
                        error!("send failed: {e}");
                        break;
                    }
                }
                debug!("done sending, dropping tx");
            });

            let result = service.sum(rx).await;
            info!("sum result: {result:?}");
        }
        "generate" => {
            // Server-to-client streaming: request N numbers
            let (tx, mut rx) = roam::channel::<i32>();

            // Spawn task to receive numbers
            let recv_task = peeps::spawn_tracked!("subject_generate_receiver", async move {
                let mut received = Vec::new();
                while let Ok(Some(n)) = rx.recv().await {
                    debug!("received {n}");
                    received.push(n);
                }
                received
            });

            let result = service.generate(5, tx).await;
            info!("generate result: {result:?}");

            let received = recv_task
                .await
                .map_err(|e| format!("recv task failed: {e}"))?;
            info!("received numbers: {received:?}");
        }
        "shape_area" => {
            let result = service
                .shape_area(Shape::Rectangle {
                    width: 3.0,
                    height: 4.0,
                })
                .await
                .map_err(|e| format!("shape_area call failed: {e}"))?;
            if (result - 12.0).abs() > f64::EPSILON {
                return Err(format!("shape_area expected 12.0, got {result}"));
            }
            info!("shape_area result: {result}");
        }
        "create_canvas" => {
            let result = service
                .create_canvas(
                    "enum-canvas".to_string(),
                    vec![Shape::Point, Shape::Circle { radius: 2.5 }],
                    Color::Green,
                )
                .await
                .map_err(|e| format!("create_canvas call failed: {e}"))?;

            if result.name != "enum-canvas" {
                return Err(format!(
                    "create_canvas expected name 'enum-canvas', got {:?}",
                    result.name
                ));
            }
            if result.background != Color::Green {
                return Err(format!(
                    "create_canvas expected background Green, got {:?}",
                    result.background
                ));
            }
            if result.shapes.len() != 2 {
                return Err(format!(
                    "create_canvas expected 2 shapes, got {}",
                    result.shapes.len()
                ));
            }
            match &result.shapes[0] {
                Shape::Point => {}
                other => {
                    return Err(format!(
                        "create_canvas expected first shape Point, got {other:?}"
                    ));
                }
            }
            match &result.shapes[1] {
                Shape::Circle { radius } if (*radius - 2.5).abs() < f64::EPSILON => {}
                other => {
                    return Err(format!(
                        "create_canvas expected second shape Circle {{ radius: 2.5 }}, got {other:?}"
                    ));
                }
            }
            info!("create_canvas result: {result:?}");
        }
        "process_message" => {
            let result = service
                .process_message(Message::Data(vec![1, 2, 3, 4]))
                .await
                .map_err(|e| format!("process_message call failed: {e}"))?;
            match result {
                Message::Data(data) if data == vec![4, 3, 2, 1] => {
                    info!("process_message result: {data:?}");
                }
                other => {
                    return Err(format!(
                        "process_message expected Data([4, 3, 2, 1]), got {other:?}"
                    ));
                }
            }
        }
        _ => {
            return Err(format!("unknown CLIENT_SCENARIO: {scenario}"));
        }
    }

    Ok(())
}
