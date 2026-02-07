//! In-process, high-volume RPC benchmark for roam-session.
//!
//! This models VFS-like traffic:
//! - frequent `read(item_id, offset, len)` calls returning large `Vec<u8>` blobs
//! - interleaved `get_attributes(item_id)` calls
//!
//! Usage:
//! - `cargo run -p roam-session --example in_process_bench -- --iterations 200000`
//! - `cargo samply -p roam-session --example in_process_bench -- --iterations 200000`

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use roam::service;
use roam_session::{
    HandshakeConfig, MessageTransport, NoDispatcher, accept_framed, initiate_framed,
};
use roam_wire::Message;
use tokio::sync::mpsc;

#[derive(Debug, Clone, facet::Facet)]
struct ItemAttributes {
    size: u64,
    modified_time: u64,
    created_time: u64,
    mode: u32,
}

#[derive(Debug, Clone, facet::Facet)]
struct ReadResult {
    data: Vec<u8>,
    error: i32,
}

#[derive(Debug, Clone, facet::Facet)]
struct GetAttributesResult {
    attrs: ItemAttributes,
    error: i32,
}

#[service]
trait BenchVfs {
    async fn get_attributes(&self, item_id: u64) -> GetAttributesResult;
    async fn read(&self, item_id: u64, offset: u64, len: u64) -> ReadResult;
}

struct InMemoryTransport {
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    last_decoded: Vec<u8>,
}

fn in_memory_transport_pair(buffer: usize) -> (InMemoryTransport, InMemoryTransport) {
    let (a_to_b_tx, a_to_b_rx) = mpsc::channel(buffer);
    let (b_to_a_tx, b_to_a_rx) = mpsc::channel(buffer);

    let a = InMemoryTransport {
        tx: a_to_b_tx,
        rx: b_to_a_rx,
        last_decoded: Vec::new(),
    };
    let b = InMemoryTransport {
        tx: b_to_a_tx,
        rx: a_to_b_rx,
        last_decoded: Vec::new(),
    };

    (a, b)
}

impl MessageTransport for InMemoryTransport {
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        self.tx
            .send(msg.clone())
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "peer disconnected"))
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        match tokio::time::timeout(timeout, self.rx.recv()).await {
            Ok(msg) => Ok(msg),
            Err(_) => Ok(None),
        }
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        Ok(self.rx.recv().await)
    }

    fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

#[derive(Clone)]
struct BenchService {
    file_a: Arc<[u8]>,
    file_b: Arc<[u8]>,
}

impl BenchService {
    fn new(file_size: usize) -> Self {
        let mut file_a = vec![0u8; file_size];
        let mut file_b = vec![0u8; file_size];
        for (i, b) in file_a.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(31).wrapping_add(7);
        }
        for (i, b) in file_b.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(17).wrapping_add(13);
        }
        Self {
            file_a: Arc::from(file_a),
            file_b: Arc::from(file_b),
        }
    }

    fn file_for(&self, item_id: u64) -> &[u8] {
        match item_id {
            396 => &self.file_a,
            _ => &self.file_b,
        }
    }
}

impl BenchVfs for BenchService {
    async fn get_attributes(&self, _cx: &roam::Context, item_id: u64) -> GetAttributesResult {
        let file = self.file_for(item_id);
        GetAttributesResult {
            attrs: ItemAttributes {
                size: file.len() as u64,
                modified_time: 0,
                created_time: 0,
                mode: 0o644,
            },
            error: 0,
        }
    }

    async fn read(&self, _cx: &roam::Context, item_id: u64, offset: u64, len: u64) -> ReadResult {
        let file = self.file_for(item_id);
        let start = offset as usize;
        let end = (start.saturating_add(len as usize)).min(file.len());
        if start >= file.len() {
            return ReadResult {
                data: Vec::new(),
                error: 22,
            };
        }
        ReadResult {
            data: file[start..end].to_vec(),
            error: 0,
        }
    }
}

#[derive(Clone, Copy)]
enum Op {
    GetAttributes { item_id: u64 },
    Read { item_id: u64, offset: u64, len: u64 },
}

struct Config {
    iterations: usize,
    warmup: usize,
    file_size: usize,
}

fn parse_args() -> Result<Config, String> {
    let mut iterations = 120_000usize;
    let mut warmup = 2_000usize;
    let mut file_size = 32 * 1024 * 1024usize;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "--iterations needs a value".to_string())?;
                iterations = raw
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --iterations: {e}"))?;
            }
            "--warmup" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "--warmup needs a value".to_string())?;
                warmup = raw
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --warmup: {e}"))?;
            }
            "--file-size" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "--file-size needs a value".to_string())?;
                file_size = raw
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --file-size: {e}"))?;
            }
            _ => {
                return Err(format!(
                    "unknown arg: {arg}. expected --iterations N --warmup N --file-size N"
                ));
            }
        }
    }

    Ok(Config {
        iterations,
        warmup,
        file_size,
    })
}

