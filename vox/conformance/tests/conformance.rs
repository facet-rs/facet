//! Conformance tests using real rapace-core implementation.
//!
//! This test harness runs conformance tests from the rapace-conformance binary.
//! It spawns the conformance harness and connects to it using a real StreamTransport,
//! letting rapace-core handle the protocol.

use std::process::Stdio;
use std::sync::Arc;

use libtest_mimic::{Arguments, Failed, Trial};
use tokio::process::{Child, Command as TokioCommand};
use tracing::trace;

use rapace_core::stream::StreamTransport;
use rapace_core::{BufferPool, Frame, FrameFlags, MsgDescHot, Payload, RpcSession, Transport};
use rapace_protocol::{
    ChannelKind, Hello, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, Limits, OpenChannel,
    PROTOCOL_VERSION_1_0, Role, control_verb, features, flags,
};

/// Wrapper to make ChildStdin/ChildStdout work with StreamTransport.
struct ChildIo {
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
}

impl tokio::io::AsyncRead for ChildIo {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for ChildIo {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

/// Spawn the conformance harness and create a transport connected to it.
async fn spawn_harness(
    bin_path: &str,
    test_case: &str,
) -> Result<(Child, StreamTransport), String> {
    let mut child = TokioCommand::new(bin_path)
        .args(["--case", test_case])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()) // Capture stderr so we can see harness errors
        .spawn()
        .map_err(|e| format!("failed to spawn conformance binary: {}", e))?;

    let stdin = child.stdin.take().ok_or("failed to get stdin")?;
    let stdout = child.stdout.take().ok_or("failed to get stdout")?;

    let io = ChildIo { stdin, stdout };
    let transport = StreamTransport::with_buffer_pool(io, BufferPool::new());

    Ok((child, transport))
}

/// Send a Hello frame as initiator.
async fn send_hello(transport: &StreamTransport) -> Result<(), String> {
    let hello = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Initiator,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: vec![],
    };

    let payload = facet_format_postcard::to_vec(&hello)
        .map_err(|e| format!("failed to encode Hello: {}", e))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = FrameFlags::from_bits_truncate(flags::CONTROL);

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(&payload);
        Frame {
            desc,
            payload: Payload::Inline,
        }
    } else {
        desc.payload_slot = 0;
        desc.payload_len = payload.len() as u32;
        Frame {
            desc,
            payload: Payload::Owned(payload),
        }
    };

    transport
        .send_frame(frame)
        .await
        .map_err(|e| format!("failed to send Hello: {}", e))
}

/// Receive and validate Hello response.
async fn recv_hello(transport: &StreamTransport) -> Result<(), String> {
    let frame = transport
        .recv_frame()
        .await
        .map_err(|e| format!("failed to receive Hello response: {}", e))?;

    trace!(
        channel_id = frame.desc.channel_id,
        method_id = frame.desc.method_id,
        flags = ?frame.desc.flags,
        payload_len = frame.desc.payload_len,
        "received frame"
    );

    if frame.desc.channel_id != 0 {
        return Err(format!(
            "expected Hello on channel 0, got channel {}",
            frame.desc.channel_id
        ));
    }

    if frame.desc.method_id != control_verb::HELLO {
        return Err(format!(
            "expected Hello (method_id=0), got method_id={}",
            frame.desc.method_id
        ));
    }

    // Decode and validate
    let hello: Hello = facet_format_postcard::from_slice(frame.payload_bytes())
        .map_err(|e| format!("failed to decode Hello: {}", e))?;

    trace!(?hello, "decoded Hello response");

    if hello.role != Role::Acceptor {
        return Err(format!(
            "expected Role::Acceptor in response, got {:?}",
            hello.role
        ));
    }

    Ok(())
}

/// Run the handshake.valid_hello_exchange test.
async fn run_handshake_valid_hello_exchange(bin_path: &str) -> Result<(), String> {
    let (mut child, transport) = spawn_harness(bin_path, "handshake.valid_hello_exchange").await?;

    // Send our Hello as initiator
    send_hello(&transport).await?;

    // Receive Hello response from harness
    recv_hello(&transport).await?;

    // Close transport and wait for child
    transport.close();

    let status = child
        .wait()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "conformance test failed with exit code {:?}",
            status.code()
        ))
    }
}

/// Send an OpenChannel control message.
async fn send_open_channel(
    transport: &StreamTransport,
    channel_id: u32,
    kind: ChannelKind,
) -> Result<(), String> {
    let open = OpenChannel {
        channel_id,
        kind,
        attach: None,
        metadata: vec![],
        initial_credits: 1024 * 1024,
    };

    let payload = facet_format_postcard::to_vec(&open)
        .map_err(|e| format!("failed to encode OpenChannel: {}", e))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2; // After Hello
    desc.channel_id = 0; // Control channel
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = FrameFlags::from_bits_truncate(flags::CONTROL);

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(&payload);
        Frame {
            desc,
            payload: Payload::Inline,
        }
    } else {
        desc.payload_slot = 0;
        desc.payload_len = payload.len() as u32;
        Frame {
            desc,
            payload: Payload::Owned(payload),
        }
    };

    transport
        .send_frame(frame)
        .await
        .map_err(|e| format!("failed to send OpenChannel: {}", e))
}

