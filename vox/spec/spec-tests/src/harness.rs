use std::process::Stdio;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use roam_wire::{Hello, Message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};

/// Enable wire-level message logging for debugging.
/// Set ROAM_WIRE_SPY=1 to enable.
static WIRE_SPY_ENABLED: AtomicBool = AtomicBool::new(false);
static WIRE_SPY_INIT: OnceLock<()> = OnceLock::new();

fn wire_spy_enabled() -> bool {
    WIRE_SPY_INIT.get_or_init(|| {
        if std::env::var("ROAM_WIRE_SPY").is_ok() {
            WIRE_SPY_ENABLED.store(true, Ordering::Relaxed);
        }
    });

    WIRE_SPY_ENABLED.load(Ordering::Relaxed)
}

fn format_message(msg: &Message, direction: &str) -> String {
    match msg {
        Message::Hello(hello) => match hello {
            Hello::V1 {
                max_payload_size,
                initial_channel_credit,
            } => format!(
                "{direction} Hello::V1 {{ max_payload: {max_payload_size}, credit: {initial_channel_credit} }}"
            ),
            Hello::V2 {
                max_payload_size,
                initial_channel_credit,
            } => format!(
                "{direction} Hello::V2 {{ max_payload: {max_payload_size}, credit: {initial_channel_credit} }}"
            ),
            Hello::V3 {
                max_payload_size,
                initial_channel_credit,
            } => format!(
                "{direction} Hello::V3 {{ max_payload: {max_payload_size}, credit: {initial_channel_credit} }}"
            ),
            Hello::V4 {
                max_payload_size,
                initial_channel_credit,
            } => format!(
                "{direction} Hello::V4 {{ max_payload: {max_payload_size}, credit: {initial_channel_credit} }}"
            ),
            Hello::V5 {
                max_payload_size,
                initial_channel_credit,
                max_concurrent_requests,
            } => format!(
                "{direction} Hello::V5 {{ max_payload: {max_payload_size}, credit: {initial_channel_credit}, max_concurrent_requests: {max_concurrent_requests} }}"
            ),
        },
        Message::Goodbye { reason, .. } => format!("{direction} Goodbye {{ reason: {reason:?} }}"),
        Message::Request {
            request_id,
            method_id,
            payload,
            ..
        } => format!(
            "{direction} Request {{ id: {request_id}, method: 0x{method_id:016x}, payload: {} bytes }}",
            payload.len()
        ),
        Message::Response {
            request_id,
            payload,
            ..
        } => format!(
            "{direction} Response {{ id: {request_id}, payload: {} bytes }}",
            payload.len()
        ),
        Message::Cancel { request_id, .. } => format!("{direction} Cancel {{ id: {request_id} }}"),
        Message::Data {
            channel_id,
            payload,
            ..
        } => format!(
            "{direction} Data {{ channel: {channel_id}, payload: {} bytes }}",
            payload.len()
        ),
        Message::Close { channel_id, .. } => {
            format!("{direction} Close {{ channel: {channel_id} }}")
        }
        Message::Reset { channel_id, .. } => {
            format!("{direction} Reset {{ channel: {channel_id} }}")
        }
        Message::Credit {
            channel_id, bytes, ..
        } => {
            format!("{direction} Credit {{ channel: {channel_id}, bytes: {bytes} }}")
        }
        Message::Connect { .. } => format!("{direction} Connect {{ ... }}"),
        Message::Accept { .. } => format!("{direction} Accept {{ ... }}"),
        Message::Reject { .. } => format!("{direction} Reject {{ ... }}"),
    }
}

pub fn workspace_root() -> &'static std::path::Path {
    // `spec/spec-tests` → `spec` → workspace root
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

pub fn subject_cmd() -> String {
    match std::env::var("SUBJECT_CMD") {
        Ok(s) if !s.trim().is_empty() => s,
        _ => "./target/release/subject-rust".to_string(),
    }
}

pub fn run_async<T>(f: impl std::future::Future<Output = T>) -> T {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(f)
}

