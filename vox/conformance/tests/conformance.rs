//! Conformance tests using libtest-mimic.
//!
//! This test harness runs all conformance tests from the rapace-conformance binary.
//! It acts as the "implementation under test" - spawning the conformance binary
//! and communicating with it via stdin/stdout pipes.

use facet::Facet;
use libtest_mimic::{Arguments, Failed, Trial};
use rapace_protocol::{
    CallResult, ChannelKind, Hello, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, Limits, MethodInfo,
    MsgDescHot, OpenChannel, PROTOCOL_VERSION_1_0, Role, Status, compute_method_id, control_verb,
    features, flags,
};
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};

/// Test case from the conformance binary.
#[derive(Facet)]
struct TestCase {
    name: String,
    rules: Vec<String>,
}

/// A frame for transmission.
#[derive(Debug, Clone)]
struct Frame {
    desc: MsgDescHot,
    payload: Vec<u8>,
}

impl Frame {
    /// Create a new frame with inline payload.
    fn inline(mut desc: MsgDescHot, payload: &[u8]) -> Self {
        assert!(
            payload.len() <= INLINE_PAYLOAD_SIZE,
            "payload too large for inline"
        );
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(payload);
        Self {
            desc,
            payload: Vec::new(),
        }
    }

    /// Create a new frame with external payload.
    fn with_payload(mut desc: MsgDescHot, payload: Vec<u8>) -> Self {
        desc.payload_slot = 0;
        desc.payload_len = payload.len() as u32;
        Self { desc, payload }
    }

    /// Get payload bytes.
    fn payload_bytes(&self) -> &[u8] {
        if self.desc.payload_slot == INLINE_PAYLOAD_SLOT {
            &self.desc.inline_payload[..self.desc.payload_len as usize]
        } else {
            &self.payload
        }
    }
}

/// Conformance test runner that communicates with the rapace-conformance binary.
///
/// This acts as the "implementation under test" - it spawns the conformance
/// binary and talks to it via stdin/stdout pipes.
struct ConformanceRunner {
    child: Child,
    next_msg_id: u64,
}

impl ConformanceRunner {
    /// Start a conformance test case.
    fn start(bin_path: &str, test_case: &str) -> Result<Self, String> {
        let child = Command::new(bin_path)
            .args(["--case", test_case])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("failed to spawn conformance binary: {}", e))?;