fn build_ops(count: usize, file_size: usize) -> Vec<Op> {
    let lens: [u64; 11] = [
        16 * 1024,
        32 * 1024,
        48 * 1024,
        64 * 1024,
        80 * 1024,
        160 * 1024,
        240 * 1024,
        320 * 1024,
        384 * 1024,
        512 * 1024,
        896 * 1024,
    ];
    let items = [396u64, 419u64];

    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut ops = Vec::with_capacity(count);
    for i in 0..count {
        seed ^= seed << 7;
        seed ^= seed >> 9;
        seed ^= seed << 8;

        let item_id = items[(seed as usize) & 1];
        if i % 4 == 0 {
            ops.push(Op::GetAttributes { item_id });
            continue;
        }

        let len = lens[(seed as usize) % lens.len()];
        let max_offset = file_size.saturating_sub(len as usize).max(1);
        let offset = (seed as usize % max_offset) as u64;
        ops.push(Op::Read {
            item_id,
            offset,
            len,
        });
    }

    ops
}

async fn run(config: Config) -> Result<(), String> {
    let service = BenchService::new(config.file_size);
    let ops = build_ops(config.warmup + config.iterations, config.file_size);

    let (client_transport, server_transport) = in_memory_transport_pair(8192);
    let dispatcher = BenchVfsDispatcher::new(service);

    let client_fut = initiate_framed(client_transport, HandshakeConfig::default(), NoDispatcher);
    let server_fut = accept_framed(server_transport, HandshakeConfig::default(), dispatcher);

    let (client_setup, server_setup) = tokio::try_join!(client_fut, server_fut)
        .map_err(|e| format!("failed to establish in-memory connection: {e}"))?;

    let (client_handle, _incoming_client, client_driver) = client_setup;
    let (_server_handle, _incoming_server, server_driver) = server_setup;

    let client_driver_task = tokio::spawn(async move { client_driver.run().await });
    let server_driver_task = tokio::spawn(async move { server_driver.run().await });

    let client = BenchVfsClient::new(client_handle);

    let mut checksum = 0u64;

    for op in &ops[..config.warmup] {
        match *op {
            Op::GetAttributes { item_id } => {
                let result = client
                    .get_attributes(item_id)
                    .await
                    .map_err(|e| format!("warmup get_attributes failed: {e}"))?;
                checksum ^= result.attrs.size;
            }
            Op::Read {
                item_id,
                offset,
                len,
            } => {
                let result = client
                    .read(item_id, offset, len)
                    .await
                    .map_err(|e| format!("warmup read failed: {e}"))?;
                if result.error == 0 {
                    checksum ^= result.data.len() as u64;
                    if let Some(first) = result.data.first() {
                        checksum ^= *first as u64;
                    }
                }
            }
        }
    }

    let started = Instant::now();
    let mut bytes_read = 0u64;
    for op in &ops[config.warmup..] {
        match *op {
            Op::GetAttributes { item_id } => {
                let result = client
                    .get_attributes(item_id)
                    .await
                    .map_err(|e| format!("benchmark get_attributes failed: {e}"))?;
                checksum ^= result.attrs.mode as u64;
            }
            Op::Read {
                item_id,
                offset,
                len,
            } => {
                let result = client
                    .read(item_id, offset, len)
                    .await
                    .map_err(|e| format!("benchmark read failed: {e}"))?;
                if result.error != 0 {
                    return Err(format!("read returned error={}", result.error));
                }
                bytes_read += result.data.len() as u64;
                checksum ^= result.data.len() as u64;
            }
        }
    }
    let elapsed = started.elapsed();

    let seconds = elapsed.as_secs_f64();
    let calls_per_sec = config.iterations as f64 / seconds;
    let mib_per_sec = (bytes_read as f64 / (1024.0 * 1024.0)) / seconds;
    let us_per_call = elapsed.as_micros() as f64 / config.iterations as f64;

    println!(
        "bench complete: iterations={} warmup={} file_size={} elapsed={:.3}s calls_per_sec={:.0} avg_us_per_call={:.2} read_mib_per_sec={:.1} checksum={}",
        config.iterations,
        config.warmup,
        config.file_size,
        seconds,
        calls_per_sec,
        us_per_call,
        mib_per_sec,
        checksum
    );

    client_driver_task.abort();
    server_driver_task.abort();
    Ok(())
}

fn main() -> Result<(), String> {
    let config = parse_args()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    rt.block_on(run(config))
}