/// Run the call.one_req_one_resp test using RpcSession.
///
/// This tests that RpcSession.call() properly sends frames and receives responses.
async fn run_call_one_req_one_resp(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "call.one_req_one_resp").await?;

    // 1. Send Hello (RpcSession doesn't do this)
    send_hello(&transport).await?;
    trace!("sent Hello");

    // 2. Receive Hello response
    recv_hello(&transport).await?;
    trace!("received Hello response");

    // 3. Send OpenChannel for channel 1 (RpcSession doesn't do this either)
    // RpcSession starts channel IDs at 1 (odd = initiator)
    let channel_id = 1u32;
    send_open_channel(&transport, channel_id, ChannelKind::Call).await?;
    trace!(channel_id, "sent OpenChannel");

    // 4. Create RpcSession and use it to make the call
    // The session will send DATA|EOS and wait for response
    let session = Arc::new(RpcSession::new(transport));

    // Spawn the run loop to receive the response
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    // Make the call on the channel we already opened
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let response = session
        .call(channel_id, method_id, b"test request".to_vec())
        .await
        .map_err(|e| format!("call failed: {:?}", e))?;

    trace!(
        channel_id = response.channel_id(),
        method_id = response.method_id(),
        flags = ?response.flags(),
        "received response via RpcSession"
    );

    // Validate response
    if response.channel_id() != channel_id {
        return Err(format!(
            "response on wrong channel: expected {}, got {}",
            channel_id,
            response.channel_id()
        ));
    }

    if !response.flags().contains(FrameFlags::RESPONSE) {
        return Err("response missing RESPONSE flag".to_string());
    }

    // Clean up
    session.close();
    run_handle.abort();

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        trace!("harness stderr: {}", stderr);
    }

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "conformance test failed with exit code {:?}, stderr: {}",
            output.status.code(),
            stderr
        ))
    }
}

/// Helper to run a call test that makes a single RPC.
/// Returns the response frame for validation.
async fn run_single_call_test(
    bin_path: &str,
    test_name: &str,
    method_id: u32,
    payload: Vec<u8>,
) -> Result<(rapace_core::ReceivedFrame, Child), String> {
    let (child, transport) = spawn_harness(bin_path, test_name).await?;

    // 1. Send Hello
    send_hello(&transport).await?;

    // 2. Receive Hello response
    recv_hello(&transport).await?;

    // 3. Send OpenChannel for channel 1
    let channel_id = 1u32;
    send_open_channel(&transport, channel_id, ChannelKind::Call).await?;

    // 4. Create RpcSession and make the call
    let session = Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    let response = session
        .call(channel_id, method_id, payload)
        .await
        .map_err(|e| format!("call failed: {:?}", e))?;

    session.close();
    run_handle.abort();

    Ok((response, child))
}

