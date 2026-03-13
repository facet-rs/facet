//! Rust subject binary for the roam compliance suite.

use roam::{Rx, Tx};
use roam_core::{TransportMode, initiator, initiator_on};
use roam_shm::bootstrap::{BootstrapStatus, encode_request};
use roam_shm::segment::Segment;
use roam_stream::tcp_connector;
use spec_proto::{
    Canvas, Color, LookupError, MathError, Message, Person, Point, Rectangle, Shape, Testbed,
    TestbedClient, TestbedDispatcher,
};
use tracing::{debug, error, info, instrument};

#[cfg(windows)]
use roam_shm::guest_link_from_names;
#[cfg(unix)]
use roam_shm::guest_link_from_raw;
#[cfg(unix)]
use std::os::fd::AsRawFd;

#[derive(Clone)]
struct TestbedService;

impl Testbed for TestbedService {
    #[instrument(skip(self))]
    async fn echo(&self, message: String) -> String {
        info!("echo called");
        message
    }

    #[instrument(skip(self))]
    async fn reverse(&self, message: String) -> String {
        info!("reverse called");
        message.chars().rev().collect()
    }

    #[instrument(skip(self))]
    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError> {
        info!("divide called");
        if divisor == 0 {
            Err(MathError::DivisionByZero)
        } else {
            Ok(dividend / divisor)
        }
    }

    #[instrument(skip(self))]
    async fn lookup(&self, id: u32) -> Result<Person, LookupError> {
        info!("lookup called");
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

    #[instrument(skip(self, numbers))]
    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        info!("sum called");
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            debug!(n = *n, total, "received number");
            total += *n as i64;
        }
        info!(total, "sum complete");
        total
    }

    #[instrument(skip(self, output))]
    async fn generate(&self, count: u32, output: Tx<i32>) {
        info!(count, "generate called");
        for i in 0..count as i32 {
            debug!(i, "sending value");
            if let Err(e) = output.send(i).await {
                error!(i, ?e, "send failed");
                break;
            }
        }
        output.close(Default::default()).await.ok();
    }

    #[instrument(skip(self, input, output))]
    async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
        info!("transform called");
        while let Ok(Some(s)) = input.recv().await {
            debug!(s = ?*s, "transforming");
            let _ = output.send(s.clone()).await;
        }
        output.close(Default::default()).await.ok();
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
            Message::Text(s) => Message::Text(format!("processed: {s}")),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(d) => Message::Data(d.into_iter().rev().collect()),
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

fn requested_transport_mode() -> TransportMode {
    match std::env::var("SPEC_CONDUIT").ok().as_deref() {
        Some("stable") => TransportMode::Stable,
        _ => TransportMode::Bare,
    }
}

fn main() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    let mode = std::env::var("SUBJECT_MODE").unwrap_or_else(|_| "server".to_string());
    match mode.as_str() {
        "server" => rt.block_on(connect_and_serve()),
        "shm-server" => rt.block_on(connect_and_serve_shm()),
        other => Err(format!("unknown SUBJECT_MODE: {other}")),
    }
}

async fn connect_and_serve() -> Result<(), String> {
    let addr = std::env::var("PEER_ADDR").map_err(|_| "PEER_ADDR env var not set".to_string())?;
    info!("connecting to {addr}");

    let (root_caller_guard, _sh) = initiator(tcp_connector(addr), requested_transport_mode())
        .establish::<TestbedClient>(TestbedDispatcher::new(TestbedService))
        .await
        .map_err(|e| format!("handshake failed: {e}"))?;

    let _root_caller_guard = root_caller_guard;
    std::future::pending::<()>().await;
    Ok(())
}

