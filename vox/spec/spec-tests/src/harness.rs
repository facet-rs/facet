use std::process::Stdio;
use std::time::Duration;

use cobs::{decode_vec as cobs_decode_vec, encode_vec as cobs_encode_vec};
use rapace_wire::{Hello, Message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};

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
        _ => panic!(
            "SUBJECT_CMD is not set. Example:\n  SUBJECT_CMD=./target/release/subject-rust cargo nextest run -p spec-tests --release"
        ),
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
        initial_stream_credit: 64 * 1024,
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
