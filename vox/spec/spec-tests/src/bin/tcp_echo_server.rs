//! TCP Echo server for cross-language testing.
//!
//! Listens on a TCP port and handles Echo service requests.
//! Used to test clients in other languages against a Rust server.

use cobs::{decode_vec as cobs_decode_vec, encode_vec as cobs_encode_vec};
use roam::facet::Facet;
use roam_wire::{Hello, Message};
use spec_tests::complex::ComplexHandler;
use spec_tests::echo::EchoHandler;
use spec_tests::{complex, echo};
use std::env;
use std::future::Future;
use std::io::ErrorKind;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// Service implementation
#[derive(Clone)]
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

#[derive(Clone)]
struct ComplexService;

#[allow(clippy::manual_async_fn)]
impl complex::ComplexHandler for ComplexService {
    fn echo_point(
        &self,
        point: spec_proto::Point,
    ) -> impl Future<Output = Result<spec_proto::Point, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok(point) }
    }

    fn create_person(
        &self,
        name: String,
        age: u8,
        email: Option<String>,
    ) -> impl Future<Output = Result<spec_proto::Person, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok(spec_proto::Person { name, age, email }) }
    }

    fn rectangle_area(
        &self,
        rect: spec_proto::Rectangle,
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
    ) -> impl Future<
        Output = Result<Option<spec_proto::Color>, Box<dyn std::error::Error + Send + Sync>>,
    > + Send {
        async move {
            match name.to_lowercase().as_str() {
                "red" => Ok(Some(spec_proto::Color::Red)),
                "green" => Ok(Some(spec_proto::Color::Green)),
                "blue" => Ok(Some(spec_proto::Color::Blue)),
                _ => Ok(None),
            }
        }
    }

    fn shape_area(
        &self,
        shape: spec_proto::Shape,
    ) -> impl Future<Output = Result<f64, Box<dyn std::error::Error + Send + Sync>>> + Send {
        async move {
            let area = match shape {
                spec_proto::Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
                spec_proto::Shape::Rectangle { width, height } => width * height,
                spec_proto::Shape::Point => 0.0,
            };
            Ok(area)
        }
    }

    fn create_canvas(
        &self,
        name: String,
        shapes: Vec<spec_proto::Shape>,
        background: spec_proto::Color,
    ) -> impl Future<Output = Result<spec_proto::Canvas, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move {
            Ok(spec_proto::Canvas {
                name,
                shapes,
                background,
            })
        }
    }

    fn process_message(
        &self,
        msg: spec_proto::Message,
    ) -> impl Future<Output = Result<spec_proto::Message, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move {
            match msg {
                spec_proto::Message::Text(text) => {
                    Ok(spec_proto::Message::Text(format!("Processed: {}", text)))
                }
                spec_proto::Message::Number(n) => Ok(spec_proto::Message::Number(n * 2)),
                spec_proto::Message::Data(data) => {
                    Ok(spec_proto::Message::Data(data.into_iter().rev().collect()))
                }
            }
        }
    }

    fn get_points(
        &self,
        count: u32,
    ) -> impl Future<
        Output = Result<Vec<spec_proto::Point>, Box<dyn std::error::Error + Send + Sync>>,
    > + Send {
        async move {
            Ok((0..count as i32)
                .map(|i| spec_proto::Point { x: i, y: i * 2 })
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

struct CobsFramed {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl CobsFramed {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
        }
    }

    async fn send(&mut self, msg: &Message) -> std::io::Result<()> {
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e.to_string()))?;
        let mut framed = cobs_encode_vec(&payload);
        framed.push(0x00);
        self.stream.write_all(&framed).await?;
        self.stream.flush().await
    }

    async fn recv(&mut self) -> std::io::Result<Option<Message>> {
        loop {
            if let Some(idx) = self.buf.iter().position(|b| *b == 0x00) {
                let frame = self.buf.drain(..idx).collect::<Vec<_>>();
                self.buf.drain(..1);
                if frame.is_empty() {
                    continue;
                }
                let decoded = cobs_decode_vec(&frame).map_err(|e| {
                    std::io::Error::new(ErrorKind::InvalidData, format!("cobs: {e}"))
                })?;
                let msg: Message = facet_postcard::from_slice(&decoded).map_err(|e| {
                    std::io::Error::new(ErrorKind::InvalidData, format!("postcard: {e}"))
                })?;
                return Ok(Some(msg));
            }
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await?;
            if n == 0 {
                return Ok(None);
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }
}

async fn handle_connection(stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let mut io = CobsFramed::new(stream);

    // Send our Hello
    let our_hello = Hello::V1 {
        max_payload_size: 1024 * 1024,
        initial_stream_credit: 64 * 1024,
    };
    io.send(&Message::Hello(our_hello)).await?;

    // Wait for client Hello
    let msg = io.recv().await?.ok_or("expected Hello")?;
    let _negotiated_max = match msg {
        Message::Hello(Hello::V1 {
            max_payload_size, ..
        }) => max_payload_size,
        _ => return Err("expected Hello".into()),
    };

    // Handle requests
    loop {
        let msg = match io.recv().await? {
            Some(m) => m,
            None => break,
        };

        match msg {
            Message::Request {
                request_id,
                method_id,
                payload,
                ..
            } => {
                // Dispatch based on method_id
                let response_payload = dispatch_method(method_id, &payload).await?;

                io.send(&Message::Response {
                    request_id,
                    metadata: vec![],
                    payload: response_payload,
                })
                .await?;
            }
            Message::Goodbye { .. } => break,
            _ => {}
        }
    }

    Ok(())
}

/// Encode Result::Ok(value) - prepend 0x00 (Ok variant) to the serialized value
fn encode_ok<T: Facet<'static>>(value: &T) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut result = vec![0x00]; // Result::Ok variant
    result.extend(facet_postcard::to_vec(value)?);
    Ok(result)
}

/// Encode Result::Err(RoamError::User(msg))
fn encode_user_error(msg: &str) -> Vec<u8> {
    let mut result = vec![0x01]; // Result::Err variant
    result.push(0x00); // RoamError::User variant
    // Encode string: varint length + bytes
    let msg_bytes = msg.as_bytes();
    let len = msg_bytes.len();
    // Simple varint encoding for length
    if len < 128 {
        result.push(len as u8);
    } else {
        result.push((len & 0x7f) as u8 | 0x80);
        result.push((len >> 7) as u8);
    }
    result.extend_from_slice(msg_bytes);
    result
}

/// Encode Result::Err(RoamError::UnknownMethod)
fn encode_unknown_method() -> Vec<u8> {
    vec![0x01, 0x01] // Result::Err, RoamError::UnknownMethod
}

async fn dispatch_method(
    method_id: u64,
    payload: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let service = EchoService;
    let complex_service = ComplexService;

    // Echo methods
    if method_id == echo::method_id::ECHO {
        let args: (String,) = facet_postcard::from_slice(payload)?;
        return match service.echo(args.0).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }
    if method_id == echo::method_id::REVERSE {
        let args: (String,) = facet_postcard::from_slice(payload)?;
        return match service.reverse(args.0).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }

    // Complex methods
    if method_id == complex::method_id::ECHO_POINT {
        let args: (spec_proto::Point,) = facet_postcard::from_slice(payload)?;
        return match complex_service.echo_point(args.0).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }
    if method_id == complex::method_id::CREATE_PERSON {
        let args: (String, u8, Option<String>) = facet_postcard::from_slice(payload)?;
        return match complex_service.create_person(args.0, args.1, args.2).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }
    if method_id == complex::method_id::RECTANGLE_AREA {
        let args: (spec_proto::Rectangle,) = facet_postcard::from_slice(payload)?;
        return match complex_service.rectangle_area(args.0).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }
    if method_id == complex::method_id::PARSE_COLOR {
        let args: (String,) = facet_postcard::from_slice(payload)?;
        return match complex_service.parse_color(args.0).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }
    if method_id == complex::method_id::SHAPE_AREA {
        let args: (spec_proto::Shape,) = facet_postcard::from_slice(payload)?;
        return match complex_service.shape_area(args.0).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }
    if method_id == complex::method_id::GET_POINTS {
        let args: (u32,) = facet_postcard::from_slice(payload)?;
        return match complex_service.get_points(args.0).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }
    if method_id == complex::method_id::SWAP_PAIR {
        let args: ((i32, String),) = facet_postcard::from_slice(payload)?;
        return match complex_service.swap_pair(args.0).await {
            Ok(v) => encode_ok(&v),
            Err(e) => Ok(encode_user_error(&e.to_string())),
        };
    }

    // Unknown method
    Ok(encode_unknown_method())
}

#[tokio::main]
async fn main() {
    let port = env::var("TCP_PORT").unwrap_or_else(|_| "9001".to_string());
    let addr = format!("127.0.0.1:{}", port);

    let listener = TcpListener::bind(&addr).await.unwrap();
    eprintln!("TCP Echo server listening on {}", addr);

    // Print port on stdout for test harness
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
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Connection error: {:?}", e);
            }
            eprintln!("Connection closed: {}", peer);
        });
    }
}