pub fn our_hello(max_payload_size: u32) -> Hello {
    Hello::V5 {
        max_payload_size,
        initial_channel_credit: 64 * 1024,
        max_concurrent_requests: 64,
    }
}

pub struct LengthPrefixedFramed {
    pub stream: TcpStream,
    buf: Vec<u8>,
}

impl LengthPrefixedFramed {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
        }
    }

    pub async fn send(&mut self, msg: &Message) -> std::io::Result<()> {
        if wire_spy_enabled() {
            eprintln!("[WIRE] {}", format_message(msg, "-->"));
        }
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        let frame_len = u32::try_from(payload.len())
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "frame too large"))?
            .to_le_bytes();

        self.stream.write_all(&frame_len).await?;
        self.stream.write_all(&payload).await?;
        self.stream.flush().await?;
        Ok(())
    }

    pub async fn recv_timeout(&mut self, timeout: Duration) -> std::io::Result<Option<Message>> {
        tokio::time::timeout(timeout, self.recv_inner())
            .await
            .unwrap_or(Ok(None))
    }

    async fn recv_inner(&mut self) -> std::io::Result<Option<Message>> {
        loop {
            if self.buf.len() >= 4 {
                let frame_len =
                    u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]])
                        as usize;
                let needed = 4 + frame_len;
                if self.buf.len() >= needed {
                    let frame = self.buf[4..needed].to_vec();
                    self.buf.drain(..needed);

                    let msg: Message = facet_postcard::from_slice(&frame).map_err(|e| {
                        eprintln!(
                            "Failed to decode {} bytes: {:02x?}",
                            frame.len(),
                            &frame[..frame.len().min(64)]
                        );
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("postcard: {e}"),
                        )
                    })?;
                    if wire_spy_enabled() {
                        eprintln!("[WIRE] {}", format_message(&msg, "<--"));
                    }
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
                    std::io::ErrorKind::UnexpectedEof,
                    format!("eof with {} trailing bytes", self.buf.len()),
                ));
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }
}

pub async fn spawn_subject(peer_addr: &str) -> Result<Child, String> {
    let cmd = subject_cmd();

    // Use a shell so SUBJECT_CMD can be `node subject.js`, etc.
    let mut child = Command::new("sh")
        .current_dir(workspace_root())
        .arg("-lc")
        .arg(cmd)
        .env("PEER_ADDR", peer_addr)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn subject: {e}"))?;

    // If it exits immediately, surface that early.
    tokio::time::sleep(Duration::from_millis(10)).await;
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
        return Err(format!("subject exited immediately with {status}"));
    }

    Ok(child)
}

pub async fn accept_subject() -> Result<(LengthPrefixedFramed, Child), String> {
    accept_subject_with_options(false).await
}

/// Accept subject with option to enable incoming virtual connections.
pub async fn accept_subject_with_options(
    accept_connections: bool,
) -> Result<(LengthPrefixedFramed, Child), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    let child = spawn_subject_with_options(&addr.to_string(), accept_connections).await?;

    let (stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .map_err(|_| "subject did not connect within 5s".to_string())?
        .map_err(|e| format!("accept: {e}"))?;

    Ok((LengthPrefixedFramed::new(stream), child))
}

/// Spawn subject with option to enable incoming virtual connections.
pub async fn spawn_subject_with_options(
    peer_addr: &str,
    accept_connections: bool,
) -> Result<Child, String> {
    let cmd = subject_cmd();

    let mut command = Command::new("sh");
    command
        .current_dir(workspace_root())
        .arg("-lc")
        .arg(cmd)
        .env("PEER_ADDR", peer_addr)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if accept_connections {
        command.env("ACCEPT_CONNECTIONS", "1");
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to spawn subject: {e}"))?;

    // If it exits immediately, surface that early.
    tokio::time::sleep(Duration::from_millis(10)).await;
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
        return Err(format!("subject exited immediately with {status}"));
    }

    Ok(child)
}

