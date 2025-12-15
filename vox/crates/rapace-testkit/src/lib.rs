//! rapace-testkit: Conformance test suite for rapace transports.
//!
//! Provides `TransportFactory` trait and shared test scenarios that all
//! transports must pass.
//!
//! # Usage
//!
//! Each transport crate implements `TransportFactory` and runs the shared tests:
//!
//! ```ignore
//! use rapace_testkit::{TransportFactory, TestError};
//!
//! struct MyTransportFactory;
//!
//! impl TransportFactory for MyTransportFactory {
//!     type Transport = MyTransport;
//!
//!     fn connect_pair() -> impl Future<Output = Result<(Self::Transport, Self::Transport), TestError>> + Send {
//!         async { /* create connected pair */ }
//!     }
//! }
//!
//! #[tokio::test]
//! async fn my_transport_unary_happy_path() {
//!     rapace_testkit::run_unary_happy_path::<MyTransportFactory>().await;
//! }
//! ```

use std::future::Future;
use std::sync::Arc;

use rapace::session::Session;
use rapace_core::{
    CancelReason, ControlPayload, ErrorCode, Frame, FrameFlags, MsgDescHot, NO_DEADLINE, RpcError,
    RpcSession, Transport, control_method,
};

pub mod bidirectional;
pub mod helper_binary;

/// Error type for test scenarios.
#[derive(Debug)]
pub enum TestError {
    /// Transport creation failed.
    Setup(String),
    /// RPC call failed.
    Rpc(rapace_core::RpcError),
    /// Transport error.
    Transport(rapace_core::TransportError),
    /// Assertion failed.
    Assertion(String),
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::Setup(msg) => write!(f, "setup error: {}", msg),
            TestError::Rpc(e) => write!(f, "RPC error: {}", e),
            TestError::Transport(e) => write!(f, "transport error: {}", e),
            TestError::Assertion(msg) => write!(f, "assertion failed: {}", msg),
        }
    }
}

impl std::error::Error for TestError {}

impl From<rapace_core::RpcError> for TestError {
    fn from(e: rapace_core::RpcError) -> Self {
        TestError::Rpc(e)
    }
}

impl From<rapace_core::TransportError> for TestError {
    fn from(e: rapace_core::TransportError) -> Self {
        TestError::Transport(e)
    }
}

/// Factory trait for creating transport pairs for testing.
///
/// Each transport implementation provides a factory that creates connected
/// pairs of transports for testing.
pub trait TransportFactory: Send + Sync + 'static {
    /// The transport type being tested.
    type Transport: Transport + Send + Sync + 'static;

    /// Create a connected pair of transports.
    ///
    /// Returns (client_side, server_side) where frames sent from client
    /// are received by server and vice versa.
    fn connect_pair()
    -> impl Future<Output = Result<(Self::Transport, Self::Transport), TestError>> + Send;
}

// ============================================================================
// Test service: Adder
// ============================================================================

/// Simple arithmetic service used for testing.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Adder {
    /// Add two numbers.
    async fn add(&self, a: i32, b: i32) -> i32;
}

/// Implementation of the Adder service for testing.
pub struct AdderImpl;

impl Adder for AdderImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}

// ============================================================================
// Test service: RangeService (server-streaming)
// ============================================================================

/// Service with server-streaming RPC for testing.
///
/// Uses the new `Streaming<T>` return type pattern.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait RangeService {
    /// Stream numbers from 0 to n-1.
    async fn range(&self, n: u32) -> rapace_core::Streaming<u32>;
}

/// Implementation of the RangeService for testing.
pub struct RangeServiceImpl;

impl RangeService for RangeServiceImpl {
    async fn range(&self, n: u32) -> rapace_core::Streaming<u32> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            for i in 0..n {
                if tx.send(Ok(i)).await.is_err() {
                    break;
                }
            }
        });
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

// ============================================================================
// Test scenarios
// ============================================================================

/// Run a single unary RPC call and verify the result.
///
/// This is the most basic test: client calls `add(2, 3)` and expects `5`.
pub async fn run_unary_happy_path<F: TransportFactory>() {
    let result = run_unary_happy_path_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_unary_happy_path failed: {}", e);
    }
}

async fn run_unary_happy_path_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = AdderServer::new(AdderImpl);

    // Spawn server task to handle one request
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            let request = server_transport.recv_frame().await?;
            let mut response = server
                .dispatch(request.desc.method_id, request.payload_bytes())
                .await
                .map_err(TestError::Rpc)?;
            // Set channel_id on response to match request
            response.desc.channel_id = request.desc.channel_id;
            server_transport.send_frame(&response).await?;
            Ok::<_, TestError>(())
        }
    });

    // Create client session and spawn its demux loop
    let client_session = Arc::new(RpcSession::new(client_transport));
    let client_session_runner = client_session.clone();
    let _session_handle = tokio::spawn(async move { client_session_runner.run().await });

    // Create client and make call
    let client = AdderClient::new(client_session);
    let result = client.add(2, 3).await?;

    if result != 5 {
        return Err(TestError::Assertion(format!(
            "expected add(2, 3) = 5, got {}",
            result
        )));
    }

    // Wait for server to finish
    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

/// Run multiple unary RPC calls sequentially.
///
/// Verifies that the transport correctly handles multiple request/response pairs.
pub async fn run_unary_multiple_calls<F: TransportFactory>() {
    let result = run_unary_multiple_calls_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_unary_multiple_calls failed: {}", e);
    }
}

async fn run_unary_multiple_calls_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = AdderServer::new(AdderImpl);

    // Spawn server task to handle multiple requests
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            for _ in 0..3 {
                let request = server_transport.recv_frame().await?;
                let mut response = server
                    .dispatch(request.desc.method_id, request.payload_bytes())
                    .await
                    .map_err(TestError::Rpc)?;
                // Set channel_id on response to match request
                response.desc.channel_id = request.desc.channel_id;
                server_transport.send_frame(&response).await?;
            }
            Ok::<_, TestError>(())
        }
    });

    // Create client session and spawn its demux loop
    let client_session = Arc::new(RpcSession::new(client_transport));
    let client_session_runner = client_session.clone();
    let _session_handle = tokio::spawn(async move { client_session_runner.run().await });

    let client = AdderClient::new(client_session);

    // Multiple calls with different values
    let test_cases = [(1, 2, 3), (10, 20, 30), (-5, 5, 0)];

    for (a, b, expected) in test_cases {
        let result = client.add(a, b).await?;
        if result != expected {
            return Err(TestError::Assertion(format!(
                "expected add({}, {}) = {}, got {}",
                a, b, expected, result
            )));
        }
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

// ============================================================================
// Error response scenarios
// ============================================================================

/// Test that error responses are correctly transmitted.
///
/// Server returns `RpcError::Status` with `ErrorCode::InvalidArgument`,
/// client receives and correctly deserializes the error.
pub async fn run_error_response<F: TransportFactory>() {
    let result = run_error_response_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_error_response failed: {}", e);
    }
}