        Ok(Self {
            child,
            next_msg_id: 1,
        })
    }

    /// Get the next message ID.
    fn next_msg_id(&mut self) -> u64 {
        let id = self.next_msg_id;
        self.next_msg_id += 1;
        id
    }

    /// Send a frame to the conformance harness.
    fn send(&mut self, frame: &Frame) -> Result<(), String> {
        let stdin = self.child.stdin.as_mut().ok_or("stdin not available")?;

        let external_payload = if frame.desc.payload_slot == INLINE_PAYLOAD_SLOT {
            &[] as &[u8]
        } else {
            &frame.payload
        };

        let total_len = 64 + external_payload.len();

        // Write length prefix
        stdin
            .write_all(&(total_len as u32).to_le_bytes())
            .map_err(|e| format!("failed to write length: {}", e))?;

        // Write descriptor
        stdin
            .write_all(&frame.desc.to_bytes())
            .map_err(|e| format!("failed to write descriptor: {}", e))?;

        // Write external payload if present
        if !external_payload.is_empty() {
            stdin
                .write_all(external_payload)
                .map_err(|e| format!("failed to write payload: {}", e))?;
        }

        stdin
            .flush()
            .map_err(|e| format!("failed to flush: {}", e))?;

        Ok(())
    }

    /// Receive a frame from the conformance harness.
    fn recv(&mut self) -> Result<Frame, String> {
        let stdout = self.child.stdout.as_mut().ok_or("stdout not available")?;

        // Read length prefix
        let mut len_buf = [0u8; 4];
        stdout
            .read_exact(&mut len_buf)
            .map_err(|e| format!("failed to read length: {}", e))?;
        let total_len = u32::from_le_bytes(len_buf) as usize;

        if total_len < 64 {
            return Err(format!("frame too short: {} bytes", total_len));
        }

        // Read descriptor
        let mut desc_buf = [0u8; 64];
        stdout
            .read_exact(&mut desc_buf)
            .map_err(|e| format!("failed to read descriptor: {}", e))?;
        let desc = MsgDescHot::from_bytes(&desc_buf);

        // Read external payload if present
        let payload = if total_len > 64 {
            let mut payload = vec![0u8; total_len - 64];
            stdout
                .read_exact(&mut payload)
                .map_err(|e| format!("failed to read payload: {}", e))?;
            payload
        } else {
            Vec::new()
        };

        Ok(Frame { desc, payload })
    }

    /// Try to receive a frame, returning None on EOF.
    fn try_recv(&mut self) -> Result<Option<Frame>, String> {
        let stdout = self.child.stdout.as_mut().ok_or("stdout not available")?;

        // Read length prefix
        let mut len_buf = [0u8; 4];
        match stdout.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(format!("failed to read length: {}", e)),
        }

        let total_len = u32::from_le_bytes(len_buf) as usize;

        if total_len < 64 {
            return Err(format!("frame too short: {} bytes", total_len));
        }

        // Read descriptor
        let mut desc_buf = [0u8; 64];
        stdout
            .read_exact(&mut desc_buf)
            .map_err(|e| format!("failed to read descriptor: {}", e))?;
        let desc = MsgDescHot::from_bytes(&desc_buf);

        // Read external payload if present
        let payload = if total_len > 64 {
            let mut payload = vec![0u8; total_len - 64];
            stdout
                .read_exact(&mut payload)
                .map_err(|e| format!("failed to read payload: {}", e))?;
            payload
        } else {
            Vec::new()
        };

        Ok(Some(Frame { desc, payload }))
    }

    /// Send Hello as initiator (don't wait for response).
    fn send_hello_as_initiator(&mut self) -> Result<(), String> {
        let hello = Hello {
            protocol_version: PROTOCOL_VERSION_1_0,
            role: Role::Initiator,
            required_features: 0,
            supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
            limits: Limits::default(),
            methods: vec![],
            params: Vec::new(),
        };

        let payload = facet_format_postcard::to_vec(&hello)
            .map_err(|e| format!("failed to encode Hello: {}", e))?;

        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = 0;
        desc.method_id = control_verb::HELLO;
        desc.flags = flags::CONTROL;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::inline(desc, &payload)
        } else {
            Frame::with_payload(desc, payload)
        };

        self.send(&frame)
    }

    /// Perform handshake as initiator (send Hello, receive Hello response).
    fn do_handshake_as_initiator(&mut self) -> Result<(), String> {
        self.send_hello_as_initiator()?;

        // Wait for Hello response
        let response = self.recv()?;

        if response.desc.channel_id != 0 {
            return Err(format!(
                "expected Hello on channel 0, got channel {}",
                response.desc.channel_id
            ));
        }

        if response.desc.method_id != control_verb::HELLO {
            return Err(format!(
                "expected Hello (method_id=0), got method_id={}",
                response.desc.method_id
            ));
        }

        Ok(())
    }

    /// Perform handshake as acceptor (receive Hello, send Hello response).
    fn do_handshake_as_acceptor(&mut self) -> Result<(), String> {
        // Wait for Hello from harness
        let frame = self.recv()?;

        if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
            return Err("expected Hello as first frame".to_string());
        }

        // Send Hello response
        let hello = Hello {
            protocol_version: PROTOCOL_VERSION_1_0,
            role: Role::Acceptor,
            required_features: 0,
            supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
            limits: Limits::default(),
            methods: vec![MethodInfo {
                method_id: compute_method_id("Test", "echo"),
                sig_hash: [0u8; 32],
                name: Some("Test.echo".to_string()),
            }],
            params: Vec::new(),
        };

        let payload = facet_format_postcard::to_vec(&hello)
            .map_err(|e| format!("failed to encode Hello: {}", e))?;

        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = 0;
        desc.method_id = control_verb::HELLO;
        desc.flags = flags::CONTROL;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::inline(desc, &payload)
        } else {
            Frame::with_payload(desc, payload)
        };

        self.send(&frame)?;
        Ok(())
    }

    /// Wait for the conformance process to finish and get exit status.
    fn finish(mut self) -> (bool, Option<i32>) {
        // Close stdin to signal EOF
        drop(self.child.stdin.take());

        match self.child.wait() {
            Ok(status) => (status.success(), status.code()),
            Err(_) => (false, None),
        }
    }
}