/// Run call.request_flags test - verifies our request has correct flags.
/// Note: This test validates request flags only - harness doesn't send a response.
async fn run_call_request_flags(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "call.request_flags").await?;

    // 1. Send Hello
    send_hello(&transport).await?;
    recv_hello(&transport).await?;

    // 2. Send OpenChannel
    let channel_id = 1u32;
    send_open_channel(&transport, channel_id, ChannelKind::Call).await?;

    // 3. Send request with DATA|EOS flags (this is what the test validates)
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let payload = b"test".to_vec();

    let mut desc = MsgDescHot::new();
    desc.msg_id = 3;
    desc.channel_id = channel_id;
    desc.method_id = method_id;
    desc.flags = FrameFlags::DATA | FrameFlags::EOS;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(&payload);
        Frame {
            desc,
            payload: Payload::Inline,
        }
    } else {
        desc.payload_slot = 0;
        desc.payload_len = payload.len() as u32;
        Frame {
            desc,
            payload: Payload::Owned(payload),
        }
    };

    transport
        .send_frame(frame)
        .await
        .map_err(|e| format!("failed to send request: {}", e))?;

    // Harness validates flags and exits - no response expected
    transport.close();

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run call.response_msg_id_echo test - verifies response echoes our msg_id.
async fn run_call_response_msg_id_echo(bin_path: &str) -> Result<(), String> {
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let (response, child) = run_single_call_test(
        bin_path,
        "call.response_msg_id_echo",
        method_id,
        b"test".to_vec(),
    )
    .await?;

    // The harness validates that we sent a proper request and responds
    trace!(
        msg_id = response.frame.desc.msg_id,
        "response_msg_id_echo: got response"
    );

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run call.request_payload test - verifies payload is sent correctly.
async fn run_call_request_payload(bin_path: &str) -> Result<(), String> {
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let payload = b"test payload data".to_vec();
    let (response, child) =
        run_single_call_test(bin_path, "call.request_payload", method_id, payload).await?;

    trace!(
        payload_len = response.payload_bytes().len(),
        "request_payload: got response"
    );

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run call.response_payload test - verifies we can receive payload.
async fn run_call_response_payload(bin_path: &str) -> Result<(), String> {
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let (response, child) = run_single_call_test(
        bin_path,
        "call.response_payload",
        method_id,
        b"request".to_vec(),
    )
    .await?;

    // Harness should echo back our payload
    trace!(
        payload_len = response.payload_bytes().len(),
        "response_payload: got response"
    );

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run call.call_complete test - verifies call completes properly.
async fn run_call_call_complete(bin_path: &str) -> Result<(), String> {
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let (response, child) =
        run_single_call_test(bin_path, "call.call_complete", method_id, b"test".to_vec()).await?;

    // Response should have EOS flag indicating completion
    if !response.flags().contains(FrameFlags::EOS) {
        return Err("response missing EOS flag".to_string());
    }

    trace!("call_complete: call completed with EOS");

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run call.error_flag_match test - verifies ERROR flag on error responses.
async fn run_call_error_flag_match(bin_path: &str) -> Result<(), String> {
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let (response, child) = run_single_call_test(
        bin_path,
        "call.error_flag_match",
        method_id,
        b"test".to_vec(),
    )
    .await?;

    // Harness sends an error response - verify it has ERROR flag
    if !response.flags().contains(FrameFlags::ERROR) {
        return Err("error response missing ERROR flag".to_string());
    }

    trace!(flags = ?response.flags(), "error_flag_match: got error response");

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run call.unknown_method test - verifies UNIMPLEMENTED for unknown methods.
async fn run_call_unknown_method(bin_path: &str) -> Result<(), String> {
    // Use an unknown method_id
    let method_id = 0xDEADBEEF;
    let (response, child) =
        run_single_call_test(bin_path, "call.unknown_method", method_id, b"test".to_vec()).await?;

    // Harness should respond with UNIMPLEMENTED error
    if !response.flags().contains(FrameFlags::ERROR) {
        return Err("unknown method response missing ERROR flag".to_string());
    }

    trace!(flags = ?response.flags(), "unknown_method: got error response");

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Send a Hello frame as acceptor.
async fn send_hello_as_acceptor(transport: &StreamTransport) -> Result<(), String> {
    let hello = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: vec![],
    };

    let payload = facet_format_postcard::to_vec(&hello)
        .map_err(|e| format!("failed to encode Hello: {}", e))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = FrameFlags::from_bits_truncate(flags::CONTROL);

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(&payload);
        Frame {
            desc,
            payload: Payload::Inline,
        }
    } else {
        desc.payload_slot = 0;
        desc.payload_len = payload.len() as u32;
        Frame {
            desc,
            payload: Payload::Owned(payload),
        }
    };

    transport
        .send_frame(frame)
        .await
        .map_err(|e| format!("failed to send Hello: {}", e))
}

/// Receive and validate Hello from initiator.
async fn recv_hello_from_initiator(transport: &StreamTransport) -> Result<(), String> {
    let frame = transport
        .recv_frame()
        .await
        .map_err(|e| format!("failed to receive Hello: {}", e))?;

    if frame.desc.channel_id != 0 {
        return Err(format!(
            "expected Hello on channel 0, got channel {}",
            frame.desc.channel_id
        ));
    }

    if frame.desc.method_id != control_verb::HELLO {
        return Err(format!(
            "expected Hello (method_id=0), got method_id={}",
            frame.desc.method_id
        ));
    }

    let hello: Hello = facet_format_postcard::from_slice(frame.payload_bytes())
        .map_err(|e| format!("failed to decode Hello: {}", e))?;

    if hello.role != Role::Initiator {
        return Err(format!(
            "expected Role::Initiator in Hello, got {:?}",
            hello.role
        ));
    }

    Ok(())
}

/// Run call.response_method_id_must_match test.
///
/// In this test, the harness acts as INITIATOR - it sends Hello, OpenChannel, and request.
/// We act as ACCEPTOR - we receive the request and send a response echoing method_id.
async fn run_call_response_method_id_must_match(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "call.response_method_id_must_match").await?;

    // 1. Receive Hello from harness (harness is initiator)
    recv_hello_from_initiator(&transport).await?;
    trace!("received Hello from initiator");

    // 2. Send Hello as acceptor
    send_hello_as_acceptor(&transport).await?;
    trace!("sent Hello as acceptor");

    // 3. Receive OpenChannel from harness
    let frame = transport
        .recv_frame()
        .await
        .map_err(|e| format!("failed to receive OpenChannel: {}", e))?;

    if frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return Err(format!(
            "expected OpenChannel (method_id={}), got method_id={}",
            control_verb::OPEN_CHANNEL,
            frame.desc.method_id
        ));
    }

    let open: OpenChannel = facet_format_postcard::from_slice(frame.payload_bytes())
        .map_err(|e| format!("failed to decode OpenChannel: {}", e))?;

    let channel_id = open.channel_id;
    trace!(channel_id, "received OpenChannel");

    // 4. Receive request from harness
    let request = transport
        .recv_frame()
        .await
        .map_err(|e| format!("failed to receive request: {}", e))?;

    if request.desc.channel_id != channel_id {
        return Err(format!(
            "request on wrong channel: expected {}, got {}",
            channel_id, request.desc.channel_id
        ));
    }

    let request_method_id = request.desc.method_id;
    trace!(method_id = request_method_id, "received request");

    // 5. Send response echoing method_id
    let mut desc = MsgDescHot::new();
    desc.msg_id = 10; // arbitrary
    desc.channel_id = channel_id;
    desc.method_id = request_method_id; // Echo the method_id per spec
    desc.flags = FrameFlags::RESPONSE | FrameFlags::DATA | FrameFlags::EOS;

    let response_payload = b"response";
    desc.payload_slot = INLINE_PAYLOAD_SLOT;
    desc.payload_len = response_payload.len() as u32;
    desc.inline_payload[..response_payload.len()].copy_from_slice(response_payload);

    let response_frame = Frame {
        desc,
        payload: Payload::Inline,
    };

    transport
        .send_frame(response_frame)
        .await
        .map_err(|e| format!("failed to send response: {}", e))?;

    trace!(
        method_id = request_method_id,
        "sent response with echoed method_id"
    );

    // 6. Wait for harness to validate and exit
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run control.flag_set_on_channel_zero test.
///
/// Tests that Hello frame has CONTROL flag set (required for channel 0).
async fn run_control_flag_set_on_channel_zero(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "control.flag_set_on_channel_zero").await?;

    // Send Hello - the harness will check our Hello has CONTROL flag set
    send_hello(&transport).await?;

    // Wait for harness to validate
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run control.flag_clear_on_other_channels test.
///
/// Tests that data frames (non-channel 0) do NOT have CONTROL flag set.
async fn run_control_flag_clear_on_other_channels(bin_path: &str) -> Result<(), String> {
    let (child, transport) =
        spawn_harness(bin_path, "control.flag_clear_on_other_channels").await?;

    // 1. Send Hello
    send_hello(&transport).await?;

    // 2. Receive Hello response
    recv_hello(&transport).await?;

    // 3. Send OpenChannel - this is on channel 0, has CONTROL flag
    let channel_id = 1u32;
    send_open_channel(&transport, channel_id, ChannelKind::Call).await?;

    // 4. Send a data frame on channel 1 directly (without CONTROL flag)
    // The harness checks this does NOT have CONTROL flag
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let payload = b"test";

    let mut desc = MsgDescHot::new();
    desc.msg_id = 3;
    desc.channel_id = channel_id;
    desc.method_id = method_id;
    // DATA | EOS flags, but NOT CONTROL
    desc.flags = FrameFlags::DATA | FrameFlags::EOS;
    desc.payload_slot = INLINE_PAYLOAD_SLOT;
    desc.payload_len = payload.len() as u32;
    desc.inline_payload[..payload.len()].copy_from_slice(payload);

    let frame = Frame {
        desc,
        payload: Payload::Inline,
    };

    transport
        .send_frame(frame)
        .await
        .map_err(|e| format!("failed to send data frame: {}", e))?;

    // Wait for harness to validate
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run channel.lifecycle test.
///
/// Tests the channel lifecycle: Open -> Active -> HalfClosed -> Closed.
/// Harness opens a channel, sends EOS, expects EOS back.
async fn run_channel_lifecycle(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "channel.lifecycle").await?;

    // 1. Send Hello as initiator
    send_hello(&transport).await?;

    // 2. Receive Hello response
    recv_hello(&transport).await?;

    // The harness will:
    // 1. Send OpenChannel with even ID (as acceptor)
    // 2. Send DATA|EOS
    // 3. Expect DATA|EOS response

    // We need to receive the OpenChannel and respond appropriately
    // Let's receive frames and respond

    // Receive OpenChannel
    let frame = transport
        .recv_frame()
        .await
        .map_err(|e| format!("failed to receive OpenChannel: {}", e))?;

    if frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return Err(format!(
            "expected OpenChannel, got method_id={}",
            frame.desc.method_id
        ));
    }

    let open: OpenChannel = facet_format_postcard::from_slice(frame.payload_bytes())
        .map_err(|e| format!("failed to decode OpenChannel: {}", e))?;

    let channel_id = open.channel_id;
    trace!(channel_id, "received OpenChannel from harness");

    // Receive DATA|EOS
    let frame = transport
        .recv_frame()
        .await
        .map_err(|e| format!("failed to receive data: {}", e))?;

    if frame.desc.channel_id != channel_id {
        return Err(format!(
            "data on wrong channel: expected {}, got {}",
            channel_id, frame.desc.channel_id
        ));
    }

    if !frame.desc.flags.contains(FrameFlags::EOS) {
        return Err("expected EOS flag in request".to_string());
    }

    trace!("received DATA|EOS, sending response");

    // Send DATA|EOS response
    let mut desc = MsgDescHot::new();
    desc.msg_id = 10;
    desc.channel_id = channel_id;
    desc.method_id = 0;
    desc.flags = FrameFlags::RESPONSE | FrameFlags::DATA | FrameFlags::EOS;

    let response_payload = b"response";
    desc.payload_slot = INLINE_PAYLOAD_SLOT;
    desc.payload_len = response_payload.len() as u32;
    desc.inline_payload[..response_payload.len()].copy_from_slice(response_payload);

    let response_frame = Frame {
        desc,
        payload: Payload::Inline,
    };

    transport
        .send_frame(response_frame)
        .await
        .map_err(|e| format!("failed to send response: {}", e))?;

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run channel.close_semantics test.
///
/// Tests that CloseChannel is unilateral - no ack required.
async fn run_channel_close_semantics(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "channel.close_semantics").await?;

    // 1. Send Hello as initiator
    send_hello(&transport).await?;

    // 2. Receive Hello response
    recv_hello(&transport).await?;

    // The harness will:
    // 1. Send OpenChannel
    // 2. Send CloseChannel
    // No response expected - CloseChannel is unilateral

    // Just let it run - the harness passes if we don't break

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run control.ping_pong test.
///
/// The harness sends Ping after handshake. We need to run the session to handle it.
async fn run_control_ping_pong(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "control.ping_pong").await?;

    // 1. Send Hello as initiator
    send_hello(&transport).await?;

    // 2. Receive Hello from harness
    recv_hello(&transport).await?;

    // 3. Create RpcSession and run it - the session handles Ping/Pong automatically
    let session = Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    // Wait for harness to complete (it will send Ping, we respond with Pong)
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    session.close();
    run_handle.abort();

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run control.unknown_extension_verb test.
///
/// The harness sends unknown control verb (200+), then Ping. We should ignore the
/// unknown verb and respond to Ping with Pong.
async fn run_control_unknown_extension_verb(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "control.unknown_extension_verb").await?;

    // 1. Send Hello as initiator
    send_hello(&transport).await?;

    // 2. Receive Hello from harness
    recv_hello(&transport).await?;

    // 3. Create RpcSession and run it - the session handles control frames
    let session = Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    // Wait for harness to complete
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    session.close();
    run_handle.abort();

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run cancel.cancel_idempotent test.
///
/// The harness sends CancelChannel twice for the same (non-existent) channel,
/// then Ping. We should accept both CancelChannels and respond to Ping.
async fn run_cancel_idempotent(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "cancel.cancel_idempotent").await?;

    // 1. Send Hello as initiator
    send_hello(&transport).await?;

    // 2. Receive Hello from harness
    recv_hello(&transport).await?;

    // 3. Create RpcSession and run it
    let session = Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    // Wait for harness to complete
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    session.close();
    run_handle.abort();

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run cancel.cancel_impl_idempotent test (same as cancel.cancel_idempotent).
async fn run_cancel_impl_idempotent(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "cancel.cancel_impl_idempotent").await?;

    send_hello(&transport).await?;
    recv_hello(&transport).await?;

    let session = Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    session.close();
    run_handle.abort();

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run cancel.cancel_impl_support test.
///
/// The harness sends CancelChannel for a non-existent channel and expects
/// the connection to stay open.
async fn run_cancel_impl_support(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "cancel.cancel_impl_support").await?;

    send_hello(&transport).await?;
    recv_hello(&transport).await?;

    let session = Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    session.close();
    run_handle.abort();

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run channel.id_zero_reserved test.
///
/// The harness sends OpenChannel with channel_id=0. We should respond with
/// CancelChannel to reject it.
async fn run_channel_id_zero_reserved(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "channel.id_zero_reserved").await?;

    send_hello(&transport).await?;
    recv_hello(&transport).await?;

    let session = Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    session.close();
    run_handle.abort();

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Run channel.open_required_before_data test.
///
/// The harness sends data on an unopened channel. We should respond with
/// CancelChannel or GoAway.
async fn run_channel_open_required_before_data(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "channel.open_required_before_data").await?;

    send_hello(&transport).await?;
    recv_hello(&transport).await?;

    let session = Arc::new(RpcSession::new(transport));
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    session.close();
    run_handle.abort();

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

/// Generic test runner for tests that just need Hello exchange.
/// The harness validates the frames we send.
async fn run_hello_based_test(bin_path: &str, test_name: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, test_name).await?;

    // Send Hello as initiator
    send_hello(&transport).await?;

    // For most tests, the harness will validate our Hello and pass/fail
    // Some tests may send Hello back, try to receive it
    let _ = recv_hello(&transport).await;

    // Close transport and wait for harness
    transport.close();

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("conformance test failed: {}", stderr))
    }
}