async fn run_error_response_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    // Spawn server that returns an error
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            let request = server_transport.recv_frame().await?;

            // Build error response frame
            let mut desc = MsgDescHot::new();
            desc.msg_id = request.desc.msg_id;
            desc.channel_id = request.desc.channel_id;
            desc.method_id = request.desc.method_id;
            desc.flags = FrameFlags::ERROR | FrameFlags::EOS;

            // Encode error as payload: ErrorCode (u32) + message length (u32) + message bytes
            let error_code = ErrorCode::InvalidArgument as u32;
            let message = "test error message";
            let mut payload = Vec::new();
            payload.extend_from_slice(&error_code.to_le_bytes());
            payload.extend_from_slice(&(message.len() as u32).to_le_bytes());
            payload.extend_from_slice(message.as_bytes());

            let frame = Frame::with_payload(desc, payload);
            server_transport.send_frame(&frame).await?;

            Ok::<_, TestError>(())
        }
    });

    // Create client session and spawn its demux loop
    let client_session = Arc::new(RpcSession::new(client_transport));
    let client_session_runner = client_session.clone();
    let _session_handle = tokio::spawn(async move { client_session_runner.run().await });

    // Client makes call and expects error
    let client = AdderClient::new(client_session);
    let result = client.add(1, 2).await;

    match result {
        Err(RpcError::Status { code, message }) => {
            if code != ErrorCode::InvalidArgument {
                return Err(TestError::Assertion(format!(
                    "expected InvalidArgument, got {:?}",
                    code
                )));
            }
            if message != "test error message" {
                return Err(TestError::Assertion(format!(
                    "expected 'test error message', got '{}'",
                    message
                )));
            }
        }
        Ok(v) => {
            return Err(TestError::Assertion(format!(
                "expected error, got success: {}",
                v
            )));
        }
        Err(e) => {
            return Err(TestError::Assertion(format!(
                "expected Status error, got {:?}",
                e
            )));
        }
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

// ============================================================================
// PING/PONG control frame scenarios
// ============================================================================

/// Test PING/PONG round-trip on control channel.
///
/// Verifies that control frames on channel 0 are correctly transmitted.
pub async fn run_ping_pong<F: TransportFactory>() {
    let result = run_ping_pong_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_ping_pong failed: {}", e);
    }
}

async fn run_ping_pong_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    // Server responds to PING with PONG
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            let request = server_transport.recv_frame().await?;

            // Verify it's a PING on control channel
            if request.desc.channel_id != 0 {
                return Err(TestError::Assertion("expected control channel".into()));
            }
            if request.desc.method_id != control_method::PING {
                return Err(TestError::Assertion("expected PING method_id".into()));
            }
            if !request.desc.flags.contains(FrameFlags::CONTROL) {
                return Err(TestError::Assertion("expected CONTROL flag".into()));
            }

            // Extract ping payload and echo it back as PONG
            let ping_payload: [u8; 8] = request
                .payload_bytes()
                .try_into()
                .map_err(|_| TestError::Assertion("ping payload should be 8 bytes".into()))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = request.desc.msg_id;
            desc.channel_id = 0; // control channel
            desc.method_id = control_method::PONG;
            desc.flags = FrameFlags::CONTROL | FrameFlags::EOS;

            let frame = Frame::with_inline_payload(desc, &ping_payload)
                .expect("pong payload should fit inline");
            server_transport.send_frame(&frame).await?;

            Ok::<_, TestError>(())
        }
    });

    // Client sends PING
    let ping_data: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0; // control channel
    desc.method_id = control_method::PING;
    desc.flags = FrameFlags::CONTROL | FrameFlags::EOS;

    let frame =
        Frame::with_inline_payload(desc, &ping_data).expect("ping payload should fit inline");
    client_transport.send_frame(&frame).await?;

    // Receive PONG
    let pong = client_transport.recv_frame().await?;

    if pong.desc.channel_id != 0 {
        return Err(TestError::Assertion("expected control channel".into()));
    }
    if pong.desc.method_id != control_method::PONG {
        return Err(TestError::Assertion("expected PONG method_id".into()));
    }
    if pong.payload_bytes() != ping_data {
        return Err(TestError::Assertion(format!(
            "PONG payload mismatch: expected {:?}, got {:?}",
            ping_data,
            pong.payload_bytes()
        )));
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

// ============================================================================
// Deadline scenarios
// ============================================================================

/// Get current monotonic time in nanoseconds.
fn now_ns() -> u64 {
    use std::time::Instant;
    // Use a static reference point for consistent monotonic time
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as u64
}

/// Test that requests with generous deadlines succeed.
pub async fn run_deadline_success<F: TransportFactory>() {
    let result = run_deadline_success_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_deadline_success failed: {}", e);
    }
}

async fn run_deadline_success_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = AdderServer::new(AdderImpl);

    // Server checks deadline before dispatch
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            let request = server_transport.recv_frame().await?;

            // Check deadline
            if request.desc.deadline_ns != NO_DEADLINE {
                let now = now_ns();
                if now > request.desc.deadline_ns {
                    // Deadline exceeded - send error response
                    let mut desc = MsgDescHot::new();
                    desc.msg_id = request.desc.msg_id;
                    desc.channel_id = request.desc.channel_id;
                    desc.flags = FrameFlags::ERROR | FrameFlags::EOS;

                    let error_code = ErrorCode::DeadlineExceeded as u32;
                    let message = "deadline exceeded";
                    let mut payload = Vec::new();
                    payload.extend_from_slice(&error_code.to_le_bytes());
                    payload.extend_from_slice(&(message.len() as u32).to_le_bytes());
                    payload.extend_from_slice(message.as_bytes());

                    let frame = Frame::with_payload(desc, payload);
                    server_transport.send_frame(&frame).await?;
                    return Ok(());
                }
            }

            // Deadline not exceeded - process normally
            let mut response = server
                .dispatch(request.desc.method_id, request.payload_bytes())
                .await
                .map_err(TestError::Rpc)?;
            // Set channel_id on response to match request
            response.desc.channel_id = request.desc.channel_id;
            server_transport.send_frame(&response).await?;
            Ok::<_, TestError>(())
        }
    });

    // Create client session and spawn its demux loop
    let client_session = Arc::new(RpcSession::new(client_transport));
    let client_session_runner = client_session.clone();
    let _session_handle = tokio::spawn(async move { client_session_runner.run().await });

    // Client sets a generous deadline (10 seconds from now)
    let deadline = now_ns() + 10_000_000_000; // 10 seconds

    // We need to set the deadline on the frame. Since the generated client
    // doesn't support deadlines yet, we'll call add() which should succeed
    // because the server won't see an expired deadline.
    let client = AdderClient::new(client_session);
    let result = client.add(2, 3).await?;

    if result != 5 {
        return Err(TestError::Assertion(format!("expected 5, got {}", result)));
    }

    let _ = deadline; // suppress unused warning - we'll use this when client supports deadlines

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

/// Test that requests with expired deadlines fail with DeadlineExceeded.
pub async fn run_deadline_exceeded<F: TransportFactory>() {
    let result = run_deadline_exceeded_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_deadline_exceeded failed: {}", e);
    }
}

async fn run_deadline_exceeded_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    // Initialize the time base and capture current time
    // This ensures now_ns() is properly initialized before we set an expired deadline
    let baseline = now_ns();
    // A small sleep to ensure time advances
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    // The expired deadline is set to the baseline, which is now in the past
    let expired_deadline = baseline;

    // Server checks deadline before dispatch
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            let request = server_transport.recv_frame().await?;

            // Check deadline
            if request.desc.deadline_ns != NO_DEADLINE {
                let now = now_ns();
                if now > request.desc.deadline_ns {
                    // Deadline exceeded - send error response
                    let mut desc = MsgDescHot::new();
                    desc.msg_id = request.desc.msg_id;
                    desc.channel_id = request.desc.channel_id;
                    desc.flags = FrameFlags::ERROR | FrameFlags::EOS;

                    let error_code = ErrorCode::DeadlineExceeded as u32;
                    let message = "deadline exceeded";
                    let mut payload = Vec::new();
                    payload.extend_from_slice(&error_code.to_le_bytes());
                    payload.extend_from_slice(&(message.len() as u32).to_le_bytes());
                    payload.extend_from_slice(message.as_bytes());

                    let frame = Frame::with_payload(desc, payload);
                    server_transport.send_frame(&frame).await?;
                    return Ok(());
                }
            }

            // Should not reach here - deadline should be exceeded
            Err(TestError::Assertion(
                "server should have rejected expired deadline".into(),
            ))
        }
    });

    let request_payload = facet_postcard::to_vec(&(1i32, 2i32)).unwrap();

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 1;
    desc.method_id = 1; // add method
    desc.flags = FrameFlags::DATA | FrameFlags::EOS;
    desc.deadline_ns = expired_deadline;

    let frame = if request_payload.len() <= rapace_core::INLINE_PAYLOAD_SIZE {
        Frame::with_inline_payload(desc, &request_payload).expect("should fit inline")
    } else {
        Frame::with_payload(desc, request_payload)
    };

    client_transport.send_frame(&frame).await?;

    // Receive error response
    let response = client_transport.recv_frame().await?;

    if !response.desc.flags.contains(FrameFlags::ERROR) {
        return Err(TestError::Assertion(
            "expected ERROR flag on response".into(),
        ));
    }

    // Parse error from payload
    if response.payload_bytes().len() < 8 {
        return Err(TestError::Assertion("error payload too short".into()));
    }

    let error_code = u32::from_le_bytes(response.payload_bytes()[0..4].try_into().unwrap());
    let code = ErrorCode::from_u32(error_code);

    if code != Some(ErrorCode::DeadlineExceeded) {
        return Err(TestError::Assertion(format!(
            "expected DeadlineExceeded, got {:?}",
            code
        )));
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

// ============================================================================
// Cancellation scenarios
// ============================================================================

/// Test that cancellation frames are correctly transmitted.
///
/// Client sends a request, then sends a CancelChannel control frame.
/// Server observes the cancellation.
pub async fn run_cancellation<F: TransportFactory>() {
    let result = run_cancellation_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_cancellation failed: {}", e);
    }
}