/// Spawn subject in client mode with the given scenario.
///
/// The subject will connect to us, and we act as the server.
pub async fn spawn_subject_client(peer_addr: &str, scenario: &str) -> Result<Child, String> {
    let cmd = subject_cmd();

    // Use a shell so SUBJECT_CMD can be `node subject.js`, etc.
    let mut child = Command::new("sh")
        .current_dir(workspace_root())
        .arg("-lc")
        .arg(cmd)
        .env("PEER_ADDR", peer_addr)
        .env("SUBJECT_MODE", "client")
        .env("CLIENT_SCENARIO", scenario)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn subject: {e}"))?;

    // If it exits immediately, surface that early.
    tokio::time::sleep(Duration::from_millis(10)).await;
    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
        return Err(format!("subject exited immediately with {status}"));
    }

    Ok(child)
}

pub async fn wait_for_goodbye_with_rule(
    io: &mut LengthPrefixedFramed,
    rule: &str,
) -> Result<(), String> {
    let mut saw_reason = None::<String>;
    for _ in 0..10 {
        match io
            .recv_timeout(Duration::from_millis(250))
            .await
            .map_err(|e| e.to_string())?
        {
            None => break,
            Some(Message::Goodbye { reason, .. }) => {
                saw_reason = Some(reason);
                break;
            }
            Some(_) => continue,
        }
    }

    let reason = saw_reason.ok_or_else(|| "expected Goodbye, got none".to_string())?;
    if !reason.contains(rule) {
        return Err(format!(
            "Goodbye reason must mention {rule}, got {reason:?}"
        ));
    }
    Ok(())
}

pub mod wire_server {
    use super::*;
    use std::collections::HashMap;

