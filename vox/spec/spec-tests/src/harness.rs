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

/// Accept a client connection and run as a server with the given dispatcher.
///
/// Returns when the client disconnects or errors.
pub async fn run_as_server<D: roam::session::ServiceDispatcher>(
    dispatcher: D,
    scenario: &str,
) -> Result<(), String> {
    use roam_stream::{CobsFramed as StreamCobsFramed, establish_acceptor};

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

    let io = StreamCobsFramed::new(stream);
    let hello = our_hello(1024 * 1024);

    let (_handle, driver) = tokio::time::timeout(
        Duration::from_secs(5),
        establish_acceptor(io, hello, dispatcher),
    )
    .await
    .map_err(|_| "handshake timed out after 5s".to_string())?
    .map_err(|e| format!("handshake failed: {e:?}"))?;

    // Run the driver until completion
    let result = driver.run().await;

    // Wait for child to exit
    let status = child.wait().await.map_err(|e| format!("wait: {e}"))?;
    if !status.success() {
        return Err(format!("subject exited with {status}"));
    }

    result.map_err(|e| format!("driver error: {e:?}"))
}
