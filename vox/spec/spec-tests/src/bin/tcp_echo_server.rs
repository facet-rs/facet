//! Rust TCP server for cross-language testing.
//!
//! Listens on a TCP port and handles Testbed service requests.
//! Used to test clients in other languages against a Rust server.
//!
//! This is a wire-level implementation that does not use any roam runtime types.

use facet::Facet;
use roam_wire::{Hello, Message};
use spec_tests::testbed::method_id;
use std::env;
use std::io::ErrorKind;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

struct LengthPrefixedFramed {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl LengthPrefixedFramed {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
        }
    }

    async fn send(&mut self, msg: &Message) -> std::io::Result<()> {
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e.to_string()))?;
        let len = u32::try_from(payload.len())
            .map_err(|_| std::io::Error::new(ErrorKind::InvalidInput, "frame too large"))?
            .to_le_bytes();
        self.stream.write_all(&len).await?;
        self.stream.write_all(&payload).await?;
        self.stream.flush().await
    }

    async fn recv(&mut self) -> std::io::Result<Option<Message>> {
        loop {
            if self.buf.len() >= 4 {
                let len = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]])
                    as usize;
                let needed = 4 + len;
                if self.buf.len() >= needed {
                    let frame = self.buf[4..needed].to_vec();
                    self.buf.drain(..needed);
                    let msg: Message = facet_postcard::from_slice(&frame).map_err(|e| {
                        std::io::Error::new(ErrorKind::InvalidData, format!("postcard: {e}"))
                    })?;
                    return Ok(Some(msg));
                }
            }
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await?;
            if n == 0 {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("eof with {} trailing bytes", self.buf.len()),
                ));
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }
}

async fn handle_connection(stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let mut io = LengthPrefixedFramed::new(stream);

    // Send our Hello
    let our_hello = Hello::V6 {
        max_payload_size: 1024 * 1024,
        initial_channel_credit: 64 * 1024,
        max_concurrent_requests: 64,
        metadata: vec![],
    };
    io.send(&Message::Hello(our_hello)).await?;

    // Wait for client Hello
    let msg = io.recv().await?.ok_or("expected Hello")?;
    let _negotiated_max = match msg {
        Message::Hello(Hello::V6 {
            max_payload_size, ..
        }) => max_payload_size,
        _ => return Err("expected Hello::V6".into()),
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
                let response_payload = dispatch_method(method_id, &payload)?;

                io.send(&Message::Response {
                    conn_id: roam_wire::ConnectionId::ROOT,
                    request_id,
                    metadata: vec![],
                    channels: vec![],
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

/// Encode Result::Ok(value)
fn encode_ok<T: for<'a> Facet<'a>>(value: &T) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut result = vec![0x00]; // Result::Ok variant
    result.extend(facet_postcard::to_vec(value)?);
    Ok(result)
}

/// Encode Result::Err(RoamError::UnknownMethod)
fn encode_unknown_method() -> Vec<u8> {
    vec![0x01, 0x01] // Result::Err, RoamError::UnknownMethod
}

fn dispatch_method(method: u64, payload: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Simple RPC methods
    if method == method_id::echo() {
        let args: (String,) = facet_postcard::from_slice(payload)?;
        return encode_ok(&args.0);
    }
    if method == method_id::reverse() {
        let args: (String,) = facet_postcard::from_slice(payload)?;
        let reversed: String = args.0.chars().rev().collect();
        return encode_ok(&reversed);
    }

    // Complex type methods
    if method == method_id::echo_point() {
        let args: (spec_proto::Point,) = facet_postcard::from_slice(payload)?;
        return encode_ok(&args.0);
    }
    if method == method_id::create_person() {
        let args: (String, u8, Option<String>) = facet_postcard::from_slice(payload)?;
        let person = spec_proto::Person {
            name: args.0,
            age: args.1,
            email: args.2,
        };
        return encode_ok(&person);
    }
    if method == method_id::rectangle_area() {
        let args: (spec_proto::Rectangle,) = facet_postcard::from_slice(payload)?;
        let rect = args.0;
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        let area = width * height;
        return encode_ok(&area);
    }
    if method == method_id::parse_color() {
        let args: (String,) = facet_postcard::from_slice(payload)?;
        let color: Option<spec_proto::Color> = match args.0.to_lowercase().as_str() {
            "red" => Some(spec_proto::Color::Red),
            "green" => Some(spec_proto::Color::Green),
            "blue" => Some(spec_proto::Color::Blue),
            _ => None,
        };
        return encode_ok(&color);
    }
    if method == method_id::shape_area() {
        let args: (spec_proto::Shape,) = facet_postcard::from_slice(payload)?;
        let area = match args.0 {
            spec_proto::Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            spec_proto::Shape::Rectangle { width, height } => width * height,
            spec_proto::Shape::Point => 0.0,
        };
        return encode_ok(&area);
    }
    if method == method_id::get_points() {
        let args: (u32,) = facet_postcard::from_slice(payload)?;
        let points: Vec<spec_proto::Point> = (0..args.0 as i32)
            .map(|i| spec_proto::Point { x: i, y: i * 2 })
            .collect();
        return encode_ok(&points);
    }
    if method == method_id::swap_pair() {
        let args: ((i32, String),) = facet_postcard::from_slice(payload)?;
        let swapped = (args.0.1, args.0.0);
        return encode_ok(&swapped);
    }
    if method == method_id::create_canvas() {
        let args: (String, Vec<spec_proto::Shape>, spec_proto::Color) =
            facet_postcard::from_slice(payload)?;
        let canvas = spec_proto::Canvas {
            name: args.0,
            shapes: args.1,
            background: args.2,
        };
        return encode_ok(&canvas);
    }
    if method == method_id::process_message() {
        let args: (spec_proto::Message,) = facet_postcard::from_slice(payload)?;
        let response = match args.0 {
            spec_proto::Message::Text(text) => {
                spec_proto::Message::Text(format!("Processed: {}", text))
            }
            spec_proto::Message::Number(n) => spec_proto::Message::Number(n * 2),
            spec_proto::Message::Data(data) => {
                spec_proto::Message::Data(data.into_iter().rev().collect())
            }
        };
        return encode_ok(&response);
    }

    // Unknown method
    Ok(encode_unknown_method())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:0".to_string());
    let listener = TcpListener::bind(&addr).await?;
    let local_addr = listener.local_addr()?;

    // Print actual address for test harness to capture
    println!("LISTENING:{}", local_addr);

    // Accept one connection and handle it
    let (stream, _peer) = listener.accept().await?;
    handle_connection(stream).await?;

    Ok(())
}