/// Determine what role we should play for a given test.
/// Some tests expect us to be initiator, others expect us to be acceptor.
fn get_test_role(test_name: &str) -> TestRole {
    // Tests where the harness acts as initiator (we are acceptor):
    // - call.response_method_id_must_match: harness calls us, we respond
    if test_name == "call.response_method_id_must_match" {
        return TestRole::Acceptor;
    }

    // Tests where the harness acts as acceptor (we are initiator):
    // - handshake.*: we send Hello, harness responds (except missing_hello)
    // - call.*: harness does do_handshake() which waits for our Hello, then we make calls
    // - cancel.*: similar to call.*
    // - channel.*: harness waits for our Hello and operations
    // - control.*: harness waits for our Hello
    if test_name.starts_with("handshake.") && test_name != "handshake.missing_hello" {
        return TestRole::Initiator;
    }

    if test_name.starts_with("call.") {
        return TestRole::Initiator;
    }

    if test_name.starts_with("cancel.") {
        return TestRole::Initiator;
    }

    if test_name.starts_with("channel.") {
        return TestRole::Initiator;
    }

    if test_name.starts_with("control.") {
        return TestRole::Initiator;
    }

    // Non-interactive tests don't need a role
    TestRole::Acceptor
}

/// Tests that only send Hello and don't expect a response.
/// These tests just check properties of our Hello frame.
fn is_hello_only_test(test_name: &str) -> bool {
    matches!(
        test_name,
        "control.flag_set_on_channel_zero" | "handshake.missing_hello"
    )
}

enum TestRole {
    Initiator,
    Acceptor,
}

/// Run a conformance test with proper interaction.
fn run_interactive_test(runner: &mut ConformanceRunner, test_name: &str) -> Result<(), String> {
    // Special case: tests that only check our Hello frame don't expect a response
    if is_hello_only_test(test_name) {
        // Just send Hello, don't wait for response
        runner.send_hello_as_initiator()?;
        return Ok(());
    }

    let role = get_test_role(test_name);

    match role {
        TestRole::Initiator => {
            // We send Hello first
            runner.do_handshake_as_initiator()?;

            // Then run test-specific logic
            run_test_as_initiator(runner, test_name)?;
        }
        TestRole::Acceptor => {
            // Harness sends Hello first (or the test doesn't need handshake)
            // Try to do handshake, but don't fail if it doesn't work
            // (some tests intentionally break the handshake)
            let _ = runner.do_handshake_as_acceptor();

            // Then run test-specific logic
            run_test_as_acceptor(runner, test_name)?;
        }
    }

    Ok(())
}

/// Send a CALL request as initiator without expecting a response.
fn do_call_no_response(
    runner: &mut ConformanceRunner,
    channel_id: u32,
    method_id: u32,
    request_payload: &[u8],
) -> Result<(), String> {
    // Send OpenChannel for a CALL
    let open = OpenChannel {
        channel_id,
        kind: ChannelKind::Call,
        attach: None,
        metadata: Vec::new(),
        initial_credits: 65536,
    };

    let payload = facet_format_postcard::to_vec(&open)
        .map_err(|e| format!("failed to encode OpenChannel: {}", e))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = runner.next_msg_id();
    desc.channel_id = 0;
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    runner.send(&frame)?;

    // Send the request
    let mut desc = MsgDescHot::new();
    desc.msg_id = runner.next_msg_id();
    desc.channel_id = channel_id;
    desc.method_id = method_id;
    desc.flags = flags::DATA | flags::EOS;

    let frame = if request_payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, request_payload)
    } else {
        Frame::with_payload(desc, request_payload.to_vec())
    };

    runner.send(&frame)?;

    Ok(())
}