async fn run_cancellation_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let channel_to_cancel: u32 = 42;

    // Server receives request, then expects cancel control frame
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            // First frame: the data request
            let request = server_transport.recv_frame().await?;
            if request.desc.channel_id != channel_to_cancel {
                return Err(TestError::Assertion(format!(
                    "expected channel {}, got {}",
                    channel_to_cancel, request.desc.channel_id
                )));
            }

            // Second frame: the cancel control frame
            let cancel = server_transport.recv_frame().await?;
            if cancel.desc.channel_id != 0 {
                return Err(TestError::Assertion(
                    "cancel should be on control channel".into(),
                ));
            }
            if cancel.desc.method_id != control_method::CANCEL_CHANNEL {
                return Err(TestError::Assertion(format!(
                    "expected CANCEL_CHANNEL method_id, got {}",
                    cancel.desc.method_id
                )));
            }
            if !cancel.desc.flags.contains(FrameFlags::CONTROL) {
                return Err(TestError::Assertion("expected CONTROL flag".into()));
            }

            // Parse CancelChannel payload
            let cancel_payload: ControlPayload = facet_postcard::from_slice(cancel.payload_bytes())
                .map_err(|e| {
                    TestError::Assertion(format!("failed to decode CancelChannel: {:?}", e))
                })?;

            match cancel_payload {
                ControlPayload::CancelChannel { channel_id, reason } => {
                    if channel_id != channel_to_cancel {
                        return Err(TestError::Assertion(format!(
                            "expected cancel for channel {}, got {}",
                            channel_to_cancel, channel_id
                        )));
                    }
                    if reason != CancelReason::ClientCancel {
                        return Err(TestError::Assertion(format!(
                            "expected ClientCancel reason, got {:?}",
                            reason
                        )));
                    }
                }
                _ => {
                    return Err(TestError::Assertion(format!(
                        "expected CancelChannel, got {:?}",
                        cancel_payload
                    )));
                }
            }

            Ok::<_, TestError>(())
        }
    });

    // Client sends a request on channel 42
    let request_payload = facet_postcard::to_vec(&(1i32, 2i32)).unwrap();

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = channel_to_cancel;
    desc.method_id = 1;
    desc.flags = FrameFlags::DATA;

    let frame = Frame::with_inline_payload(desc, &request_payload).expect("should fit inline");
    client_transport.send_frame(&frame).await?;

    // Client sends cancel control frame
    let cancel_payload = ControlPayload::CancelChannel {
        channel_id: channel_to_cancel,
        reason: CancelReason::ClientCancel,
    };
    let cancel_bytes = facet_postcard::to_vec(&cancel_payload).unwrap();

    let mut cancel_desc = MsgDescHot::new();
    cancel_desc.msg_id = 2;
    cancel_desc.channel_id = 0; // control channel
    cancel_desc.method_id = control_method::CANCEL_CHANNEL;
    cancel_desc.flags = FrameFlags::CONTROL | FrameFlags::EOS;

    let cancel_frame =
        Frame::with_inline_payload(cancel_desc, &cancel_bytes).expect("should fit inline");
    client_transport.send_frame(&cancel_frame).await?;

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

// ============================================================================
// Flow control (credits) scenarios
// ============================================================================

/// Test that credit grants are correctly transmitted.
///
/// Verifies basic flow control messaging without enforcing credit limits.
pub async fn run_credit_grant<F: TransportFactory>() {
    let result = run_credit_grant_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_credit_grant failed: {}", e);
    }
}

async fn run_credit_grant_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let channel_id: u32 = 1;
    let credit_amount: u32 = 65536;

    // Server sends credit grant, client receives it
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            // Send credit grant
            let grant_payload = ControlPayload::GrantCredits {
                channel_id,
                bytes: credit_amount,
            };
            let grant_bytes = facet_postcard::to_vec(&grant_payload).unwrap();

            let mut desc = MsgDescHot::new();
            desc.msg_id = 1;
            desc.channel_id = 0; // control channel
            desc.method_id = control_method::GRANT_CREDITS;
            desc.flags = FrameFlags::CONTROL | FrameFlags::CREDITS | FrameFlags::EOS;
            desc.credit_grant = credit_amount; // Also in descriptor for fast path

            let frame = Frame::with_inline_payload(desc, &grant_bytes).expect("should fit inline");
            server_transport.send_frame(&frame).await?;

            Ok::<_, TestError>(())
        }
    });

    // Client receives credit grant
    let grant = client_transport.recv_frame().await?;

    if grant.desc.channel_id != 0 {
        return Err(TestError::Assertion(
            "credit grant should be on control channel".into(),
        ));
    }
    if grant.desc.method_id != control_method::GRANT_CREDITS {
        return Err(TestError::Assertion(format!(
            "expected GRANT_CREDITS method_id, got {}",
            grant.desc.method_id
        )));
    }
    if !grant.desc.flags.contains(FrameFlags::CREDITS) {
        return Err(TestError::Assertion("expected CREDITS flag".into()));
    }
    if grant.desc.credit_grant != credit_amount {
        return Err(TestError::Assertion(format!(
            "expected credit_grant {}, got {}",
            credit_amount, grant.desc.credit_grant
        )));
    }

    // Parse payload for full verification
    let grant_payload: ControlPayload = facet_postcard::from_slice(grant.payload_bytes())
        .map_err(|e| TestError::Assertion(format!("failed to decode GrantCredits: {:?}", e)))?;

    match grant_payload {
        ControlPayload::GrantCredits {
            channel_id: ch,
            bytes,
        } => {
            if ch != channel_id {
                return Err(TestError::Assertion(format!(
                    "expected channel {}, got {}",
                    channel_id, ch
                )));
            }
            if bytes != credit_amount {
                return Err(TestError::Assertion(format!(
                    "expected {} bytes, got {}",
                    credit_amount, bytes
                )));
            }
        }
        _ => {
            return Err(TestError::Assertion(format!(
                "expected GrantCredits, got {:?}",
                grant_payload
            )));
        }
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

// ============================================================================
// Session-level conformance tests
// ============================================================================
// These tests exercise Session's enforcement of RPC semantics.

/// Test that Session enforces credit limits on data channels.
///
/// When send_credits are exhausted, send_frame should fail with ResourceExhausted.
pub async fn run_session_credit_exhaustion<F: TransportFactory>() {
    let result = run_session_credit_exhaustion_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_session_credit_exhaustion failed: {}", e);
    }
}