async fn connect_and_serve_shm() -> Result<(), String> {
    let control_sock = std::env::var("SHM_CONTROL_SOCK")
        .map_err(|_| "SHM_CONTROL_SOCK env var not set".to_string())?;
    let sid = std::env::var("SHM_SESSION_ID")
        .map_err(|_| "SHM_SESSION_ID env var not set".to_string())?;

    let request = encode_request(sid.as_bytes()).map_err(|e| format!("encode request: {e}"))?;

    // Connect to the control socket, send the bootstrap request, and receive the
    // bootstrap response. The response carries fds on Unix and names on Windows.
    #[cfg(unix)]
    let link = {
        let mmap_tx_fd: i32 = std::env::var("SHM_MMAP_TX_FD")
            .map_err(|_| "SHM_MMAP_TX_FD env var not set".to_string())?
            .parse()
            .map_err(|e| format!("invalid SHM_MMAP_TX_FD: {e}"))?;

        let mut stream = std::os::unix::net::UnixStream::connect(&control_sock)
            .map_err(|e| format!("connect bootstrap socket: {e}"))?;
        std::io::Write::write_all(&mut stream, &request)
            .map_err(|e| format!("send bootstrap request: {e}"))?;

        let received = shm_primitives::bootstrap::recv_response_unix(stream.as_raw_fd())
            .map_err(|e| format!("recv bootstrap response: {e}"))?;
        if received.response.status != BootstrapStatus::Success {
            return Err(format!(
                "bootstrap failed: status={:?}, payload={}",
                received.response.status,
                String::from_utf8_lossy(&received.response.payload)
            ));
        }
        let fds = received
            .fds
            .ok_or_else(|| "missing bootstrap success fds".to_string())?;
        let hub_path = std::str::from_utf8(&received.response.payload)
            .map_err(|e| format!("bootstrap payload is not utf-8 path: {e}"))?;
        let segment = std::sync::Arc::new(
            Segment::attach(std::path::Path::new(hub_path))
                .map_err(|e| format!("attach segment at {hub_path}: {e}"))?,
        );
        let peer_id = shm_primitives::PeerId::new(received.response.peer_id as u8)
            .ok_or_else(|| format!("invalid peer id {}", received.response.peer_id))?;

        use std::os::fd::IntoRawFd;
        let doorbell_fd = fds.doorbell_fd.into_raw_fd();
        let mmap_rx_fd = fds.mmap_control_fd.into_raw_fd();

        unsafe { guest_link_from_raw(segment, peer_id, doorbell_fd, mmap_rx_fd, mmap_tx_fd, true) }
            .map_err(|e| format!("guest_link_from_raw: {e}"))?
    };

    #[cfg(windows)]
    let link = {
        use roam_shm::bootstrap::{
            BOOTSTRAP_RESPONSE_HEADER_LEN, BootstrapSuccessNames, decode_response,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mmap_tx_pipe = std::env::var("SHM_MMAP_TX_PIPE")
            .map_err(|_| "SHM_MMAP_TX_PIPE env var not set".to_string())?;

        // On Windows, SHM_CONTROL_SOCK is a named pipe path.
        let mut stream = roam_local::connect(&control_sock)
            .await
            .map_err(|e| format!("connect bootstrap pipe: {e}"))?;
        stream
            .write_all(&request)
            .await
            .map_err(|e| format!("send bootstrap request: {e}"))?;

        // Read the bootstrap response header.
        let mut header = [0u8; BOOTSTRAP_RESPONSE_HEADER_LEN];
        stream
            .read_exact(&mut header)
            .await
            .map_err(|e| format!("read bootstrap response header: {e}"))?;

        // Parse payload length from header (bytes 9-10 = payload_len as u16 LE).
        let payload_len = u16::from_le_bytes([header[9], header[10]]) as usize;
        let mut payload = vec![0u8; payload_len];
        if payload_len > 0 {
            stream
                .read_exact(&mut payload)
                .await
                .map_err(|e| format!("read bootstrap response payload: {e}"))?;
        }

        // Combine into full frame for decode_response.
        let mut frame = Vec::with_capacity(BOOTSTRAP_RESPONSE_HEADER_LEN + payload_len);
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&payload);
        let response_ref =
            decode_response(&frame).map_err(|e| format!("decode bootstrap response: {e}"))?;

        if response_ref.status != BootstrapStatus::Success {
            return Err(format!(
                "bootstrap failed: status={:?}, payload={}",
                response_ref.status,
                String::from_utf8_lossy(response_ref.payload)
            ));
        }

        let names = BootstrapSuccessNames::decode(response_ref.payload)
            .map_err(|e| format!("decode bootstrap names: {e}"))?;
        let segment = std::sync::Arc::new(
            Segment::attach(std::path::Path::new(&names.segment_path))
                .map_err(|e| format!("attach segment at {}: {e}", names.segment_path))?,
        );
        let peer_id = shm_primitives::PeerId::new(response_ref.peer_id as u8)
            .ok_or_else(|| format!("invalid peer id {}", response_ref.peer_id))?;

        guest_link_from_names(
            segment,
            peer_id,
            &names.doorbell_name,
            &names.mmap_ctrl_name,
            &mmap_tx_pipe,
            true,
        )
        .map_err(|e| format!("guest_link_from_names: {e}"))?
    };

    let (root_caller_guard, _sh) = initiator_on(link, requested_transport_mode())
        .establish::<TestbedClient>(TestbedDispatcher::new(TestbedService))
        .await
        .map_err(|e| format!("handshake failed: {e}"))?;

    let _root_caller_guard = root_caller_guard;
    // Session and driver are spawned internally by establish(); wait forever
    // so the spawned tasks can continue serving requests.
    std::future::pending::<()>().await;
    Ok(())
}