    /// Run a wire-level server for the given scenario.
    ///
    /// Spawns the subject in client mode, accepts its connection, and handles
    /// the protocol exchange at the wire level.
    pub async fn run(scenario: &str, method_ids: &MethodIds) -> Result<(), String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("bind: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("local_addr: {e}"))?;

        // Spawn subject in client mode
        let mut child = spawn_subject_client(&addr.to_string(), scenario).await?;

        // Accept the connection
        let (stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .map_err(|_| "subject did not connect within 5s".to_string())?
            .map_err(|e| format!("accept: {e}"))?;

        let mut io = LengthPrefixedFramed::new(stream);

        // Hello exchange
        io.send(&Message::Hello(our_hello(1024 * 1024)))
            .await
            .map_err(|e| format!("send hello: {e}"))?;

        let msg = io
            .recv_timeout(Duration::from_secs(5))
            .await
            .map_err(|e| format!("recv hello: {e}"))?
            .ok_or("connection closed before hello")?;

        match msg {
            Message::Hello(Hello::V4 { .. } | Hello::V5 { .. }) => {}
            other => return Err(format!("expected Hello::V4/V5, got {other:?}")),
        }

        // Handle requests until client disconnects
        let result = handle_requests(&mut io, scenario, method_ids).await;

        // Wait for child to exit
        let status = child.wait().await.map_err(|e| format!("wait: {e}"))?;
        if !status.success() {
            return Err(format!("subject exited with {status}"));
        }

        result
    }

    /// Method IDs for the Testbed service.
    ///
    /// These are passed in from tests so the harness doesn't depend on spec-proto.
    #[derive(Clone)]
    pub struct MethodIds {
        pub echo: u64,
        pub reverse: u64,
        pub sum: u64,
        pub generate: u64,
        pub transform: u64,
        pub shape_area: u64,
        pub create_canvas: u64,
        pub process_message: u64,
    }

    fn metadata_empty() -> roam_wire::Metadata {
        Vec::new()
    }

    /// Encode a successful result: Result::Ok(value)
    fn encode_ok<T: for<'a> facet::Facet<'a>>(value: &T) -> Result<Vec<u8>, String> {
        let mut result = vec![0x00]; // Result::Ok variant
        result
            .extend(facet_postcard::to_vec(value).map_err(|e| format!("encode ok payload: {e}"))?);
        Ok(result)
    }

    async fn handle_requests(
        io: &mut LengthPrefixedFramed,
        scenario: &str,
        method_ids: &MethodIds,
    ) -> Result<(), String> {
        // Track open channels: channel_id -> accumulated data
        let mut channels: HashMap<u64, Vec<i32>> = HashMap::new();
        // Track pending requests that are waiting for channel data
        let mut pending_sum: Option<(u64, u64)> = None; // (request_id, channel_id)
        let mut scenario_satisfied = false;

        loop {
            let msg = match io.recv_timeout(Duration::from_secs(5)).await {
                Ok(Some(m)) => m,
                Ok(None) => break, // Client disconnected or idle timeout
                Err(e) => return Err(format!("recv: {e}")),
            };

            match msg {
                Message::Request {
                    request_id,
                    method_id,
                    payload,
                    ..
                } => {
                    if method_id == method_ids.echo {
                        // echo(message: String) -> String
                        let args: (String,) = facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("decode echo args: {e}"))?;
                        let response_payload = encode_ok(&args.0)?;
                        io.send(&Message::Response {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            request_id,
                            metadata: metadata_empty(),
                            channels: vec![],
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send response: {e}"))?;
                    } else if method_id == method_ids.reverse {
                        // reverse(message: String) -> String
                        let args: (String,) = facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("decode reverse args: {e}"))?;
                        let reversed: String = args.0.chars().rev().collect();
                        let response_payload = encode_ok(&reversed)?;
                        io.send(&Message::Response {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            request_id,
                            metadata: metadata_empty(),
                            channels: vec![],
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send response: {e}"))?;
                    } else if method_id == method_ids.sum {
                        // sum(numbers: Rx<i32>) -> i64
                        // Payload is (channel_id: u64)
                        let args: (u64,) = facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("decode sum args: {e}"))?;
                        let channel_id = args.0;
                        channels.insert(channel_id, Vec::new());
                        pending_sum = Some((request_id, channel_id));
                        // Response will be sent when we receive Close
                    } else if method_id == method_ids.generate {
                        // generate(count: u32, output: Tx<i32>)
                        // Payload is (count: u32, channel_id: u64)
                        let args: (u32, u64) = facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("decode generate args: {e}"))?;
                        let (count, channel_id) = args;

                        // Send Data messages
                        for i in 0..count as i32 {
                            let data_payload = facet_postcard::to_vec(&i)
                                .map_err(|e| format!("encode data: {e}"))?;
                            io.send(&Message::Data {
                                conn_id: roam_wire::ConnectionId::ROOT,
                                channel_id,
                                payload: data_payload,
                            })
                            .await
                            .map_err(|e| format!("send data: {e}"))?;
                        }

                        // Send Close
                        io.send(&Message::Close {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            channel_id,
                        })
                        .await
                        .map_err(|e| format!("send close: {e}"))?;

                        // Send Response
                        let response_payload = encode_ok(&())?;
                        io.send(&Message::Response {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            request_id,
                            metadata: metadata_empty(),
                            channels: vec![],
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send response: {e}"))?;
                    } else if method_id == method_ids.transform {
                        // transform(input: Rx<String>, output: Tx<String>)
                        // This is more complex - we'll handle it if needed
                        return Err("transform not yet implemented in wire server".to_string());
                    } else if method_id == method_ids.shape_area {
                        // shape_area(shape: Shape) -> f64
                        let args: (spec_proto::Shape,) = facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("decode shape_area args: {e}"))?;
                        match args.0 {
                            spec_proto::Shape::Rectangle { width, height }
                                if (width - 3.0).abs() < f64::EPSILON
                                    && (height - 4.0).abs() < f64::EPSILON => {}
                            other => {
                                return Err(format!(
                                    "shape_area expected Rectangle {{ width: 3.0, height: 4.0 }}, got {other:?}"
                                ));
                            }
                        }

                        let response_payload = encode_ok(&12.0_f64)?;
                        io.send(&Message::Response {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            request_id,
                            metadata: metadata_empty(),
                            channels: vec![],
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send shape_area response: {e}"))?;
                        if scenario == "shape_area" {
                            scenario_satisfied = true;
                        }
                    } else if method_id == method_ids.create_canvas {
                        // create_canvas(name: String, shapes: Vec<Shape>, background: Color) -> Canvas
                        let args: (String, Vec<spec_proto::Shape>, spec_proto::Color) =
                            facet_postcard::from_slice(&payload)
                                .map_err(|e| format!("decode create_canvas args: {e}"))?;

                        if args.0 != "enum-canvas" {
                            return Err(format!(
                                "create_canvas expected name 'enum-canvas', got {:?}",
                                args.0
                            ));
                        }
                        if args.2 != spec_proto::Color::Green {
                            return Err(format!(
                                "create_canvas expected background Green, got {:?}",
                                args.2
                            ));
                        }
                        if args.1.len() != 2 {
                            return Err(format!(
                                "create_canvas expected 2 shapes, got {}",
                                args.1.len()
                            ));
                        }
                        match &args.1[0] {
                            spec_proto::Shape::Point => {}
                            other => {
                                return Err(format!(
                                    "create_canvas expected first shape Point, got {other:?}"
                                ));
                            }
                        }
                        match &args.1[1] {
                            spec_proto::Shape::Circle { radius }
                                if (*radius - 2.5).abs() < f64::EPSILON => {}
                            other => {
                                return Err(format!(
                                    "create_canvas expected second shape Circle {{ radius: 2.5 }}, got {other:?}"
                                ));
                            }
                        }

                        let canvas = spec_proto::Canvas {
                            name: args.0,
                            shapes: args.1,
                            background: args.2,
                        };
                        let response_payload = encode_ok(&canvas)?;
                        io.send(&Message::Response {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            request_id,
                            metadata: metadata_empty(),
                            channels: vec![],
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send create_canvas response: {e}"))?;
                        if scenario == "create_canvas" {
                            scenario_satisfied = true;
                        }
                    } else if method_id == method_ids.process_message {
                        // process_message(msg: Message) -> Message
                        let args: (spec_proto::Message,) = facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("decode process_message args: {e}"))?;
                        match args.0 {
                            spec_proto::Message::Data(ref data)
                                if data.as_slice() == [1, 2, 3, 4] => {}
                            ref other => {
                                return Err(format!(
                                    "process_message expected Data([1, 2, 3, 4]), got {other:?}"
                                ));
                            }
                        }

                        let response_msg = spec_proto::Message::Data(vec![4, 3, 2, 1]);
                        let response_payload = encode_ok(&response_msg)?;
                        io.send(&Message::Response {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            request_id,
                            metadata: metadata_empty(),
                            channels: vec![],
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send process_message response: {e}"))?;
                        if scenario == "process_message" {
                            scenario_satisfied = true;
                        }
                    } else {
                        // Unknown method - send error response
                        let response_payload = vec![0x01, 0x01]; // Result::Err, RoamError::UnknownMethod
                        io.send(&Message::Response {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            request_id,
                            metadata: metadata_empty(),
                            channels: vec![],
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send error response: {e}"))?;
                    }
                }

                Message::Data {
                    channel_id,
                    payload,
                    ..
                } => {
                    // Accumulate data for the channel
                    if let Some(data) = channels.get_mut(&channel_id) {
                        let value: i32 = facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("decode channel data: {e}"))?;
                        data.push(value);
                    }
                }

                Message::Close { channel_id, .. } => {
                    // Channel closed - if this was a sum request, send the response
                    if let Some((request_id, sum_channel_id)) = pending_sum.take()
                        && sum_channel_id == channel_id
                    {
                        let data = channels.remove(&channel_id).unwrap_or_default();
                        let sum: i64 = data.iter().map(|&x| x as i64).sum();
                        let response_payload = encode_ok(&sum)?;
                        io.send(&Message::Response {
                            conn_id: roam_wire::ConnectionId::ROOT,
                            request_id,
                            metadata: metadata_empty(),
                            channels: vec![],
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send sum response: {e}"))?;
                    }
                }

                Message::Goodbye { .. } => break,

                _ => {
                    // Ignore other messages
                }
            }
        }

        if matches!(scenario, "shape_area" | "create_canvas" | "process_message")
            && !scenario_satisfied
        {
            return Err(format!(
                "scenario '{scenario}' was not exercised by subject client"
            ));
        }

        Ok(())
    }
}