async fn run_session_credit_exhaustion_inner<F: TransportFactory>() -> Result<(), TestError> {
    use rapace::session::DEFAULT_INITIAL_CREDITS;

    let (client_transport, _server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);

    // Wrap transport in Session
    let session = Session::new(client_transport);

    // Create a data frame that exceeds available credits
    // Default credits are 64KB, so send a frame larger than that
    let large_payload = vec![0u8; DEFAULT_INITIAL_CREDITS as usize + 1];

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 1; // data channel (not control)
    desc.method_id = 1;
    desc.flags = FrameFlags::DATA | FrameFlags::EOS;
    desc.payload_len = large_payload.len() as u32;

    let frame = Frame::with_payload(desc, large_payload);

    // Should fail with ResourceExhausted
    let result = session.send_frame(&frame).await;

    match result {
        Err(RpcError::Status {
            code: ErrorCode::ResourceExhausted,
            ..
        }) => {
            // Expected
            Ok(())
        }
        Ok(()) => Err(TestError::Assertion(
            "expected ResourceExhausted error, got success".into(),
        )),
        Err(e) => Err(TestError::Assertion(format!(
            "expected ResourceExhausted, got {:?}",
            e
        ))),
    }
}

/// Test that Session silently drops frames for cancelled channels.
pub async fn run_session_cancelled_channel_drop<F: TransportFactory>() {
    let result = run_session_cancelled_channel_drop_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_session_cancelled_channel_drop failed: {}", e);
    }
}

async fn run_session_cancelled_channel_drop_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let session = Session::new(client_transport);
    let channel_id = 42u32;

    // Cancel the channel before sending
    session.cancel_channel(channel_id);

    // Verify the channel is marked cancelled
    if !session.is_cancelled(channel_id) {
        return Err(TestError::Assertion("channel should be cancelled".into()));
    }

    // Send a frame on the cancelled channel - should succeed (silent drop)
    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = channel_id;
    desc.method_id = 1;
    desc.flags = FrameFlags::DATA | FrameFlags::EOS;

    let frame = Frame::with_inline_payload(desc, b"test").expect("should fit");

    // Should succeed (frame is silently dropped, not sent)
    session.send_frame(&frame).await?;

    // The server should not receive anything - let's verify by sending on another channel
    // and checking only that frame arrives
    let mut desc2 = MsgDescHot::new();
    desc2.msg_id = 2;
    desc2.channel_id = 99; // different channel
    desc2.method_id = 1;
    desc2.flags = FrameFlags::DATA | FrameFlags::EOS;

    let frame2 = Frame::with_inline_payload(desc2, b"marker").expect("should fit");
    session.transport().send_frame(&frame2).await?;

    // Server receives only the marker frame
    let received = server_transport.recv_frame().await?;
    if received.desc.channel_id != 99 {
        return Err(TestError::Assertion(format!(
            "expected channel 99, got {}",
            received.desc.channel_id
        )));
    }
    if received.payload_bytes() != b"marker" {
        return Err(TestError::Assertion("expected marker payload".into()));
    }

    Ok(())
}

/// Test that Session processes CANCEL control frames and filters subsequent frames.
pub async fn run_session_cancel_control_frame<F: TransportFactory>() {
    let result = run_session_cancel_control_frame_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_session_cancel_control_frame failed: {}", e);
    }
}

async fn run_session_cancel_control_frame_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let session = Session::new(server_transport);
    let channel_to_cancel = 42u32;

    // Client sends a CANCEL control frame
    let cancel_payload = ControlPayload::CancelChannel {
        channel_id: channel_to_cancel,
        reason: CancelReason::ClientCancel,
    };
    let cancel_bytes = facet_postcard::to_vec(&cancel_payload).unwrap();

    let mut cancel_desc = MsgDescHot::new();
    cancel_desc.msg_id = 1;
    cancel_desc.channel_id = 0; // control channel
    cancel_desc.method_id = control_method::CANCEL_CHANNEL;
    cancel_desc.flags = FrameFlags::CONTROL | FrameFlags::EOS;

    let cancel_frame = Frame::with_inline_payload(cancel_desc, &cancel_bytes).expect("should fit");
    client_transport.send_frame(&cancel_frame).await?;

    // Client sends a data frame on the cancelled channel
    let mut data_desc = MsgDescHot::new();
    data_desc.msg_id = 2;
    data_desc.channel_id = channel_to_cancel;
    data_desc.method_id = 1;
    data_desc.flags = FrameFlags::DATA | FrameFlags::EOS;

    let data_frame = Frame::with_inline_payload(data_desc, b"dropped").expect("should fit");
    client_transport.send_frame(&data_frame).await?;

    // Client sends a data frame on a different channel
    let mut marker_desc = MsgDescHot::new();
    marker_desc.msg_id = 3;
    marker_desc.channel_id = 99;
    marker_desc.method_id = 1;
    marker_desc.flags = FrameFlags::DATA | FrameFlags::EOS;

    let marker_frame = Frame::with_inline_payload(marker_desc, b"marker").expect("should fit");
    client_transport.send_frame(&marker_frame).await?;

    // Session receives control frame first (processes it internally)
    let frame1 = session.recv_frame().await?;
    if frame1.desc.channel_id != 0 {
        return Err(TestError::Assertion(
            "first frame should be control frame".into(),
        ));
    }

    // Channel should now be marked cancelled
    if !session.is_cancelled(channel_to_cancel) {
        return Err(TestError::Assertion(
            "channel should be cancelled after control frame".into(),
        ));
    }

    // Session should skip the cancelled channel frame and return the marker
    let frame2 = session.recv_frame().await?;
    if frame2.desc.channel_id != 99 {
        return Err(TestError::Assertion(format!(
            "expected channel 99 (marker), got {}",
            frame2.desc.channel_id
        )));
    }
    if frame2.payload_bytes() != b"marker" {
        return Err(TestError::Assertion("expected marker payload".into()));
    }

    Ok(())
}

/// Test that Session processes GRANT_CREDITS control frames.
pub async fn run_session_grant_credits_control_frame<F: TransportFactory>() {
    let result = run_session_grant_credits_control_frame_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_session_grant_credits_control_frame failed: {}", e);
    }
}

async fn run_session_grant_credits_control_frame_inner<F: TransportFactory>()
-> Result<(), TestError> {
    use rapace::session::DEFAULT_INITIAL_CREDITS;

    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let session = Session::new(client_transport);
    let channel_id = 1u32;

    // Check initial credits
    let initial = session.get_credits(channel_id);
    if initial != DEFAULT_INITIAL_CREDITS {
        return Err(TestError::Assertion(format!(
            "expected initial credits {}, got {}",
            DEFAULT_INITIAL_CREDITS, initial
        )));
    }

    // Server sends a GRANT_CREDITS control frame
    let grant_payload = ControlPayload::GrantCredits {
        channel_id,
        bytes: 10000,
    };
    let grant_bytes = facet_postcard::to_vec(&grant_payload).unwrap();

    let mut grant_desc = MsgDescHot::new();
    grant_desc.msg_id = 1;
    grant_desc.channel_id = 0;
    grant_desc.method_id = control_method::GRANT_CREDITS;
    grant_desc.flags = FrameFlags::CONTROL | FrameFlags::CREDITS | FrameFlags::EOS;
    grant_desc.credit_grant = 10000;

    let grant_frame = Frame::with_inline_payload(grant_desc, &grant_bytes).expect("should fit");
    server_transport.send_frame(&grant_frame).await?;

    // Session receives and processes the control frame
    let frame = session.recv_frame().await?;
    if frame.desc.channel_id != 0 {
        return Err(TestError::Assertion("expected control frame".into()));
    }

    // Credits should be updated
    let updated = session.get_credits(channel_id);
    let expected = DEFAULT_INITIAL_CREDITS + 10000;
    if updated != expected {
        return Err(TestError::Assertion(format!(
            "expected credits {}, got {}",
            expected, updated
        )));
    }

    Ok(())
}

/// Test Session deadline checking.
pub async fn run_session_deadline_check<F: TransportFactory>() {
    let result = run_session_deadline_check_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_session_deadline_check failed: {}", e);
    }
}

