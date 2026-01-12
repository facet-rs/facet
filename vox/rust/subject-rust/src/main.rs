//! Rust subject binary for the roam compliance suite.
//!
//! This demonstrates the minimal code needed to implement a roam service
//! using the roam-stream transport library.

use roam::session::{Rx, Tx};
use roam_stream::{Connector, HandshakeConfig, connect};
use tokio::net::TcpStream;
use tracing::{debug, error, info, instrument};

// Re-export types from spec_proto
pub use spec_proto::{Canvas, Color, Message, Person, Point, Rectangle, Shape};

// Re-export generated service items from spec-proto as a `testbed` module
mod testbed {
    pub use roam::session::{Never, RoamError};
    pub use spec_proto::{Testbed, TestbedClient, TestbedDispatcher};
}

// Service implementation using generated Testbed trait
#[derive(Clone)]
struct TestbedService;

impl testbed::Testbed for TestbedService {
    // ========================================================================
    // Unary methods
    // ========================================================================

    #[instrument(skip(self))]
    async fn echo(&self, message: String) -> Result<String, testbed::RoamError<testbed::Never>> {
        info!("echo called");
        Ok(message)
    }

    #[instrument(skip(self))]
    async fn reverse(&self, message: String) -> Result<String, testbed::RoamError<testbed::Never>> {
        info!("reverse called");
        Ok(message.chars().rev().collect())
    }

    // ========================================================================
    // Streaming methods
    // ========================================================================

    #[instrument(skip(self, numbers))]
    async fn sum(&self, mut numbers: Rx<i32>) -> Result<i64, testbed::RoamError<testbed::Never>> {
        info!("sum called, starting to receive numbers");
        let mut total: i64 = 0;
        while let Some(n) = numbers.recv().await.ok().flatten() {
            debug!(n, total, "received number");
            total += n as i64;
        }
        info!(total, "sum complete");
        Ok(total)
    }

    #[instrument(skip(self, output))]
    async fn generate(
        &self,
        count: u32,
        output: Tx<i32>,
    ) -> Result<(), testbed::RoamError<testbed::Never>> {
        info!(count, "generate called");
        for i in 0..count as i32 {
            debug!(i, "sending value");
            match output.send(&i).await {
                Ok(()) => debug!(i, "sent OK"),
                Err(e) => error!(i, ?e, "send failed"),
            }
        }
        info!("generate complete");
        Ok(())
    }

    #[instrument(skip(self, input, output))]
    async fn transform(
        &self,
        mut input: Rx<String>,
        output: Tx<String>,
    ) -> Result<(), testbed::RoamError<testbed::Never>> {
        info!("transform called");
        while let Some(s) = input.recv().await.ok().flatten() {
            debug!(?s, "transforming");
            let _ = output.send(&s).await;
        }
        info!("transform complete");
        Ok(())
    }

    // ========================================================================
    // Complex type methods
    // ========================================================================

    async fn echo_point(&self, point: Point) -> Result<Point, testbed::RoamError<testbed::Never>> {
        Ok(point)
    }

    async fn create_person(
        &self,
        name: String,
        age: u8,
        email: Option<String>,
    ) -> Result<Person, testbed::RoamError<testbed::Never>> {
        Ok(Person { name, age, email })
    }

    async fn rectangle_area(
        &self,
        rect: Rectangle,
    ) -> Result<f64, testbed::RoamError<testbed::Never>> {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        Ok(width * height)
    }

    async fn parse_color(
        &self,
        name: String,
    ) -> Result<Option<Color>, testbed::RoamError<testbed::Never>> {
        let color = match name.to_lowercase().as_str() {
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "blue" => Some(Color::Blue),
            _ => None,
        };
        Ok(color)
    }

    async fn shape_area(&self, shape: Shape) -> Result<f64, testbed::RoamError<testbed::Never>> {
        let area = match shape {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Point => 0.0,
        };
        Ok(area)
    }

    async fn create_canvas(
        &self,
        name: String,
        shapes: Vec<Shape>,
        background: Color,
    ) -> Result<Canvas, testbed::RoamError<testbed::Never>> {
        Ok(Canvas {
            name,
            shapes,
            background,
        })
    }

    async fn process_message(
        &self,
        msg: Message,
    ) -> Result<Message, testbed::RoamError<testbed::Never>> {
        // Echo the message back with some transformation
        let response = match msg {
            Message::Text(s) => Message::Text(format!("processed: {s}")),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(d) => Message::Data(d.into_iter().rev().collect()),
        };
        Ok(response)
    }

    async fn get_points(
        &self,
        count: u32,
    ) -> Result<Vec<Point>, testbed::RoamError<testbed::Never>> {
        let points = (0..count as i32)
            .map(|i| Point { x: i, y: i * 2 })
            .collect();
        Ok(points)
    }

    async fn swap_pair(
        &self,
        pair: (i32, String),
    ) -> Result<(String, i32), testbed::RoamError<testbed::Never>> {
        Ok((pair.1, pair.0))
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

    info!("connecting to {addr}");

    // Use connect() with our dispatcher - automatic reconnection
    let connector = PeerConnector { addr };
    let dispatcher = testbed::TestbedDispatcher::new(TestbedService);
    let client = connect(connector, HandshakeConfig::default(), dispatcher);

    // Get handle to verify connection works (this triggers the connection)
    let handle = client.handle().await.map_err(|e| format!("{e}"))?;
    info!("connected");
    let _ = handle;

    // Keep the connection alive until peer disconnects
    // In a real scenario, we'd have a proper shutdown mechanism
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        // Check if still connected by trying to get handle
        if client.handle().await.is_err() {
            info!("connection closed");
            break;
        }
    }

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
            tokio::spawn(async move {
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
            let recv_task = tokio::spawn(async move {
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
        _ => {
            return Err(format!("unknown CLIENT_SCENARIO: {scenario}"));
        }
    }

    Ok(())
}