/// Send a CALL request as initiator and receive response.
fn do_call(
    runner: &mut ConformanceRunner,
    channel_id: u32,
    method_id: u32,
    request_payload: &[u8],
) -> Result<Frame, String> {
    // Send OpenChannel for a CALL
    let open = OpenChannel {
        channel_id,
        kind: ChannelKind::Call,
        attach: None,
        metadata: Vec::new(),
        initial_credits: 65536,
    };

    let payload = facet_format_postcard::to_vec(&open)
        .map_err(|e| format!("failed to encode OpenChannel: {}", e))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = runner.next_msg_id();
    desc.channel_id = 0;
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    runner.send(&frame)?;

    // Send the request
    let mut desc = MsgDescHot::new();
    desc.msg_id = runner.next_msg_id();
    desc.channel_id = channel_id;
    desc.method_id = method_id;
    desc.flags = flags::DATA | flags::EOS;

    let frame = if request_payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, request_payload)
    } else {
        Frame::with_payload(desc, request_payload.to_vec())
    };

    runner.send(&frame)?;

    // Receive response
    runner.recv()
}

/// Run test-specific logic when we are the initiator.
fn run_test_as_initiator(runner: &mut ConformanceRunner, test_name: &str) -> Result<(), String> {
    let method_id = compute_method_id("Test", "echo");
    let channel_id = 1u32; // Odd for initiator

    match test_name {
        // Call tests where harness sends response
        "call.one_req_one_resp"
        | "call.response_msg_id_echo"
        | "call.error_flag_match"
        | "call.response_method_id_must_match" => {
            let _response = do_call(runner, channel_id, method_id, b"test request")?;
        }

        // Call tests where harness just validates our request (no response expected initially)
        "call.request_flags" => {
            do_call_no_response(runner, channel_id, method_id, b"test request")?;
        }

        // Call tests where harness validates our request and sends response
        "call.request_payload" | "call.response_payload" | "call.call_complete" => {
            let _response = do_call(runner, channel_id, method_id, b"test request payload")?;
        }

        // Tests that just verify constants/semantics - no interaction needed
        "call.call_optional_ports" | "call.call_required_port_missing" => {
            // These tests just verify semantic rules about port numbering
            // No interaction needed - harness passes after verifying constants
        }

        "call.unknown_method" => {
            // Call with an unknown method_id - harness should respond UNIMPLEMENTED
            let unknown_method_id = compute_method_id("Unknown", "method");
            let _response = do_call(runner, channel_id, unknown_method_id, b"test request")?;
        }

        // Cancel tests - make a call then cancel or let it timeout
        "cancel.cancel_idempotent"
        | "cancel.cancel_impl_support"
        | "cancel.cancel_impl_idempotent" => {
            // Just make a call for now
            let _response = do_call(runner, channel_id, method_id, b"test request")?;
        }

        // Cancel tests that just verify semantic rules - no interaction needed
        "cancel.cancel_impl_check_deadline"
        | "cancel.cancel_impl_error_response"
        | "cancel.cancel_impl_ignore_data"
        | "cancel.cancel_impl_shm_free" => {
            // These tests just verify semantic rules about cancellation
            // No interaction needed - harness passes after verifying constants
        }

        // Channel tests - various channel operations
        "channel.close_semantics" => {
            // Harness does handshake, opens channel 2, sends CloseChannel.
            // CloseChannel is unilateral - no ack needed.
            // We receive but don't need to respond.
            let _open = runner.recv()?; // OpenChannel
            let _close = runner.recv()?; // CloseChannel
        }

        "channel.goaway_after_send" => {
            // Harness does handshake, sends GoAway.
            // After GoAway, new channels should be rejected.
            let _goaway = runner.recv()?; // GoAway
        }

        "channel.flags_reserved" => {
            // Harness does handshake, then just validates constant values.
            // No additional interaction needed.
        }

        "channel.id_allocation_monotonic" => {
            // Harness does handshake, then waits for multiple OpenChannels from us.
            // We need to open multiple channels with monotonically increasing IDs.
            for i in 0..3 {
                let open = rapace_protocol::OpenChannel {
                    channel_id: 1 + (i * 2) as u32, // 1, 3, 5 (odd for initiator)
                    kind: rapace_protocol::ChannelKind::Call,
                    attach: None,
                    metadata: Vec::new(),
                    initial_credits: 65536,
                };

                let payload = facet_format_postcard::to_vec(&open)
                    .map_err(|e| format!("failed to encode OpenChannel: {}", e))?;

                let mut desc = MsgDescHot::new();
                desc.msg_id = runner.next_msg_id();
                desc.channel_id = 0;
                desc.method_id = control_verb::OPEN_CHANNEL;
                desc.flags = flags::CONTROL;

                let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                    Frame::inline(desc, &payload)
                } else {
                    Frame::with_payload(desc, payload)
                };

                runner.send(&frame)?;
            }
        }

        "channel.id_zero_reserved" => {
            // Harness does handshake, sends OpenChannel with channel_id=0,
            // expects us to reject with CancelChannel.
            let _frame = runner.recv()?; // OpenChannel with channel_id=0

            // Send CancelChannel to reject
            let cancel = rapace_protocol::CancelChannel {
                channel_id: 0,
                reason: rapace_protocol::CancelReason::ProtocolViolation,
            };
            let payload = facet_format_postcard::to_vec(&cancel)
                .map_err(|e| format!("failed to encode CancelChannel: {}", e))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = runner.next_msg_id();
            desc.channel_id = 0;
            desc.method_id = control_verb::CANCEL_CHANNEL;
            desc.flags = flags::CONTROL;

            let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::inline(desc, &payload)
            } else {
                Frame::with_payload(desc, payload)
            };

            runner.send(&frame)?;
        }

        "channel.lifecycle" => {
            // Harness does handshake, opens channel 2, sends DATA|EOS,
            // expects response with EOS.
            let _open = runner.recv()?; // OpenChannel
            let request = runner.recv()?; // DATA|EOS

            // Send response with DATA|EOS|RESPONSE
            let result = CallResult {
                status: Status::ok(),
                trailers: Vec::new(),
                body: Some(b"response".to_vec()),
            };

            let payload = facet_format_postcard::to_vec(&result)
                .map_err(|e| format!("failed to encode CallResult: {}", e))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = request.desc.msg_id;
            desc.channel_id = 2; // Same channel they opened
            desc.method_id = request.desc.method_id;
            desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

            let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::inline(desc, &payload)
            } else {
                Frame::with_payload(desc, payload)
            };

            runner.send(&frame)?;
        }

        "channel.parity_acceptor_even" => {
            // Harness does handshake, sends OpenChannel with even ID (2).
            // We should accept it (even is valid from acceptor).
            let _open = runner.recv()?; // OpenChannel with even channel_id
            // Don't send CancelChannel - that would mean rejection
        }

        "channel.parity_initiator_odd" => {
            // Harness does handshake, waits for us to open a channel.
            // We (initiator) must use odd IDs.
            // Harness just checks our channel ID then exits - no response.
            do_call_no_response(runner, channel_id, method_id, b"test request")?;
        }

        "channel.open_required_before_data" => {
            // Harness does handshake, sends data on channel 99 (never opened).
            // We should reject with CancelChannel or GoAway.
            let _frame = runner.recv()?; // Data on unopened channel

            // Send CancelChannel to reject
            let cancel = rapace_protocol::CancelChannel {
                channel_id: 99,
                reason: rapace_protocol::CancelReason::ProtocolViolation,
            };
            let payload = facet_format_postcard::to_vec(&cancel)
                .map_err(|e| format!("failed to encode CancelChannel: {}", e))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = runner.next_msg_id();
            desc.channel_id = 0;
            desc.method_id = control_verb::CANCEL_CHANNEL;
            desc.flags = flags::CONTROL;

            let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::inline(desc, &payload)
            } else {
                Frame::with_payload(desc, payload)
            };

            runner.send(&frame)?;
        }

        // Control tests - ping/pong, goaway, etc.
        "control.ping_pong" => {
            // Harness sends Ping, we respond with Pong
            let ping_frame = runner.recv()?;

            if ping_frame.desc.method_id != control_verb::PING {
                return Err(format!(
                    "expected Ping, got method_id={}",
                    ping_frame.desc.method_id
                ));
            }

            // Extract ping payload and echo it in pong
            let ping: rapace_protocol::Ping =
                facet_format_postcard::from_slice(ping_frame.payload_bytes())
                    .map_err(|e| format!("failed to decode Ping: {}", e))?;

            let pong = rapace_protocol::Pong {
                payload: ping.payload,
            };
            let payload = facet_format_postcard::to_vec(&pong)
                .map_err(|e| format!("failed to encode Pong: {}", e))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = runner.next_msg_id();
            desc.channel_id = 0;
            desc.method_id = control_verb::PONG;
            desc.flags = flags::CONTROL;

            let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::inline(desc, &payload)
            } else {
                Frame::with_payload(desc, payload)
            };

            runner.send(&frame)?;
        }

        "control.goaway_last_channel_id" => {
            // Harness sends GoAway, we just need to receive it
            let frame = runner.recv()?;
            if frame.desc.method_id != control_verb::GO_AWAY {
                return Err(format!(
                    "expected GoAway, got method_id={}",
                    frame.desc.method_id
                ));
            }
            // Test passes if we received a valid GoAway
        }

        // control.flag_set_on_channel_zero is handled by is_hello_only_test()
        "control.flag_clear_on_other_channels" => {
            // Harness does handshake, then receives a frame on non-zero channel
            // and checks CONTROL flag is NOT set.
            // We need to send a data frame on a non-zero channel.
            // Don't wait for response - harness just checks our frames.
            do_call_no_response(runner, channel_id, method_id, b"test request")?;
        }

        "control.unknown_extension_verb" => {
            // Harness does handshake, then sends an unknown extension verb,
            // then sends Ping to verify connection is still alive.
            // We need to receive the unknown verb (silently ignore) and respond to Ping.

            // Receive the unknown extension verb
            let _unknown_frame = runner.recv()?;

            // Receive the Ping
            let ping_frame = runner.recv()?;
            if ping_frame.desc.method_id != control_verb::PING {
                return Err(format!(
                    "expected Ping after unknown extension verb, got method_id={}",
                    ping_frame.desc.method_id
                ));
            }

            // Respond with Pong
            let ping: rapace_protocol::Ping =
                facet_format_postcard::from_slice(ping_frame.payload_bytes())
                    .map_err(|e| format!("failed to decode Ping: {}", e))?;

            let pong = rapace_protocol::Pong {
                payload: ping.payload,
            };
            let payload = facet_format_postcard::to_vec(&pong)
                .map_err(|e| format!("failed to encode Pong: {}", e))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = runner.next_msg_id();
            desc.channel_id = 0;
            desc.method_id = control_verb::PONG;
            desc.flags = flags::CONTROL;

            let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::inline(desc, &payload)
            } else {
                Frame::with_payload(desc, payload)
            };

            runner.send(&frame)?;
        }

        "control.unknown_reserved_verb" => {
            // Harness does handshake, then sends an unknown reserved verb.
            // Implementation should send GoAway with ProtocolError.

            // Receive the unknown reserved verb
            let _unknown_frame = runner.recv()?;

            // Send GoAway
            let goaway = rapace_protocol::GoAway {
                reason: rapace_protocol::GoAwayReason::ProtocolError,
                last_channel_id: 0,
                message: "unknown reserved control verb".to_string(),
                metadata: Vec::new(),
            };
            let payload = facet_format_postcard::to_vec(&goaway)
                .map_err(|e| format!("failed to encode GoAway: {}", e))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = runner.next_msg_id();
            desc.channel_id = 0;
            desc.method_id = control_verb::GO_AWAY;
            desc.flags = flags::CONTROL;

            let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::inline(desc, &payload)
            } else {
                Frame::with_payload(desc, payload)
            };

            runner.send(&frame)?;
        }

        _ => {
            // Other initiator tests - just let them complete after handshake
        }
    }

    Ok(())
}