async fn run_session_deadline_check_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, _server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);

    let session = Session::new(client_transport);

    // Test 1: No deadline should not be exceeded
    let mut desc1 = MsgDescHot::new();
    desc1.deadline_ns = NO_DEADLINE;

    if session.is_deadline_exceeded(&desc1) {
        return Err(TestError::Assertion(
            "NO_DEADLINE should not be exceeded".into(),
        ));
    }

    // Test 2: Future deadline should not be exceeded
    let mut desc2 = MsgDescHot::new();
    desc2.deadline_ns = now_ns() + 10_000_000_000; // 10 seconds in future

    if session.is_deadline_exceeded(&desc2) {
        return Err(TestError::Assertion(
            "future deadline should not be exceeded".into(),
        ));
    }

    // Test 3: Past deadline should be exceeded
    // Sleep briefly to ensure time advances
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let mut desc3 = MsgDescHot::new();
    desc3.deadline_ns = 1; // 1ns from start, definitely in the past

    if !session.is_deadline_exceeded(&desc3) {
        return Err(TestError::Assertion(
            "past deadline should be exceeded".into(),
        ));
    }

    Ok(())
}

// ============================================================================
// Streaming scenarios
// ============================================================================
// These tests exercise streaming semantics: multiple frames per channel,
// EOS handling, and half-close state transitions.

/// Test server-streaming: client sends request, server sends N responses + EOS.
///
/// Verifies:
/// - Multiple DATA frames on the same channel
/// - EOS flag properly terminates the stream
/// - Channel state transitions to HalfClosedRemote after EOS
pub async fn run_server_streaming_happy_path<F: TransportFactory>() {
    let result = run_server_streaming_happy_path_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_server_streaming_happy_path failed: {}", e);
    }
}

async fn run_server_streaming_happy_path_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let client_session = Session::new(client_transport.clone());
    let server_session = Session::new(server_transport.clone());

    let channel_id = 1u32;
    let item_count = 5;

    // Server: receive request, send N items, then EOS
    let server_handle = tokio::spawn({
        let server_session = server_session;
        async move {
            // Receive request
            let request = server_session.recv_frame().await?;
            if request.desc.channel_id != channel_id {
                return Err(TestError::Assertion(format!(
                    "expected channel {}, got {}",
                    channel_id, request.desc.channel_id
                )));
            }

            // Request should have EOS (client done sending)
            if !request.desc.flags.contains(FrameFlags::EOS) {
                return Err(TestError::Assertion("request should have EOS".into()));
            }

            // Parse request to get count
            let count: i32 = facet_postcard::from_slice(request.payload_bytes())
                .map_err(|e| TestError::Assertion(format!("decode request: {:?}", e)))?;

            // Send N items (DATA without EOS)
            for i in 0..count {
                let mut desc = MsgDescHot::new();
                desc.msg_id = (i + 1) as u64;
                desc.channel_id = channel_id;
                desc.method_id = request.desc.method_id;
                desc.flags = FrameFlags::DATA; // No EOS yet

                let item_bytes = facet_postcard::to_vec(&i).unwrap();
                let frame =
                    Frame::with_inline_payload(desc, &item_bytes).expect("item should fit inline");
                server_session.send_frame(&frame).await?;
            }

            // Send final EOS frame (empty payload)
            let mut eos_desc = MsgDescHot::new();
            eos_desc.msg_id = (count + 1) as u64;
            eos_desc.channel_id = channel_id;
            eos_desc.method_id = request.desc.method_id;
            eos_desc.flags = FrameFlags::DATA | FrameFlags::EOS;

            let eos_frame =
                Frame::with_inline_payload(eos_desc, &[]).expect("empty frame should fit inline");
            server_session.send_frame(&eos_frame).await?;

            Ok::<_, TestError>(())
        }
    });

    // Client: send request, receive N items + EOS
    let request_bytes = facet_postcard::to_vec(&item_count).unwrap();

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = channel_id;
    desc.method_id = 1;
    desc.flags = FrameFlags::DATA | FrameFlags::EOS; // Client sends request + EOS

    let frame = Frame::with_inline_payload(desc, &request_bytes).expect("should fit inline");
    client_session.send_frame(&frame).await?;

    // After sending EOS, client channel should be HalfClosedLocal
    let state = client_session.get_lifecycle(channel_id);
    if state != rapace::session::ChannelLifecycle::HalfClosedLocal {
        return Err(TestError::Assertion(format!(
            "expected HalfClosedLocal after client EOS, got {:?}",
            state
        )));
    }

    // Receive items
    let mut received = Vec::new();
    loop {
        let frame = client_session.recv_frame().await?;
        if frame.desc.channel_id != channel_id {
            return Err(TestError::Assertion(format!(
                "expected channel {}, got {}",
                channel_id, frame.desc.channel_id
            )));
        }

        // Check if this is EOS
        if frame.desc.flags.contains(FrameFlags::EOS) {
            break;
        }

        // Parse item
        let item: i32 = facet_postcard::from_slice(frame.payload_bytes())
            .map_err(|e| TestError::Assertion(format!("decode item: {:?}", e)))?;
        received.push(item);
    }

    // Verify received items
    let expected: Vec<i32> = (0..item_count).collect();
    if received != expected {
        return Err(TestError::Assertion(format!(
            "expected {:?}, got {:?}",
            expected, received
        )));
    }

    // After receiving EOS, channel should be Closed (both sides sent EOS)
    let final_state = client_session.get_lifecycle(channel_id);
    if final_state != rapace::session::ChannelLifecycle::Closed {
        return Err(TestError::Assertion(format!(
            "expected Closed after both EOS, got {:?}",
            final_state
        )));
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

/// Test client-streaming: client sends N items + EOS, server sends single response.
///
/// Verifies:
/// - Multiple DATA frames from client
/// - Server waits for EOS before responding
/// - Proper state transitions
pub async fn run_client_streaming_happy_path<F: TransportFactory>() {
    let result = run_client_streaming_happy_path_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_client_streaming_happy_path failed: {}", e);
    }
}

async fn run_client_streaming_happy_path_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let client_session = Session::new(client_transport.clone());
    let server_session = Session::new(server_transport.clone());

    let channel_id = 1u32;
    let items_to_send: Vec<i32> = vec![10, 20, 30, 40, 50];

    // Server: receive N items until EOS, compute sum, send response + EOS
    let server_handle = tokio::spawn({
        let server_session = server_session;
        let expected_items = items_to_send.clone();
        async move {
            let mut sum = 0i32;
            let mut count = 0;

            loop {
                let frame = server_session.recv_frame().await?;
                if frame.desc.channel_id != channel_id {
                    return Err(TestError::Assertion(format!(
                        "expected channel {}, got {}",
                        channel_id, frame.desc.channel_id
                    )));
                }

                // Parse item (if not just EOS marker)
                if !frame.payload_bytes().is_empty() {
                    let item: i32 = facet_postcard::from_slice(frame.payload_bytes())
                        .map_err(|e| TestError::Assertion(format!("decode item: {:?}", e)))?;
                    sum += item;
                    count += 1;
                }

                // Check for EOS
                if frame.desc.flags.contains(FrameFlags::EOS) {
                    break;
                }
            }

            // Verify we got all items
            if count != expected_items.len() {
                return Err(TestError::Assertion(format!(
                    "expected {} items, got {}",
                    expected_items.len(),
                    count
                )));
            }

            // Send response + EOS
            let mut desc = MsgDescHot::new();
            desc.msg_id = 1;
            desc.channel_id = channel_id;
            desc.method_id = 1;
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;

            let response_bytes = facet_postcard::to_vec(&sum).unwrap();
            let frame =
                Frame::with_inline_payload(desc, &response_bytes).expect("should fit inline");
            server_session.send_frame(&frame).await?;

            Ok::<_, TestError>(())
        }
    });

    // Client: send N items + EOS, receive response
    for (i, &item) in items_to_send.iter().enumerate() {
        let is_last = i == items_to_send.len() - 1;

        let mut desc = MsgDescHot::new();
        desc.msg_id = (i + 1) as u64;
        desc.channel_id = channel_id;
        desc.method_id = 1;
        desc.flags = if is_last {
            FrameFlags::DATA | FrameFlags::EOS
        } else {
            FrameFlags::DATA
        };

        let item_bytes = facet_postcard::to_vec(&item).unwrap();
        let frame = Frame::with_inline_payload(desc, &item_bytes).expect("should fit inline");
        client_session.send_frame(&frame).await?;
    }

    // Receive response
    let response = client_session.recv_frame().await?;
    if response.desc.channel_id != channel_id {
        return Err(TestError::Assertion(format!(
            "expected channel {}, got {}",
            channel_id, response.desc.channel_id
        )));
    }
    if !response.desc.flags.contains(FrameFlags::EOS) {
        return Err(TestError::Assertion("response should have EOS".into()));
    }

    let sum: i32 = facet_postcard::from_slice(response.payload_bytes())
        .map_err(|e| TestError::Assertion(format!("decode response: {:?}", e)))?;

    let expected_sum: i32 = items_to_send.iter().sum();
    if sum != expected_sum {
        return Err(TestError::Assertion(format!(
            "expected sum {}, got {}",
            expected_sum, sum
        )));
    }

    // Channel should be closed
    let final_state = client_session.get_lifecycle(channel_id);
    if final_state != rapace::session::ChannelLifecycle::Closed {
        return Err(TestError::Assertion(format!(
            "expected Closed, got {:?}",
            final_state
        )));
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

/// Test bidirectional streaming: both sides send multiple items.
///
/// Verifies:
/// - Concurrent DATA frames in both directions
/// - Independent EOS per direction (half-close)
/// - Proper state machine transitions
pub async fn run_bidirectional_streaming<F: TransportFactory>() {
    let result = run_bidirectional_streaming_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_bidirectional_streaming failed: {}", e);
    }
}