fn main() {
    let args = Arguments::from_args();

    // Get the path to the conformance binary
    let conformance_bin = env!("CARGO_BIN_EXE_rapace-conformance");

    let mut trials = Vec::new();

    // Macro to reduce boilerplate
    macro_rules! add_test {
        ($name:expr, $func:ident) => {{
            let bin_path = conformance_bin.to_string();
            trials.push(Trial::test($name, move || {
                let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
                rt.block_on(async { $func(&bin_path).await.map_err(Failed::from) })
            }));
        }};
    }

    // Macro for tests that just need Hello exchange - the harness validates the frames we send
    macro_rules! add_hello_test {
        ($name:expr) => {{
            let bin_path = conformance_bin.to_string();
            let test_name = $name.to_string();
            trials.push(Trial::test($name, move || {
                let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
                rt.block_on(async {
                    run_hello_based_test(&bin_path, &test_name)
                        .await
                        .map_err(Failed::from)
                })
            }));
        }};
    }

    add_test!(
        "handshake.valid_hello_exchange",
        run_handshake_valid_hello_exchange
    );
    add_test!("call.one_req_one_resp", run_call_one_req_one_resp);
    add_test!("call.request_flags", run_call_request_flags);
    add_test!("call.response_msg_id_echo", run_call_response_msg_id_echo);
    add_test!("call.request_payload", run_call_request_payload);
    add_test!("call.response_payload", run_call_response_payload);
    add_test!("call.call_complete", run_call_call_complete);
    add_test!("call.error_flag_match", run_call_error_flag_match);
    add_test!("call.unknown_method", run_call_unknown_method);
    add_test!(
        "call.response_method_id_must_match",
        run_call_response_method_id_must_match
    );
    add_test!(
        "control.flag_set_on_channel_zero",
        run_control_flag_set_on_channel_zero
    );
    add_test!(
        "control.flag_clear_on_other_channels",
        run_control_flag_clear_on_other_channels
    );

    // Channel tests
    // NOTE: channel.parity_acceptor_even is skipped - harness blocks on try_recv
    // and we can't close the transport to unblock it
    add_test!("channel.lifecycle", run_channel_lifecycle);
    add_test!("channel.close_semantics", run_channel_close_semantics);

    // Frame format tests - these validate frame structure
    add_hello_test!("frame.descriptor_size");
    add_hello_test!("frame.inline_payload_max");
    add_hello_test!("frame.sentinel_inline");
    add_hello_test!("frame.sentinel_no_deadline");
    add_hello_test!("frame.encoding_little_endian");
    add_hello_test!("frame.msg_id_control");
    add_hello_test!("frame.msg_id_stream_tunnel");
    add_hello_test!("frame.msg_id_scope");

    // Data encoding tests
    add_hello_test!("data.determinism_map_order");
    add_hello_test!("data.unsupported_borrowed_return");
    add_hello_test!("data.unsupported_unions");
    add_hello_test!("data.service_facet_required");
    add_hello_test!("data.wire_non_self_describing");
    add_hello_test!("data.wire_field_order");
    add_hello_test!("data.unsupported_usize");
    add_hello_test!("data.type_system_additional");
    add_hello_test!("data.float_encoding");
    add_hello_test!("data.unsupported_self_ref");
    add_hello_test!("data.float_negative_zero");
    add_hello_test!("data.unsupported_pointers");
    add_hello_test!("data.float_nan_canonicalization");

    // More call tests
    add_hello_test!("call.response_flags");
    add_hello_test!("call.call_optional_ports");
    add_hello_test!("call.call_required_port_missing");
    add_hello_test!("call.request_method_id");

    // More channel tests
    add_hello_test!("channel.kind_immutable");
    add_hello_test!("channel.parity_initiator_odd");
    add_hello_test!("channel.id_allocation_monotonic");
    add_hello_test!("channel.open_cancel_on_violation");
    add_hello_test!("channel.close_full");
    add_test!(
        "channel.open_required_before_data",
        run_channel_open_required_before_data
    );
    add_hello_test!("channel.id_no_reuse");
    add_hello_test!("channel.parity_acceptor_even");
    add_hello_test!("channel.open_call_validation");
    add_hello_test!("channel.open_attach_validation");
    add_test!("channel.id_zero_reserved", run_channel_id_zero_reserved);
    add_hello_test!("channel.open_ownership");
    add_hello_test!("channel.goaway_after_send");
    add_hello_test!("channel.close_state_free");
    add_hello_test!("channel.eos_after_send");
    add_hello_test!("channel.flags_reserved");
    add_hello_test!("channel.control_reserved");
    add_hello_test!("channel.open_no_pre_open");

    // Transport tests
    add_hello_test!("transport.stream_max_length");
    add_hello_test!("transport.keepalive_transport");
    add_hello_test!("transport.framing_no_coalesce");
    add_hello_test!("transport.stream_length_match");
    add_hello_test!("transport.reliable_delivery");
    add_hello_test!("transport.stream_varint_canonical");
    add_hello_test!("transport.shutdown_orderly");
    add_hello_test!("transport.framing_boundaries");
    add_hello_test!("transport.stream_varint_limit");
    add_hello_test!("transport.buffer_pool");
    add_hello_test!("transport.stream_validation");
    add_hello_test!("transport.ordering_single");
    add_hello_test!("transport.webtransport_datagram_restrictions");
    add_hello_test!("transport.stream_size_limits");
    add_hello_test!("transport.webtransport_server_requirements");
    add_hello_test!("transport.backpressure");
    add_hello_test!("transport.stream_min_length");
    add_hello_test!("transport.ordering_channel");

    // Error handling tests
    add_hello_test!("error.impl_error_flag");
    add_hello_test!("error.details_unknown_format");
    add_hello_test!("error.cancel_reasons");
    add_hello_test!("error.protocol_codes");
    add_hello_test!("error.impl_details");
    add_hello_test!("error.status_success");
    add_hello_test!("error.impl_status_required");
    add_hello_test!("error.details_populate");
    add_hello_test!("error.flag_parse");
    add_hello_test!("error.impl_custom_codes");
    add_hello_test!("error.impl_unknown_codes");
    add_hello_test!("error.impl_backoff");
    add_hello_test!("error.status_error");
    add_hello_test!("error.status_codes");

    // Schema tests
    add_hello_test!("schema.encoding_order");
    add_hello_test!("schema.compat_check");
    add_hello_test!("schema.encoding_endianness");
    add_hello_test!("schema.encoding_lengths");
    add_hello_test!("schema.collision_runtime");
    add_hello_test!("schema.identifier_normalization");
    add_hello_test!("schema.compat_rejection");
    add_hello_test!("schema.hash_cross_language");
    add_hello_test!("schema.collision_detection");
    add_hello_test!("schema.hash_algorithm");

    // Tunnel tests
    add_hello_test!("tunnel.ordering");
    add_hello_test!("tunnel.semantics");
    add_hello_test!("tunnel.raw_bytes");
    add_hello_test!("tunnel.reliability");
    add_hello_test!("tunnel.intro");
    add_hello_test!("tunnel.channel_kind");
    add_hello_test!("tunnel.credits");
    add_hello_test!("tunnel.frame_boundaries");

    // Priority tests
    add_hello_test!("priority.guarantee_starvation");
    add_hello_test!("priority.high_flag_mapping");
    add_hello_test!("priority.non_guarantee");
    add_hello_test!("priority.value_default");
    add_hello_test!("priority.guarantee_deadline");
    add_hello_test!("priority.scheduling_queue");
    add_hello_test!("priority.value_range");
    add_hello_test!("priority.credits_minimum");
    add_hello_test!("priority.propagation_rules");
    add_hello_test!("priority.precedence");
    add_hello_test!("priority.guarantee_ordering");

    // Language mapping tests
    add_hello_test!("langmap.idiomatic");
    add_hello_test!("langmap.java_unsigned");
    add_hello_test!("langmap.lossy");
    add_hello_test!("langmap.i128_swift");
    add_hello_test!("langmap.roundtrip");
    add_hello_test!("langmap.usize_prohibited");
    add_hello_test!("langmap.semantic");
    add_hello_test!("langmap.enum_discriminant");

    // Cancellation tests
    add_test!("cancel.cancel_impl_idempotent", run_cancel_impl_idempotent);
    add_hello_test!("cancel.cancel_ordering_handle");
    add_hello_test!("cancel.deadline_rounding");
    add_hello_test!("cancel.cancel_shm_reclaim");
    add_hello_test!("cancel.deadline_shm");
    add_hello_test!("cancel.deadline_exceeded");
    add_hello_test!("cancel.cancel_impl_check_deadline");
    add_hello_test!("cancel.cancel_impl_shm_free");
    add_hello_test!("cancel.deadline_terminal");
    add_hello_test!("cancel.cancel_ordering");
    add_hello_test!("cancel.cancel_propagation");
    add_test!("cancel.cancel_impl_support", run_cancel_impl_support);
    add_hello_test!("cancel.reason_values");
    add_hello_test!("cancel.deadline_expired");
    add_hello_test!("cancel.cancel_impl_error_response");
    add_hello_test!("cancel.cancel_impl_ignore_data");
    add_hello_test!("cancel.deadline_field");
    add_test!("cancel.cancel_idempotent", run_cancel_idempotent);
    add_hello_test!("cancel.deadline_clock");
    add_hello_test!("cancel.deadline_stream");
    add_hello_test!("cancel.cancel_precedence");

    // Payload tests
    add_hello_test!("payload.varint_canonical");
    add_hello_test!("payload.map_nondeterministic");
    add_hello_test!("payload.float_nan");
    add_hello_test!("payload.encoding_scope");
    add_hello_test!("payload.struct_order_immutable");
    add_hello_test!("payload.varint_reject_noncanonical");
    add_hello_test!("payload.encoding_tunnel_exception");
    add_hello_test!("payload.stability_frozen");
    add_hello_test!("payload.float_negzero");
    add_hello_test!("payload.stability_canonical");
    add_hello_test!("payload.struct_field_order");

    // Security tests
    add_hello_test!("security.metadata_plaintext");
    add_hello_test!("security.auth_failure_handshake");
    add_hello_test!("security.profile_c_reject");

    // Method ID tests
    add_hello_test!("method.fnv1a_properties");
    add_hello_test!("method.zero_reserved");
    add_hello_test!("method.algorithm");
    add_hello_test!("method.input_format");
    add_hello_test!("method.intro");
    add_hello_test!("method.zero_enforcement");
    add_hello_test!("method.collision_detection");

    // Overload tests
    add_hello_test!("overload.drain_after_grace");
    add_hello_test!("overload.goaway_drain");
    add_hello_test!("overload.goaway_new_rejected");
    add_hello_test!("overload.goaway_no_new");
    add_hello_test!("overload.limits_response");
    add_hello_test!("overload.retry_retry_after");
    add_hello_test!("overload.goaway_existing");
    add_hello_test!("overload.drain_grace_period");
    add_hello_test!("overload.retry_retryable");

    // Stream tests
    add_hello_test!("stream.attachment_required");
    add_hello_test!("stream.method_id_zero");
    add_hello_test!("stream.intro");
    add_hello_test!("stream.type_enforcement");
    add_hello_test!("stream.direction_values");
    add_hello_test!("stream.frame_payload");
    add_hello_test!("stream.channel_kind");
    add_hello_test!("stream.empty");
    add_hello_test!("stream.decode_failure");
    add_hello_test!("stream.ordering");

    // More handshake tests
    add_hello_test!("handshake.timeout");
    add_hello_test!("handshake.role_conflict");
    // NOTE: handshake.missing_hello is NOT run because it's a meta-test that
    // expects the implementation to send an invalid first frame (not Hello).
    // A compliant implementation correctly sends Hello, so this test would fail.
    // This test validates the harness can detect violations, not the implementation.
    add_hello_test!("handshake.explicit_required");
    add_hello_test!("handshake.version_mismatch");
    add_hello_test!("handshake.required_features_missing");
    add_hello_test!("handshake.method_registry_duplicate");
    add_hello_test!("handshake.registry_failure");
    add_hello_test!("handshake.registry_cross_service");
    add_hello_test!("handshake.method_registry_zero");

    // Metadata tests
    add_hello_test!("metadata.key_duplicates");
    add_hello_test!("metadata.limits");
    add_hello_test!("metadata.limits_reject");
    add_hello_test!("metadata.key_case_sensitive");
    add_hello_test!("metadata.key_format");
    add_hello_test!("metadata.key_reserved_prefix");
    add_hello_test!("metadata.key_lowercase");

    // Flow control tests
    add_hello_test!("flow.credit_in_flags");
    add_hello_test!("flow.intro");
    add_hello_test!("flow.credit_additive");
    add_hello_test!("flow.infinite_credit");
    add_hello_test!("flow.eos_no_credits");
    add_hello_test!("flow.credit_overrun");

    // More control tests
    add_hello_test!("control.unknown_reserved_verb");
    add_hello_test!("control.goaway_last_channel_id");
    add_test!("control.ping_pong", run_control_ping_pong);
    add_test!(
        "control.unknown_extension_verb",
        run_control_unknown_extension_verb
    );

    libtest_mimic::run(&args, trials).exit();
}