/// Receive a call as acceptor and send response.
fn handle_incoming_call(runner: &mut ConformanceRunner) -> Result<(), String> {
    // Receive OpenChannel
    let frame = runner.recv()?;
    if frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return Err(format!(
            "expected OpenChannel, got method_id={}",
            frame.desc.method_id
        ));
    }

    let open: OpenChannel = facet_format_postcard::from_slice(frame.payload_bytes())
        .map_err(|e| format!("failed to decode OpenChannel: {}", e))?;

    let channel_id = open.channel_id;

    // Receive the request
    let request = runner.recv()?;

    // Send response - echo msg_id and method_id
    let result = CallResult {
        status: Status::ok(),
        trailers: Vec::new(),
        body: Some(b"echo response".to_vec()),
    };

    let payload = facet_format_postcard::to_vec(&result)
        .map_err(|e| format!("failed to encode CallResult: {}", e))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id; // Echo msg_id
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id; // Echo method_id - THIS IS CRITICAL
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    runner.send(&frame)?;

    Ok(())
}

/// Run test-specific logic when we are the acceptor.
fn run_test_as_acceptor(runner: &mut ConformanceRunner, test_name: &str) -> Result<(), String> {
    match test_name {
        // call.response_method_id_must_match: harness calls us, we must echo method_id
        "call.response_method_id_must_match" => {
            handle_incoming_call(runner)?;
        }

        // Generic acceptor behavior - shouldn't reach here for properly categorized tests
        _ => {
            // Try to receive and handle what comes
            loop {
                match runner.try_recv() {
                    Ok(Some(frame)) => {
                        if frame.desc.channel_id == 0 {
                            match frame.desc.method_id {
                                control_verb::PING => {
                                    let mut desc = MsgDescHot::new();
                                    desc.msg_id = runner.next_msg_id();
                                    desc.channel_id = 0;
                                    desc.method_id = control_verb::PONG;
                                    desc.flags = flags::CONTROL;
                                    desc.inline_payload = frame.desc.inline_payload;
                                    desc.payload_len = frame.desc.payload_len;
                                    desc.payload_slot = INLINE_PAYLOAD_SLOT;

                                    let pong_frame = Frame {
                                        desc,
                                        payload: Vec::new(),
                                    };
                                    let _ = runner.send(&pong_frame);
                                }
                                control_verb::GO_AWAY => break,
                                _ => {}
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    Ok(())
}

/// Check if a test is interactive (needs stdin/stdout communication).
/// Non-interactive tests just validate constants, encoding, etc.
fn is_interactive_test(test_name: &str) -> bool {
    // Non-interactive tests that don't use peer
    let non_interactive = [
        // call tests that just validate constants
        "call.request_method_id",
        "call.response_flags",
        "call.call_optional_ports",
        "call.call_required_port_missing",
        // cancel tests that just validate constants
        "cancel.cancel_ordering",
        "cancel.cancel_ordering_handle",
        "cancel.cancel_precedence",
        "cancel.cancel_propagation",
        "cancel.cancel_shm_reclaim",
        "cancel.deadline_clock",
        "cancel.deadline_exceeded",
        "cancel.deadline_expired",
        "cancel.deadline_field",
        "cancel.deadline_rounding",
        "cancel.deadline_shm",
        "cancel.deadline_stream",
        "cancel.deadline_terminal",
        "cancel.reason_values",
        // New cancel tests that just validate semantic rules
        "cancel.cancel_impl_check_deadline",
        "cancel.cancel_impl_error_response",
        "cancel.cancel_impl_ignore_data",
        "cancel.cancel_impl_shm_free",
        // channel tests that just validate constants
        "channel.control_reserved",
        "channel.eos_after_send",
        "channel.id_no_reuse",
        "channel.kind_immutable",
    ];

    if non_interactive.contains(&test_name) {
        return false;
    }

    test_name.starts_with("handshake.")
        || test_name.starts_with("call.")
        || test_name.starts_with("channel.")
        || test_name.starts_with("control.")
        || test_name.starts_with("cancel.")
}

/// Check if a test is a "negative test" that expects the implementation to misbehave.
/// These tests verify the harness correctly detects violations, not the implementation.
fn is_negative_test(test_name: &str) -> bool {
    matches!(test_name, "handshake.missing_hello")
}

/// Check if a test is a stub (returns "test not implemented" in the harness).
/// This should be empty when all tests are implemented!
fn is_stub_test(_test_name: &str) -> bool {
    // All stub tests have been implemented!
    false
}

fn main() {
    let args = Arguments::from_args();

    // Get the path to the conformance binary
    let conformance_bin = env!("CARGO_BIN_EXE_rapace-conformance");

    // List all tests from the binary
    let output = Command::new(conformance_bin)
        .args(["--list", "--format", "json"])
        .output()
        .expect("failed to run conformance binary");

    if !output.status.success() {
        eprintln!(
            "Failed to list tests: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::process::exit(1);
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let tests: Vec<TestCase> = facet_json::from_str(&json_str).expect("failed to parse test list");

    // Create a Trial for each test
    let trials: Vec<Trial> = tests
        .into_iter()
        .map(|test| {
            let name = test.name.clone();
            let bin_path = conformance_bin.to_string();

            Trial::test(name.clone(), move || run_test(&bin_path, &name))
        })
        .collect();

    libtest_mimic::run(&args, trials).exit();
}

fn run_test(bin_path: &str, test_name: &str) -> Result<(), Failed> {
    // Stub tests are not implemented in the harness yet
    if is_stub_test(test_name) {
        return Err(Failed::from(
            "stub test: harness returns 'test not implemented'",
        ));
    }

    let is_negative = is_negative_test(test_name);

    if is_interactive_test(test_name) {
        // Interactive test - use ConformanceRunner
        let mut runner = ConformanceRunner::start(bin_path, test_name).map_err(Failed::from)?;

        // Run the test interaction
        let interaction_result = run_interactive_test(&mut runner, test_name);

        // Wait for conformance binary to finish
        let (passed, code) = runner.finish();

        // For negative tests, invert the result:
        // - Harness PASS means implementation misbehaved → our test should FAIL
        // - Harness FAIL means implementation behaved correctly → our test should PASS
        if is_negative {
            // Interaction errors may be expected for negative tests.
            // The harness exits after detecting correct behavior, which can cause EOF.
            if let Err(e) = interaction_result {
                // EOF-related errors are expected - harness exits after validation
                let is_eof_error = e.contains("eof")
                    || e.contains("EOF")
                    || e.contains("fill whole buffer")
                    || e.contains("UnexpectedEof")
                    || e.contains("end of file");
                if !is_eof_error {
                    return Err(Failed::from(format!(
                        "negative test interaction error: {}; exit code: {:?}",
                        e, code
                    )));
                }
            }

            if passed {
                // Harness passed = implementation misbehaved = BAD
                Err(Failed::from(
                    "negative test: harness passed, meaning implementation misbehaved",
                ))
            } else {
                // Harness failed = implementation behaved correctly = GOOD
                Ok(())
            }
        } else {
            // Normal test - check interaction result first
            if let Err(e) = interaction_result {
                return Err(Failed::from(format!(
                    "interaction error: {}; exit code: {:?}",
                    e, code
                )));
            }

            if passed {
                Ok(())
            } else {
                Err(Failed::from(format!(
                    "conformance test failed with exit code {:?}",
                    code
                )))
            }
        }
    } else {
        // Non-interactive test - just run and capture output
        let output = Command::new(bin_path)
            .args(["--case", test_name, "--format", "json"])
            .output()
            .map_err(|e| Failed::from(format!("failed to run test: {}", e)))?;

        let json_str = String::from_utf8_lossy(&output.stdout);

        #[derive(Facet)]
        struct TestResult {
            test: String,
            passed: bool,
            error: Option<String>,
        }

        let result: TestResult = facet_json::from_str(&json_str)
            .map_err(|e| Failed::from(format!("failed to parse result '{}': {}", json_str, e)))?;

        if result.passed {
            Ok(())
        } else {
            let error = result.error.as_deref().unwrap_or("unknown error");
            Err(Failed::from(error.to_string()))
        }
    }
}