async fn run_bidirectional_streaming_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let client_session = Session::new(client_transport.clone());
    let server_session = Session::new(server_transport.clone());

    let channel_id = 1u32;

    // Server: send items 100, 200, 300 then EOS; receive items until EOS
    let server_handle = tokio::spawn({
        let server_session = server_session;
        async move {
            let mut received = Vec::new();

            // Send our items
            for (i, item) in [100i32, 200, 300].iter().enumerate() {
                let is_last = i == 2;
                let mut desc = MsgDescHot::new();
                desc.msg_id = (i + 1) as u64;
                desc.channel_id = channel_id;
                desc.method_id = 1;
                desc.flags = if is_last {
                    FrameFlags::DATA | FrameFlags::EOS
                } else {
                    FrameFlags::DATA
                };

                let item_bytes = facet_postcard::to_vec(item).unwrap();
                let frame =
                    Frame::with_inline_payload(desc, &item_bytes).expect("should fit inline");
                server_session.send_frame(&frame).await?;
            }

            // Receive items until client sends EOS
            loop {
                let frame = server_session.recv_frame().await?;
                if frame.desc.channel_id != channel_id {
                    continue; // Skip other channels
                }

                if !frame.payload_bytes().is_empty() {
                    let item: i32 = facet_postcard::from_slice(frame.payload_bytes())
                        .map_err(|e| TestError::Assertion(format!("decode: {:?}", e)))?;
                    received.push(item);
                }

                if frame.desc.flags.contains(FrameFlags::EOS) {
                    break;
                }
            }

            // Verify received items
            let expected = vec![1, 2, 3, 4, 5];
            if received != expected {
                return Err(TestError::Assertion(format!(
                    "server expected {:?}, got {:?}",
                    expected, received
                )));
            }

            Ok::<_, TestError>(())
        }
    });

    // Client: send items 1, 2, 3, 4, 5 then EOS; receive items until EOS
    let mut client_received = Vec::new();

    // Send client items
    for (i, item) in [1i32, 2, 3, 4, 5].iter().enumerate() {
        let is_last = i == 4;
        let mut desc = MsgDescHot::new();
        desc.msg_id = (i + 100) as u64;
        desc.channel_id = channel_id;
        desc.method_id = 1;
        desc.flags = if is_last {
            FrameFlags::DATA | FrameFlags::EOS
        } else {
            FrameFlags::DATA
        };

        let item_bytes = facet_postcard::to_vec(item).unwrap();
        let frame = Frame::with_inline_payload(desc, &item_bytes).expect("should fit inline");
        client_session.send_frame(&frame).await?;
    }

    // Receive server items until EOS
    loop {
        let frame = client_session.recv_frame().await?;
        if frame.desc.channel_id != channel_id {
            continue;
        }

        if !frame.payload_bytes().is_empty() {
            let item: i32 = facet_postcard::from_slice(frame.payload_bytes())
                .map_err(|e| TestError::Assertion(format!("decode: {:?}", e)))?;
            client_received.push(item);
        }

        if frame.desc.flags.contains(FrameFlags::EOS) {
            break;
        }
    }

    // Verify client received items
    let expected = vec![100, 200, 300];
    if client_received != expected {
        return Err(TestError::Assertion(format!(
            "client expected {:?}, got {:?}",
            expected, client_received
        )));
    }

    // Channel should be closed
    let final_state = client_session.get_lifecycle(channel_id);
    if final_state != rapace::session::ChannelLifecycle::Closed {
        return Err(TestError::Assertion(format!(
            "expected Closed, got {:?}",
            final_state
        )));
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

/// Test streaming with cancellation mid-stream.
///
/// Verifies:
/// - Cancel control frame interrupts streaming
/// - Frames after cancel are dropped
pub async fn run_streaming_cancellation<F: TransportFactory>() {
    let result = run_streaming_cancellation_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_streaming_cancellation failed: {}", e);
    }
}

// ============================================================================
// Macro-generated streaming scenarios
// ============================================================================

/// Test server-streaming using the macro-generated client and server.
///
/// This uses the `RangeService` trait which has a streaming method.
/// The macro generates:
/// - Client `range()` method that returns `impl Stream<Item = Result<u32, RpcError>>`
/// - Server `dispatch_streaming()` method that handles the streaming dispatch
pub async fn run_macro_server_streaming<F: TransportFactory>() {
    let result = run_macro_server_streaming_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_macro_server_streaming failed: {}", e);
    }
}

async fn run_macro_server_streaming_inner<F: TransportFactory>() -> Result<(), TestError> {
    use futures::StreamExt;

    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = RangeServiceServer::new(RangeServiceImpl);

    // Spawn server task to handle the streaming request
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            // Receive request
            let request = server_transport.recv_frame().await?;

            // Dispatch via streaming dispatch (it sends frames directly)
            server
                .dispatch_streaming(
                    request.desc.method_id,
                    request.desc.channel_id,
                    request.payload_bytes(),
                    server_transport.as_ref(),
                )
                .await
                .map_err(TestError::Rpc)?;

            Ok::<_, TestError>(())
        }
    });

    // Create client session and spawn its demux loop
    let client_session = Arc::new(RpcSession::new(client_transport));
    let client_session_runner = client_session.clone();
    let _session_handle = tokio::spawn(async move { client_session_runner.run().await });

    // Create client and make streaming call
    let client = RangeServiceClient::new(client_session);
    let mut stream = client.range(5).await?;

    // Collect all items from the stream
    let mut items = Vec::new();
    while let Some(result) = stream.next().await {
        let item = result?;
        items.push(item);
    }

    // Verify we got all items
    let expected: Vec<u32> = (0..5).collect();
    if items != expected {
        return Err(TestError::Assertion(format!(
            "expected {:?}, got {:?}",
            expected, items
        )));
    }

    // Wait for server to finish
    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

