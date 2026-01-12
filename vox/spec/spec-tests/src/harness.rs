use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cobs::{decode_vec as cobs_decode_vec, encode_vec as cobs_encode_vec};
use roam_wire::{Hello, Message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};

/// Enable wire-level message logging for debugging.
/// Set ROAM_WIRE_SPY=1 to enable.
static WIRE_SPY_ENABLED: AtomicBool = AtomicBool::new(false);

#[ctor::ctor]
fn init_wire_spy() {
    if std::env::var("ROAM_WIRE_SPY").is_ok() {
        WIRE_SPY_ENABLED.store(true, Ordering::Relaxed);
    }
}

fn wire_spy_enabled() -> bool {
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
        },
        Message::Goodbye { reason } => format!("{direction} Goodbye {{ reason: {reason:?} }}"),
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
        Message::Cancel { request_id } => format!("{direction} Cancel {{ id: {request_id} }}"),
        Message::Data {
            channel_id,
            payload,
        } => format!(
            "{direction} Data {{ stream: {channel_id}, payload: {} bytes }}",
            payload.len()
        ),
        Message::Close { channel_id } => format!("{direction} Close {{ stream: {channel_id} }}"),
        Message::Reset { channel_id } => format!("{direction} Reset {{ stream: {channel_id} }}"),
        Message::Credit { channel_id, bytes } => {
            format!("{direction} Credit {{ stream: {channel_id}, bytes: {bytes} }}")
        }
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
    Hello::V1 {
        max_payload_size,
        initial_channel_credit: 64 * 1024,
    }
}

pub struct CobsFramed {
    pub stream: TcpStream,
    buf: Vec<u8>,
}

impl CobsFramed {
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
        let mut framed = cobs_encode_vec(&payload);
        framed.push(0x00);
        self.stream.write_all(&framed).await?;
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
            if let Some(idx) = self.buf.iter().position(|b| *b == 0x00) {
                let frame = self.buf.drain(..idx).collect::<Vec<_>>();
                self.buf.drain(..1); // delimiter

                let decoded = cobs_decode_vec(&frame).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("cobs: {e}"))
                })?;

                let msg: Message = facet_postcard::from_slice(&decoded).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("postcard: {e}"))
                })?;
                if wire_spy_enabled() {
                    eprintln!("[WIRE] {}", format_message(&msg, "<--"));
                }
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

pub async fn accept_subject() -> Result<(CobsFramed, Child), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    let child = spawn_subject(&addr.to_string()).await?;

    let (stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .map_err(|_| "subject did not connect within 5s".to_string())?
        .map_err(|e| format!("accept: {e}"))?;

    Ok((CobsFramed::new(stream), child))
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

/// Wire-level server for client mode tests.
///
/// This implements a minimal Testbed server at the wire level, without using
/// any roam runtime types. This ensures we're testing the wire protocol, not
/// roam-against-roam.
pub mod wire_server {
    use super::*;
    use roam_wire::MetadataValue;
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

        let mut io = CobsFramed::new(stream);

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
            Message::Hello(Hello::V1 { .. }) => {}
            other => return Err(format!("expected Hello, got {other:?}")),
        }

        // Handle requests until client disconnects
        let result = handle_requests(&mut io, method_ids).await;

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
    }

    fn metadata_empty() -> Vec<(String, MetadataValue)> {
        Vec::new()
    }

    /// Encode a successful result: Result::Ok(value)
    fn encode_ok<T: for<'a> facet::Facet<'a>>(value: &T) -> Result<Vec<u8>, String> {
        let mut result = vec![0x00]; // Result::Ok variant
        result
            .extend(facet_postcard::to_vec(value).map_err(|e| format!("encode ok payload: {e}"))?);
        Ok(result)
    }

    async fn handle_requests(io: &mut CobsFramed, method_ids: &MethodIds) -> Result<(), String> {
        // Track open channels: channel_id -> accumulated data
        let mut channels: HashMap<u64, Vec<i32>> = HashMap::new();
        // Track pending requests that are waiting for channel data
        let mut pending_sum: Option<(u64, u64)> = None; // (request_id, channel_id)

        loop {
            let msg = match io.recv_timeout(Duration::from_secs(5)).await {
                Ok(Some(m)) => m,
                Ok(None) => break, // Client disconnected
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
                            request_id,
                            metadata: metadata_empty(),
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
                            request_id,
                            metadata: metadata_empty(),
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
                                channel_id,
                                payload: data_payload,
                            })
                            .await
                            .map_err(|e| format!("send data: {e}"))?;
                        }

                        // Send Close
                        io.send(&Message::Close { channel_id })
                            .await
                            .map_err(|e| format!("send close: {e}"))?;

                        // Send Response
                        let response_payload = encode_ok(&())?;
                        io.send(&Message::Response {
                            request_id,
                            metadata: metadata_empty(),
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send response: {e}"))?;
                    } else if method_id == method_ids.transform {
                        // transform(input: Rx<String>, output: Tx<String>)
                        // This is more complex - we'll handle it if needed
                        return Err("transform not yet implemented in wire server".to_string());
                    } else {
                        // Unknown method - send error response
                        let response_payload = vec![0x01, 0x01]; // Result::Err, RoamError::UnknownMethod
                        io.send(&Message::Response {
                            request_id,
                            metadata: metadata_empty(),
                            payload: response_payload,
                        })
                        .await
                        .map_err(|e| format!("send error response: {e}"))?;
                    }
                }

                Message::Data {
                    channel_id,
                    payload,
                } => {
                    // Accumulate data for the channel
                    if let Some(data) = channels.get_mut(&channel_id) {
                        let value: i32 = facet_postcard::from_slice(&payload)
                            .map_err(|e| format!("decode channel data: {e}"))?;
                        data.push(value);
                    }
                }

                Message::Close { channel_id } => {
                    // Channel closed - if this was a sum request, send the response
                    if let Some((request_id, sum_channel_id)) = pending_sum.take()
                        && sum_channel_id == channel_id
                    {
                        let data = channels.remove(&channel_id).unwrap_or_default();
                        let sum: i64 = data.iter().map(|&x| x as i64).sum();
                        let response_payload = encode_ok(&sum)?;
                        io.send(&Message::Response {
                            request_id,
                            metadata: metadata_empty(),
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

        Ok(())
    }
}
