//! WebSocket server for browser testing.
//!
//! Serves Testbed service over WebSocket for cross-language testing.
//!
//! Note: This server uses roam runtime types because it needs WebSocket
//! transport support. The wire-level approach used in tcp_echo_server
//! doesn't work for WebSocket framing.

use roam::session::{Never, RoamError, Rx, Tx};
use roam_stream::Hello;
use roam_websocket::{WsTransport, ws_accept};
use spec_proto::{Canvas, Color, Message, Person, Point, Rectangle, Shape};
use spec_proto::{Testbed, TestbedDispatcher};
use std::env;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

// Service implementation using generated Testbed trait
#[derive(Clone)]
struct TestbedService;

impl Testbed for TestbedService {
    async fn echo(&self, message: String) -> Result<String, RoamError<Never>> {
        Ok(message)
    }

    async fn reverse(&self, message: String) -> Result<String, RoamError<Never>> {
        Ok(message.chars().rev().collect())
    }

    async fn sum(&self, mut numbers: Rx<i32>) -> Result<i64, RoamError<Never>> {
        let mut total: i64 = 0;
        while let Some(n) = numbers.recv().await.ok().flatten() {
            total += n as i64;
        }
        Ok(total)
    }

    async fn generate(&self, count: u32, output: Tx<i32>) -> Result<(), RoamError<Never>> {
        for i in 0..count as i32 {
            let _ = output.send(&i).await;
        }
        Ok(())
    }

    async fn transform(
        &self,
        mut input: Rx<String>,
        output: Tx<String>,
    ) -> Result<(), RoamError<Never>> {
        while let Some(s) = input.recv().await.ok().flatten() {
            let _ = output.send(&s).await;
        }
        Ok(())
    }

    async fn echo_point(&self, point: Point) -> Result<Point, RoamError<Never>> {
        Ok(point)
    }

    async fn create_person(
        &self,
        name: String,
        age: u8,
        email: Option<String>,
    ) -> Result<Person, RoamError<Never>> {
        Ok(Person { name, age, email })
    }

    async fn rectangle_area(&self, rect: Rectangle) -> Result<f64, RoamError<Never>> {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        Ok(width * height)
    }

    async fn parse_color(&self, name: String) -> Result<Option<Color>, RoamError<Never>> {
        match name.to_lowercase().as_str() {
            "red" => Ok(Some(Color::Red)),
            "green" => Ok(Some(Color::Green)),
            "blue" => Ok(Some(Color::Blue)),
            _ => Ok(None),
        }
    }

    async fn shape_area(&self, shape: Shape) -> Result<f64, RoamError<Never>> {
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
    ) -> Result<Canvas, RoamError<Never>> {
        Ok(Canvas {
            name,
            shapes,
            background,
        })
    }

    async fn process_message(&self, msg: Message) -> Result<Message, RoamError<Never>> {
        match msg {
            Message::Text(text) => Ok(Message::Text(format!("Processed: {}", text))),
            Message::Number(n) => Ok(Message::Number(n * 2)),
            Message::Data(data) => Ok(Message::Data(data.into_iter().rev().collect())),
        }
    }

    async fn get_points(&self, count: u32) -> Result<Vec<Point>, RoamError<Never>> {
        Ok((0..count as i32)
            .map(|i| Point { x: i, y: i * 2 })
            .collect())
    }

    async fn swap_pair(&self, pair: (i32, String)) -> Result<(String, i32), RoamError<Never>> {
        Ok((pair.1, pair.0))
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

    let hello = Hello::V1 {
        max_payload_size: 1024 * 1024,
        initial_channel_credit: 64 * 1024,
    };

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
        let hello = hello.clone();

        tokio::spawn(async move {
            let dispatcher = TestbedDispatcher::new(TestbedService);

            match ws_accept(transport, hello).await {
                Ok(mut conn) => {
                    eprintln!("Connection established with {}", peer);
                    if let Err(e) = conn.run(&dispatcher).await {
                        eprintln!("Connection error: {:?}", e);
                    }
                    eprintln!("Connection closed: {}", peer);
                }
                Err(e) => {
                    eprintln!("Hello exchange failed: {:?}", e);
                }
            }
        });
    }
}