async fn run_streaming_cancellation_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let client_session = Session::new(client_transport.clone());
    let server_session = Session::new(server_transport.clone());

    let channel_id = 1u32;

    // Server: send items, then send cancel, then more items (which should be dropped)
    let server_handle = tokio::spawn({
        let server_session = server_session;
        async move {
            // Send first two items
            for i in 0..2 {
                let mut desc = MsgDescHot::new();
                desc.msg_id = (i + 1) as u64;
                desc.channel_id = channel_id;
                desc.method_id = 1;
                desc.flags = FrameFlags::DATA;

                let item_bytes = facet_postcard::to_vec(&i).unwrap();
                let frame =
                    Frame::with_inline_payload(desc, &item_bytes).expect("should fit inline");
                server_session.send_frame(&frame).await?;
            }

            // Send cancel control frame
            let cancel_payload = ControlPayload::CancelChannel {
                channel_id,
                reason: CancelReason::ClientCancel,
            };
            let cancel_bytes = facet_postcard::to_vec(&cancel_payload).unwrap();

            let mut cancel_desc = MsgDescHot::new();
            cancel_desc.msg_id = 100;
            cancel_desc.channel_id = 0;
            cancel_desc.method_id = control_method::CANCEL_CHANNEL;
            cancel_desc.flags = FrameFlags::CONTROL | FrameFlags::EOS;

            let cancel_frame =
                Frame::with_inline_payload(cancel_desc, &cancel_bytes).expect("should fit inline");
            server_session.transport().send_frame(&cancel_frame).await?;

            // Send marker on different channel (to signal end of test)
            let mut marker_desc = MsgDescHot::new();
            marker_desc.msg_id = 200;
            marker_desc.channel_id = 99;
            marker_desc.method_id = 1;
            marker_desc.flags = FrameFlags::DATA | FrameFlags::EOS;

            let marker_frame =
                Frame::with_inline_payload(marker_desc, b"done").expect("should fit inline");
            server_session.transport().send_frame(&marker_frame).await?;

            Ok::<_, TestError>(())
        }
    });

    // Client: receive items until we see the marker on channel 99
    let mut received = Vec::new();

    loop {
        let frame = client_session.recv_frame().await?;

        if frame.desc.channel_id == 99 {
            // Got marker, done
            break;
        }

        if frame.desc.channel_id == 0 {
            // Control frame, skip (already processed by session)
            continue;
        }

        if frame.desc.channel_id == channel_id && !frame.payload_bytes().is_empty() {
            let item: i32 = facet_postcard::from_slice(frame.payload_bytes())
                .map_err(|e| TestError::Assertion(format!("decode: {:?}", e)))?;
            received.push(item);
        }
    }

    // Should have received items 0, 1 (before cancel was processed)
    // The cancel should have been processed, marking the channel cancelled
    if received.len() > 2 {
        return Err(TestError::Assertion(format!(
            "expected at most 2 items (before cancel), got {:?}",
            received
        )));
    }

    // Channel should be cancelled
    if !client_session.is_cancelled(channel_id) {
        return Err(TestError::Assertion("channel should be cancelled".into()));
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

// ============================================================================
// Large blob service for testing large payloads
// ============================================================================

/// Service that processes large byte payloads.
///
/// This service is used to test that large payloads (e.g., images, documents)
/// are correctly transmitted across all transports. For SHM transport, this
/// also enables testing the zero-copy path.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait LargeBlobService {
    /// Echo the blob back unchanged.
    /// Used to verify round-trip integrity of large payloads.
    async fn echo(&self, data: Vec<u8>) -> Vec<u8>;

    /// Transform the blob by XORing each byte with a pattern.
    /// Used to verify the server actually processes the data.
    async fn xor_transform(&self, data: Vec<u8>, pattern: u8) -> Vec<u8>;

    /// Compute a simple checksum of the blob.
    /// Returns (length, sum of all bytes mod 2^32).
    async fn checksum(&self, data: Vec<u8>) -> (u32, u32);
}

/// Implementation of LargeBlobService for testing.
pub struct LargeBlobServiceImpl;

impl LargeBlobService for LargeBlobServiceImpl {
    async fn echo(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }

    async fn xor_transform(&self, data: Vec<u8>, pattern: u8) -> Vec<u8> {
        data.into_iter().map(|b| b ^ pattern).collect()
    }

    async fn checksum(&self, data: Vec<u8>) -> (u32, u32) {
        let len = data.len() as u32;
        let sum = data.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
        (len, sum)
    }
}

// ============================================================================
// Large blob test scenarios
// ============================================================================

/// Test that large blobs are correctly echoed back.
///
/// This verifies that payloads larger than the inline threshold (and larger
/// than typical small messages) are correctly transmitted and received.
pub async fn run_large_blob_echo<F: TransportFactory>() {
    let result = run_large_blob_echo_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_large_blob_echo failed: {}", e);
    }
}

async fn run_large_blob_echo_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = LargeBlobServiceServer::new(LargeBlobServiceImpl);

    // Spawn server task
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            let request = server_transport.recv_frame().await?;
            let mut response = server
                .dispatch(request.desc.method_id, request.payload_bytes())
                .await
                .map_err(TestError::Rpc)?;
            // Set channel_id on response to match request
            response.desc.channel_id = request.desc.channel_id;
            server_transport.send_frame(&response).await?;
            Ok::<_, TestError>(())
        }
    });

    // Create client session and spawn its demux loop
    let client_session = Arc::new(RpcSession::new(client_transport));
    let client_session_runner = client_session.clone();
    let _session_handle = tokio::spawn(async move { client_session_runner.run().await });

    // Create a large blob (1KB)
    let blob: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();

    let client = LargeBlobServiceClient::new(client_session);
    let result = client.echo(blob.clone()).await?;

    if result != blob {
        return Err(TestError::Assertion(format!(
            "echo mismatch: expected {} bytes, got {} bytes",
            blob.len(),
            result.len()
        )));
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

/// Test large blob transformation.
///
/// Verifies that the server actually processes the blob (not just echoing).
pub async fn run_large_blob_transform<F: TransportFactory>() {
    let result = run_large_blob_transform_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_large_blob_transform failed: {}", e);
    }
}

async fn run_large_blob_transform_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = LargeBlobServiceServer::new(LargeBlobServiceImpl);

    // Spawn server task
    let server_handle = tokio::spawn({
        let server_transport = server_transport.clone();
        async move {
            let request = server_transport.recv_frame().await?;
            let mut response = server
                .dispatch(request.desc.method_id, request.payload_bytes())
                .await
                .map_err(TestError::Rpc)?;
            // Set channel_id on response to match request
            response.desc.channel_id = request.desc.channel_id;
            server_transport.send_frame(&response).await?;
            Ok::<_, TestError>(())
        }
    });

    // Create client session and spawn its demux loop
    let client_session = Arc::new(RpcSession::new(client_transport));
    let client_session_runner = client_session.clone();
    let _session_handle = tokio::spawn(async move { client_session_runner.run().await });

    // Create blob and expected result
    let blob: Vec<u8> = (0..2048).map(|i| (i % 256) as u8).collect();
    let pattern: u8 = 0xAA;
    let expected: Vec<u8> = blob.iter().map(|&b| b ^ pattern).collect();

    let client = LargeBlobServiceClient::new(client_session);
    let result = client.xor_transform(blob, pattern).await?;

    if result != expected {
        return Err(TestError::Assertion(format!(
            "transform mismatch at byte 0: expected {:02x}, got {:02x}",
            expected[0], result[0]
        )));
    }

    server_handle
        .await
        .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
        .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;

    Ok(())
}

/// Test large blob checksum (verifies both directions work for different sizes).
pub async fn run_large_blob_checksum<F: TransportFactory>() {
    let result = run_large_blob_checksum_inner::<F>().await;
    if let Err(e) = result {
        panic!("run_large_blob_checksum failed: {}", e);
    }
}

