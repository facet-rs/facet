//! WebSocket peer server for testing TypeScript clients.
//!
//! This is a full roam implementation that TypeScript browser tests can
//! connect to. It's intentionally NOT in spec-tests because spec-tests
//! should only contain wire-level test infrastructure.
//!
//! This server uses the roam runtime (dispatcher, channels, etc.) to
//! provide a real roam peer for the TypeScript client to talk to.

use roam::session::{Rx, Tx};
use roam_stream::HandshakeConfig;
use roam_websocket::{WsTransport, ws_accept};
use spec_proto::{Canvas, Color, LookupError, MathError, Message, Person, Point, Rectangle, Shape};
use spec_proto::{Testbed, TestbedDispatcher};
use std::env;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

// Service implementation using generated Testbed trait
#[derive(Clone)]
struct TestbedService;

impl Testbed for TestbedService {
    async fn echo(&self, _cx: &roam::session::Context, message: String) -> String {
        message
    }

    async fn reverse(&self, _cx: &roam::session::Context, message: String) -> String {
        message.chars().rev().collect()
    }

    async fn divide(
        &self,
        _cx: &roam::session::Context,
        dividend: i64,
        divisor: i64,
    ) -> Result<i64, MathError> {
        if divisor == 0 {
            Err(MathError::DivisionByZero)
        } else {
            Ok(dividend / divisor)
        }
    }

    async fn lookup(&self, _cx: &roam::session::Context, id: u32) -> Result<Person, LookupError> {
        // Only IDs 1-3 exist
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

    async fn sum(&self, _cx: &roam::session::Context, mut numbers: Rx<i32>) -> i64 {
        let mut total: i64 = 0;
        while let Some(n) = numbers.recv().await.ok().flatten() {
            total += n as i64;
        }
        total
    }

    async fn generate(&self, _cx: &roam::session::Context, count: u32, output: Tx<i32>) {
        for i in 0..count as i32 {
            let _ = output.send(&i).await;
        }
    }

    async fn transform(
        &self,
        _cx: &roam::session::Context,
        mut input: Rx<String>,
        output: Tx<String>,
    ) {
        while let Some(s) = input.recv().await.ok().flatten() {
            let _ = output.send(&s).await;
        }
    }

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
        match msg {
            Message::Text(text) => Message::Text(format!("Processed: {}", text)),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(data) => Message::Data(data.into_iter().rev().collect()),
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

#[tokio::main]
async fn main() {
    let port = env::var("WS_PORT").unwrap_or_else(|_| "9000".to_string());
    let addr = format!("127.0.0.1:{}", port);

    let listener = TcpListener::bind(&addr).await.unwrap();
    eprintln!("WebSocket server listening on ws://{}", addr);

    // Print port on stdout for Playwright to parse
    println!("{}", port);

    let config = HandshakeConfig::default();

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Accept error: {}", e);
                continue;
            }
        };

        eprintln!("New connection from {}", peer);

        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("WebSocket handshake failed: {}", e);
                continue;
            }
        };

        let transport = WsTransport::new(ws_stream);
        let config = config.clone();

        tokio::spawn(async move {
            let dispatcher = TestbedDispatcher::new(TestbedService);

            match ws_accept(transport, config, dispatcher).await {
                Ok((handle, _incoming, driver)) => {
                    // Note: We drop `_incoming` - this server doesn't accept sub-connections.
                    eprintln!("Connection established with {}", peer);
                    if let Err(e) = driver.run().await {
                        eprintln!("Connection error: {:?}", e);
                    }
                    eprintln!("Connection closed: {}", peer);
                    let _ = handle;
                }
                Err(e) => {
                    eprintln!("Hello exchange failed: {:?}", e);
                }
            }
        });
    }
}
