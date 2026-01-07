//! WebSocket server for browser testing.
//!
//! Serves Echo and Complex services over WebSocket for cross-language testing.

use roam_stream::{Hello, RoutedDispatcher};
use roam_websocket::{WsTransport, ws_accept};
use spec_proto::{Canvas, Color, Message, Person, Point, Rectangle, Shape};
use spec_tests::{complex, echo};
use std::env;
use std::future::Future;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

// Echo method IDs (from generated code)
const ECHO_METHODS: &[u64] = &[echo::method_id::ECHO, echo::method_id::REVERSE];

// Service implementation using generated EchoHandler trait
struct EchoService;

#[allow(clippy::manual_async_fn)]
impl echo::EchoHandler for EchoService {
    fn echo(
        &self,
        message: String,
    ) -> impl Future<Output = Result<String, Box<dyn std::error::Error + Send + Sync>>> + Send {
        async move { Ok(message) }
    }

    fn reverse(
        &self,
        message: String,
    ) -> impl Future<Output = Result<String, Box<dyn std::error::Error + Send + Sync>>> + Send {
        async move { Ok(message.chars().rev().collect()) }
    }
}

// Service implementation using generated ComplexHandler trait
struct ComplexService;

#[allow(clippy::manual_async_fn)]
impl complex::ComplexHandler for ComplexService {
    fn echo_point(
        &self,
        point: Point,
    ) -> impl Future<Output = Result<Point, Box<dyn std::error::Error + Send + Sync>>> + Send {
        async move { Ok(point) }
    }

    fn create_person(
        &self,
        name: String,
        age: u8,
        email: Option<String>,
    ) -> impl Future<Output = Result<Person, Box<dyn std::error::Error + Send + Sync>>> + Send {
        async move { Ok(Person { name, age, email }) }
    }

    fn rectangle_area(
        &self,
        rect: Rectangle,
    ) -> impl Future<Output = Result<f64, Box<dyn std::error::Error + Send + Sync>>> + Send {
        async move {
            let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
            let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
            Ok(width * height)
        }
    }

    fn parse_color(
        &self,
        name: String,
    ) -> impl Future<Output = Result<Option<Color>, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move {
            match name.to_lowercase().as_str() {
                "red" => Ok(Some(Color::Red)),
                "green" => Ok(Some(Color::Green)),
                "blue" => Ok(Some(Color::Blue)),
                _ => Ok(None),
            }
        }
    }

    fn shape_area(
        &self,
        shape: Shape,
    ) -> impl Future<Output = Result<f64, Box<dyn std::error::Error + Send + Sync>>> + Send {
        async move {
            let area = match shape {
                Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
                Shape::Rectangle { width, height } => width * height,
                Shape::Point => 0.0,
            };
            Ok(area)
        }
    }

    fn create_canvas(
        &self,
        name: String,
        shapes: Vec<Shape>,
        background: Color,
    ) -> impl Future<Output = Result<Canvas, Box<dyn std::error::Error + Send + Sync>>> + Send {
        async move {
            Ok(Canvas {
                name,
                shapes,
                background,
            })
        }
    }

    fn process_message(
        &self,
        msg: Message,
    ) -> impl Future<Output = Result<Message, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move {
            // Echo back the message with some processing
            match msg {
                Message::Text(text) => Ok(Message::Text(format!("Processed: {}", text))),
                Message::Number(n) => Ok(Message::Number(n * 2)),
                Message::Data(data) => Ok(Message::Data(data.into_iter().rev().collect())),
            }
        }
    }

    fn get_points(
        &self,
        count: u32,
    ) -> impl Future<Output = Result<Vec<Point>, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move {
            Ok((0..count as i32)
                .map(|i| Point { x: i, y: i * 2 })
                .collect())
        }
    }

    fn swap_pair(
        &self,
        pair: (i32, String),
    ) -> impl Future<Output = Result<(String, i32), Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok((pair.1, pair.0)) }
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
        initial_stream_credit: 64 * 1024,
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
            // Combine Echo and Complex dispatchers using RoutedDispatcher
            let echo_dispatcher = echo::EchoDispatcher::new(EchoService);
            let complex_dispatcher = complex::ComplexDispatcher::new(ComplexService);
            let dispatcher =
                RoutedDispatcher::new(echo_dispatcher, complex_dispatcher, ECHO_METHODS);

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
