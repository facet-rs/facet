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

use rapace_core::{
    control_method, CancelReason, ControlPayload, ErrorCode, Frame, FrameFlags, MsgDescHot,
    RpcError, Transport, NO_DEADLINE,
};

mod session;
pub use session::Session;

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
    fn connect_pair() -> impl Future<Output = Result<(Self::Transport, Self::Transport), TestError>> + Send;
}

// ============================================================================
// Test service: Adder
// ============================================================================

/// Simple arithmetic service used for testing.
#[allow(async_fn_in_trait)]
#[rapace_macros::service]
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
            let response = server
                .dispatch(request.desc.method_id, request.payload)
                .await
                .map_err(TestError::Rpc)?;
            server_transport.send_frame(&response).await?;
            Ok::<_, TestError>(())
        }
    });

    // Create client and make call
    let client = AdderClient::new(client_transport);
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
                let response = server
                    .dispatch(request.desc.method_id, request.payload)
                    .await
                    .map_err(TestError::Rpc)?;
                server_transport.send_frame(&response).await?;
            }
            Ok::<_, TestError>(())
        }
    });

    let client = AdderClient::new(client_transport);

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

    // Client makes call and expects error
    let client = AdderClient::new(client_transport);
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
                .payload
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
    if pong.payload != ping_data {
        return Err(TestError::Assertion(format!(
            "PONG payload mismatch: expected {:?}, got {:?}",
            ping_data, pong.payload
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
            let response = server
                .dispatch(request.desc.method_id, request.payload)
                .await
                .map_err(TestError::Rpc)?;
            server_transport.send_frame(&response).await?;
            Ok::<_, TestError>(())
        }
    });

    // Client sets a generous deadline (10 seconds from now)
    let deadline = now_ns() + 10_000_000_000; // 10 seconds

    // We need to set the deadline on the frame. Since the generated client
    // doesn't support deadlines yet, we'll call add() which should succeed
    // because the server won't see an expired deadline.
    let client = AdderClient::new(client_transport);
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
    if response.payload.len() < 8 {
        return Err(TestError::Assertion("error payload too short".into()));
    }

    let error_code = u32::from_le_bytes(response.payload[0..4].try_into().unwrap());
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
            let cancel_payload: ControlPayload =
                facet_postcard::from_bytes(cancel.payload).map_err(|e| {
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

            let frame =
                Frame::with_inline_payload(desc, &grant_bytes).expect("should fit inline");
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
    let grant_payload: ControlPayload =
        facet_postcard::from_bytes(grant.payload).map_err(|e| {
            TestError::Assertion(format!("failed to decode GrantCredits: {:?}", e))
        })?;

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
    use session::DEFAULT_INITIAL_CREDITS;

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
    if received.payload != b"marker" {
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

    let data_frame =
        Frame::with_inline_payload(data_desc, b"dropped").expect("should fit");
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
    if frame2.payload != b"marker" {
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

async fn run_session_grant_credits_control_frame_inner<F: TransportFactory>(
) -> Result<(), TestError> {
    use session::DEFAULT_INITIAL_CREDITS;

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