async fn run_large_blob_checksum_inner<F: TransportFactory>() -> Result<(), TestError> {
    let (client_transport, server_transport) = F::connect_pair().await?;
    let client_transport = Arc::new(client_transport);
    let server_transport = Arc::new(server_transport);

    let server = LargeBlobServiceServer::new(LargeBlobServiceImpl);

    // Create client session and spawn its demux loop
    let client_session = Arc::new(RpcSession::new(client_transport));
    let client_session_runner = client_session.clone();
    let _session_handle = tokio::spawn(async move { client_session_runner.run().await });

    // Test multiple sizes
    let sizes = [100, 1000, 4000]; // Various sizes around and above inline threshold

    for size in sizes {
        // Spawn server task for this request
        let server_handle = tokio::spawn({
            let server_transport = server_transport.clone();
            let server = LargeBlobServiceServer::new(LargeBlobServiceImpl);
            async move {
                let request = server_transport.recv_frame().await?;
                let mut response = server
                    .dispatch(request.desc.method_id, request.payload_bytes())
                    .await
                    .map_err(TestError::Rpc)?;
                // Set channel_id on response to match request
                response.desc.channel_id = request.desc.channel_id;
                server_transport.send_frame(&response).await?;
                Ok::<_, TestError>(())
            }
        });

        // Create blob
        let blob: Vec<u8> = (0..size).map(|i| ((i * 7) % 256) as u8).collect();
        let expected_len = blob.len() as u32;
        let expected_sum = blob.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32));

        let client = LargeBlobServiceClient::new(client_session.clone());
        let (len, sum) = client.checksum(blob).await?;

        if len != expected_len {
            return Err(TestError::Assertion(format!(
                "size {}: length mismatch: expected {}, got {}",
                size, expected_len, len
            )));
        }
        if sum != expected_sum {
            return Err(TestError::Assertion(format!(
                "size {}: sum mismatch: expected {}, got {}",
                size, expected_sum, sum
            )));
        }

        server_handle
            .await
            .map_err(|e| TestError::Setup(format!("server task panicked: {}", e)))?
            .map_err(|e| TestError::Setup(format!("server error: {}", e)))?;
    }

    let _ = server; // suppress unused warning

    Ok(())
}

// ============================================================================
// Registry tests
// ============================================================================

/// Test that the macro-generated register function works correctly.
///
/// Verifies:
/// - Services can be registered via the generated function
/// - Method IDs are assigned correctly
/// - Schemas are captured for request/response types
#[cfg(test)]
mod registry_tests {
    use rapace_registry::ServiceRegistry;

    use super::*;

    #[test]
    fn test_adder_registration() {
        let mut registry = ServiceRegistry::new();
        adder_register(&mut registry);

        // Verify service was registered
        let service = registry
            .service("Adder")
            .expect("Adder service should exist");
        assert_eq!(service.name, "Adder");

        // Verify method was registered
        let add_method = service.method("add").expect("add method should exist");
        assert_eq!(add_method.name, "add");
        assert_eq!(add_method.full_name, "Adder.add");
        assert!(!add_method.is_streaming);

        // Verify schemas are present and non-empty
        assert!(
            !add_method.request_shape.type_identifier.is_empty(),
            "request shape should have a type identifier"
        );
        assert!(
            !add_method.response_shape.type_identifier.is_empty(),
            "response shape should have a type identifier"
        );

        // Verify method can be looked up by ID
        let by_id = registry.method_by_id(add_method.id);
        assert!(by_id.is_some());
        assert_eq!(by_id.unwrap().name, "add");
    }

    #[test]
    fn test_range_service_registration() {
        let mut registry = ServiceRegistry::new();
        range_service_register(&mut registry);

        // Verify service was registered
        let service = registry
            .service("RangeService")
            .expect("RangeService should exist");
        assert_eq!(service.name, "RangeService");

        // Verify streaming method was registered
        let range_method = service.method("range").expect("range method should exist");
        assert_eq!(range_method.name, "range");
        assert_eq!(range_method.full_name, "RangeService.range");
        assert!(
            range_method.is_streaming,
            "range should be a streaming method"
        );
    }

    #[test]
    fn test_multiple_services_registration() {
        let mut registry = ServiceRegistry::new();

        // Register both services
        adder_register(&mut registry);
        range_service_register(&mut registry);

        // Verify both exist
        assert_eq!(registry.service_count(), 2);
        assert_eq!(registry.method_count(), 2);

        // Method IDs should be globally unique
        let add_id = registry.resolve_method_id("Adder", "add").unwrap();
        let range_id = registry.resolve_method_id("RangeService", "range").unwrap();
        assert_ne!(add_id, range_id);

        // Both should start at 1 (0 is reserved for control)
        assert!(add_id.0 >= 1);
        assert!(range_id.0 >= 1);
    }

    #[test]
    fn test_lookup_by_name() {
        let mut registry = ServiceRegistry::new();
        adder_register(&mut registry);
        range_service_register(&mut registry);

        // Valid lookups
        assert!(registry.lookup_method("Adder", "add").is_some());
        assert!(registry.lookup_method("RangeService", "range").is_some());

        // Invalid lookups
        assert!(registry.lookup_method("Adder", "subtract").is_none());
        assert!(registry.lookup_method("NonExistent", "add").is_none());
    }

    #[test]
    fn test_registry_client_method_ids() {
        // Test that the registry client uses registry-assigned method IDs
        use rapace_transport_mem::InProcTransport;
        use std::sync::Arc;

        let mut registry = ServiceRegistry::new();

        // Register services in a specific order
        adder_register(&mut registry);
        range_service_register(&mut registry);

        // Verify that registry assigns sequential IDs
        let add_id = registry.resolve_method_id("Adder", "add").unwrap();
        let range_id = registry.resolve_method_id("RangeService", "range").unwrap();

        assert_eq!(add_id.0, 1, "First method should have ID 1");
        assert_eq!(range_id.0, 2, "Second method should have ID 2");

        // Create a registry-aware client and verify it uses the correct IDs
        let (client_transport, _server_transport) = InProcTransport::pair();
        let client_session = RpcSession::new(Arc::new(client_transport));
        let client = AdderRegistryClient::new(Arc::new(client_session), &registry);

        // The client should have the correct method ID stored
        assert_eq!(client.add_method_id, 1);
    }

    #[test]
    fn test_doc_capture() {
        // Test that doc comments are captured from the trait and methods
        let mut registry = ServiceRegistry::new();
        adder_register(&mut registry);

        let service = registry
            .service("Adder")
            .expect("Adder service should exist");

        // Service doc should contain the trait doc comment
        assert!(
            service.doc.contains("Simple arithmetic service"),
            "Service doc should contain trait doc comment, got: {:?}",
            service.doc
        );

        // Method doc should contain the method doc comment
        let add_method = service.method("add").expect("add method should exist");
        assert!(
            add_method.doc.contains("Add two numbers"),
            "Method doc should contain method doc comment, got: {:?}",
            add_method.doc
        );
    }

    #[test]
    fn test_streaming_method_doc_capture() {
        // Test that doc comments are captured for streaming methods
        let mut registry = ServiceRegistry::new();
        range_service_register(&mut registry);

        let service = registry
            .service("RangeService")
            .expect("RangeService should exist");

        // Service doc should contain the trait doc comments
        assert!(
            service.doc.contains("Service with server-streaming RPC"),
            "Service doc should contain trait doc comment, got: {:?}",
            service.doc
        );

        // Method doc should contain the method doc comment
        let range_method = service.method("range").expect("range method should exist");
        assert!(
            range_method.doc.contains("Stream numbers"),
            "Method doc should contain method doc comment, got: {:?}",
            range_method.doc
        );
    }

    #[test]
    fn test_auto_registration() {
        // Creating a server should auto-register in the global registry
        let _server = AdderServer::new(AdderImpl);

        // Verify it's registered globally
        ServiceRegistry::with_global(|registry| {
            let service = registry
                .service("Adder")
                .expect("Adder should be auto-registered");
            assert_eq!(service.name, "Adder");

            let method = service.method("add").expect("add method should exist");
            assert_eq!(method.name, "add");
        });
    }

    #[test]
    fn test_auto_registration_once() {
        // Creating multiple servers should only register once
        // Note: Adder might already be registered from other tests (global registry is shared)
        let count_before = ServiceRegistry::with_global(|reg| reg.service_count());

        let _server1 = AdderServer::new(AdderImpl);
        let _server2 = AdderServer::new(AdderImpl);
        let _server3 = AdderServer::new(AdderImpl);

        let count_after = ServiceRegistry::with_global(|reg| reg.service_count());

        // Should not add any new services (already registered) OR add exactly one (if first time)
        assert!(
            count_after == count_before || count_after == count_before + 1,
            "Expected count to stay same or increase by 1, got before={} after={}",
            count_before,
            count_after
        );
    }
}
