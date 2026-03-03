//! WebSocket peer server for testing TypeScript clients.
//!
//! This is a full roam implementation that TypeScript browser tests can
//! connect to. It uses the roam runtime (dispatcher, channels, etc.) to
//! provide a real roam peer for the TypeScript client to talk to.

use roam::{Rx, Tx};
use roam_core::acceptor;
use roam_websocket::WsLink;
use spec_proto::{Canvas, Color, LookupError, MathError, Message, Person, Point, Rectangle, Shape};
use spec_proto::{Testbed, TestbedClient, TestbedDispatcher};
use std::env;
use tokio::net::TcpListener;

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
            let _ = output.send(i).await;
        }
        let _ = output.close(Default::default()).await;
    }

    async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
        while let Ok(Some(s)) = input.recv().await {
            let _ = output.send((*s).clone()).await;
        }
        let _ = output.close(Default::default()).await;
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
            Message::Text(text) => Message::Text(format!("Processed: {}", text)),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(data) => Message::Data(data.into_iter().rev().collect()),
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

#[tokio::main]
async fn main() {
    let port = env::var("WS_PORT").unwrap_or_else(|_| "9000".to_string());
    let addr = format!("127.0.0.1:{}", port);

    let listener = TcpListener::bind(&addr).await.unwrap();
    eprintln!("WebSocket server listening on ws://{}", addr);

    // Print port on stdout for Playwright to parse
    println!("{}", port);

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Accept error: {}", e);
                continue;
            }
        };

        eprintln!("New connection from {}", peer);

        tokio::spawn(async move {
            let ws_link = match WsLink::server(stream).await {
                Ok(link) => link,
                Err(e) => {
                    eprintln!("WebSocket handshake failed: {}", e);
                    return;
                }
            };

            let (root_caller_guard, _sh) = match acceptor(ws_link)
                .establish::<TestbedClient>(TestbedDispatcher::new(TestbedService))
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Session handshake failed: {:?}", e);
                    return;
                }
            };

            eprintln!("Connection established with {}", peer);
            let _root_caller_guard = root_caller_guard;
            std::future::pending::<()>().await;
        });
    }
}
